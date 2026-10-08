//! 外设模式：按键经 CDC 串口发给外设驱动，由外设驱动作为真实 USB 键鼠敲出。即插即用、不要管理员。
//!
//! 外设驱动是 xwink-body 仓 `hand/` 固件，协议以那边 `skills/xwink-body/references/engineering/firmware.md`
//! 「HID 棒固件」为准：一行一条 ASCII 指令，每条回一行（`OK` / `ERR …` / `PONG`）。这里逐条等回复：
//! 往返实测不到 1 ms，换来拔出、卡死、六键已满这类失败当场可见，而不是静默丢键。
//!
//! 外设驱动敲出的是真硬件事件，回到低级钩子时既没有 `SIM_MARKER` 也没有 injected 标志，自注入过滤与
//! DD 一样只能靠 `PENDING_INJECTIONS` 登记（见 `lib.rs` 的 `dispatch_hidstick`）。
//!
//! 防卡键分两端：固件在串口关闭（DTR 掉）或按着键 1 秒收不到指令时松开全部；这边断开时发
//! `RESET`。心跳只在没按着任何键时发——引擎单次按下最多 30 ms，按着键还在心跳就等于替卡死的
//! 引擎续命，把固件那道 1 秒保险废掉。

use qzh_profile::key_id::MouseButton;

/// 认外设驱动只看握手（`ID` 回 `ID xwink-hand …`），不认 VID/PID：PID 是固件自己定的、没登记，
/// 换板子或改编译配置就变；VID 是乐鑫全系共用，对上也说明不了什么。VID 只用来排先后与排除。
pub const ESPRESSIF_VID: u16 = 0x303A;

/// USB 转串口芯片：CH34x、CP210x、FTDI、PL2303。外设驱动的指令口是芯片原生 USB，不会挂在这些
/// 芯片上；而它们常把 DTR/RTS 接到开发板复位脚，握手要拉 DTR，一拉就把板子复位了——
/// 外设驱动自己的烧录口（CH343）就是这种。
pub const UART_BRIDGE_VIDS: [u16; 4] = [0x1A86, 0x10C4, 0x0403, 0x067B];

/// 从枚举到的 USB 串口 `(口名, VID)` 里挑握手候选：排除转串口芯片，乐鑫的排前面，其余保持枚举顺序。
pub fn pick_candidates(usb_ports: &[(String, u16)]) -> Vec<String> {
    let mut picked: Vec<&(String, u16)> = usb_ports
        .iter()
        .filter(|(_, vid)| !UART_BRIDGE_VIDS.contains(vid))
        .collect();
    picked.sort_by_key(|(_, vid)| *vid != ESPRESSIF_VID);
    picked.into_iter().map(|(name, _)| name.clone()).collect()
}

/// 诊断里一个 USB 串口的结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortVerdict {
    /// 握手认出是外设驱动
    Stick { firmware: String },
    /// 外设模式正开着它，诊断不去打扰
    InUse { firmware: String },
    /// 转串口芯片，按 [`UART_BRIDGE_VIDS`] 跳过
    Bridge,
    /// 打不开或不按外设驱动固件应答
    NotStick(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortScan {
    pub name: String,
    pub vid: u16,
    pub pid: u16,
    pub verdict: PortVerdict,
}

impl PortScan {
    pub fn is_stick(&self) -> bool {
        matches!(
            self.verdict,
            PortVerdict::Stick { .. } | PortVerdict::InUse { .. }
        )
    }

    pub fn describe(&self) -> String {
        let head = format!("{}（{:04X}:{:04X}）", self.name, self.vid, self.pid);
        match &self.verdict {
            PortVerdict::Stick { firmware } => format!("{head}：外设驱动，固件 {firmware}"),
            PortVerdict::InUse { firmware } => {
                format!("{head}：外设模式正在使用，固件 {firmware}")
            }
            PortVerdict::Bridge => format!("{head}：转串口芯片，跳过（握手会复位开发板）"),
            PortVerdict::NotStick(why) => format!("{head}：不是外设驱动（{why}）"),
        }
    }
}

/// Interception 的设备槽：1..=10 键盘、11..=20 鼠标。
pub const KEYBOARD_SLOTS: std::ops::RangeInclusive<i32> = 1..=10;
pub const MOUSE_SLOTS: std::ops::RangeInclusive<i32> = 11..=20;

/// 在 Interception 设备槽 `(槽号, 硬件 ID)` 里找某个 USB 设备，返回它占的槽号。复合设备的
/// 每个接口各占一槽，硬件 ID 形如 `HID\VID_303A&PID_4849&REV_0100&MI_02&Col01`，按 VID/PID 认。
pub fn slot_of(
    slots: &[(i32, String)],
    range: std::ops::RangeInclusive<i32>,
    vid: u16,
    pid: u16,
) -> Option<i32> {
    let needle = format!("VID_{vid:04X}&PID_{pid:04X}");
    slots
        .iter()
        .find(|(slot, id)| range.contains(slot) && id.to_ascii_uppercase().contains(&needle))
        .map(|(slot, _)| *slot)
}

/// 自动握手（启动与插拔监视）只碰乐鑫的口：用户没点外设模式时，不该去拉别人设备的 DTR——
/// Arduino Uno 一类板子靠 DTR 自动复位，每次启动都会被重启一遍。别家芯片的外设驱动要手动选一次
/// 外设模式，那时走 [`pick_candidates`] 全量握手。
pub fn pick_auto_candidates(usb_ports: &[(String, u16)]) -> Vec<String> {
    usb_ports
        .iter()
        .filter(|(_, vid)| *vid == ESPRESSIF_VID)
        .map(|(name, _)| name.clone())
        .collect()
}

/// 插拔监视的状态：轮询拿到候选口后喂给 [`StickTracker::poll`]，只对新出现的口握一次手。
///
/// 每个口只在出现时握一次：握手要开口拉 DTR，对别的设备反复做没有好处；被别的程序占着而没握上的，
/// 拔插一次再认。外设驱动用着的口不会被重新打开——它早已不是「新出现」的。
#[derive(Debug, Default)]
pub struct StickTracker {
    seen: std::collections::HashSet<String>,
    stick: Option<String>,
}

impl StickTracker {
    /// 返回连接态的翻转：`Some(true)` 新认到外设驱动，`Some(false)` 它所在的口消失了。
    pub fn poll(
        &mut self,
        candidates: &[String],
        mut probe: impl FnMut(&str) -> bool,
    ) -> Option<bool> {
        let mut change = None;
        if let Some(port) = &self.stick {
            if !candidates.contains(port) {
                self.stick = None;
                change = Some(false);
            }
        }
        if self.stick.is_none() {
            let fresh = candidates.iter().filter(|c| !self.seen.contains(*c));
            for port in fresh {
                if probe(port) {
                    self.stick = Some(port.clone());
                    // 同一轮里拔了又插到别的口，对外仍是「连着」，不报翻转
                    change = if change == Some(false) {
                        None
                    } else {
                        Some(true)
                    };
                    break;
                }
            }
        }
        self.seen = candidates.iter().cloned().collect();
        change
    }

    pub fn connected(&self) -> bool {
        self.stick.is_some()
    }

    /// 链路断了但口还在（固件卡死、或口刚被释放）：忘掉它和见过的口，下一轮重新握手。
    pub fn forget(&mut self) {
        self.stick = None;
        self.seen.clear();
    }
}

/// Windows VK → HID 键盘页 usage（HID Usage Tables 第 10 章）。
///
/// 只收键盘页：媒体键在 Consumer 页，固件的键盘报文发不出；小键盘回车与主回车同为 `VK_RETURN`，
/// 光凭 VK 分不出，一律当主回车。返回 `None` 的键由调用方回退 SendInput。
pub fn vk_to_usage(vk: u32) -> Option<u8> {
    let usage = match vk {
        0x41..=0x5A => 0x04 + (vk - 0x41) as u8, // A..Z
        0x31..=0x39 => 0x1E + (vk - 0x31) as u8, // 1..9
        0x30 => 0x27,                            // 0
        0x0D => 0x28,                            // Enter
        0x1B => 0x29,                            // Esc
        0x08 => 0x2A,                            // Backspace
        0x09 => 0x2B,                            // Tab
        0x20 => 0x2C,                            // Space
        0xBD => 0x2D,                            // -
        0xBB => 0x2E,                            // =
        0xDB => 0x2F,                            // [
        0xDD => 0x30,                            // ]
        0xDC => 0x31,                            // \
        0xBA => 0x33,                            // ;
        0xDE => 0x34,                            // '
        0xC0 => 0x35,                            // `
        0xBC => 0x36,                            // ,
        0xBE => 0x37,                            // .
        0xBF => 0x38,                            // /
        0x14 => 0x39,                            // CapsLock
        0x70..=0x7B => 0x3A + (vk - 0x70) as u8, // F1..F12
        0x2C => 0x46,                            // PrintScreen
        0x91 => 0x47,                            // ScrollLock
        0x13 => 0x48,                            // Pause
        0x2D => 0x49,                            // Insert
        0x24 => 0x4A,                            // Home
        0x21 => 0x4B,                            // PageUp
        0x2E => 0x4C,                            // Delete
        0x23 => 0x4D,                            // End
        0x22 => 0x4E,                            // PageDown
        0x27 => 0x4F,                            // Right
        0x25 => 0x50,                            // Left
        0x28 => 0x51,                            // Down
        0x26 => 0x52,                            // Up
        0x90 => 0x53,                            // NumLock
        0x6F => 0x54,                            // 小键盘 /
        0x6A => 0x55,                            // 小键盘 *
        0x6D => 0x56,                            // 小键盘 -
        0x6B => 0x57,                            // 小键盘 +
        0x61..=0x69 => 0x59 + (vk - 0x61) as u8, // 小键盘 1..9
        0x60 => 0x62,                            // 小键盘 0
        0x6E => 0x63,                            // 小键盘 .
        0xE2 => 0x64,                            // ISO 键盘 \ 与 |
        0x5D => 0x65,                            // Menu
        0x7C..=0x87 => 0x68 + (vk - 0x7C) as u8, // F13..F24
        0x11 | 0xA2 => 0xE0,                     // 左 Ctrl（不分左右的 VK 归左）
        0x10 | 0xA0 => 0xE1,                     // 左 Shift
        0x12 | 0xA4 => 0xE2,                     // 左 Alt
        0x5B => 0xE3,                            // 左 Win
        0xA3 => 0xE4,                            // 右 Ctrl
        0xA1 => 0xE5,                            // 右 Shift
        0xA5 => 0xE6,                            // 右 Alt
        0x5C => 0xE7,                            // 右 Win
        _ => return None,
    };
    Some(usage)
}

pub fn key_command(usage: u8, is_up: bool) -> String {
    format!("{} 0x{usage:02X}", if is_up { "KU" } else { "KD" })
}

/// 滚轮不是按键，没有按下 / 松开，返回 `None`，由 [`wheel_command`] 处理。
pub fn mouse_command(button: MouseButton, is_up: bool) -> Option<String> {
    let name = match button {
        MouseButton::Left => "left",
        MouseButton::Right => "right",
        MouseButton::Middle => "middle",
        MouseButton::X1 => "x1",
        MouseButton::X2 => "x2",
        MouseButton::WheelUp | MouseButton::WheelDown => return None,
    };
    Some(format!("{} {name}", if is_up { "MU" } else { "MD" }))
}

pub fn wheel_command(up: bool) -> &'static str {
    if up {
        "MW 1"
    } else {
        "MW -1"
    }
}

/// 固件 `ID` 的回复：`ID xwink-hand <git describe> <USB 序列号>`。返回固件版本。
pub fn parse_id_reply(line: &str) -> Option<&str> {
    let mut parts = line.split_whitespace();
    if parts.next()? != "ID" || parts.next()? != "xwink-hand" {
        return None;
    }
    parts.next()
}

/// 一次下发的结果。`Unmapped` 不算失败：键不在 HID 键盘页，调用方改走 SendInput。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidSend {
    Sent,
    Unmapped,
    Failed,
}

#[cfg(windows)]
pub use port::{auto_candidate_ports, candidate_ports, probe, scan_ports, HidStickBackend};

#[cfg(windows)]
mod port {
    use super::{key_command, mouse_command, parse_id_reply, vk_to_usage, wheel_command, HidSend};
    use qzh_profile::key_id::MouseButton;
    use serialport::{ClearBuffer, SerialPort, SerialPortType};
    use std::collections::HashSet;
    use std::io::{Read, Write};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};
    use tracing::{info, warn};

    const BAUD: u32 = 115_200; // CDC 不看波特率，只是 open 需要一个值
    const REPLY_TIMEOUT: Duration = Duration::from_millis(200);
    const HEARTBEAT_EVERY: Duration = Duration::from_millis(500);
    const HEARTBEAT_TICK: Duration = Duration::from_millis(100);
    /// 握手时最多跳过几行旧回复：刚打开时缓冲里可能还有上一个会话没读走的
    const ID_MAX_LINES: usize = 4;
    /// 一行回复的总时限与长度上限。单次 read 的超时挡不住一直发数据却不换行的设备：
    /// 不设总时限，握手会永远卡在那儿、缓冲无限长。
    const LINE_DEADLINE: Duration = Duration::from_millis(500);
    const LINE_MAX: usize = 256;

    /// 只看 USB 串口：板载 COM1 不碰，蓝牙虚拟串口打开可能卡好几秒。
    fn usb_ports() -> Vec<(String, u16)> {
        serialport::available_ports()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|p| match p.port_type {
                SerialPortType::UsbPort(usb) => Some((p.port_name, usb.vid)),
                _ => None,
            })
            .collect()
    }

    /// 手动选外设模式时的握手候选，规则见 [`super::pick_candidates`]。
    pub fn candidate_ports() -> Vec<String> {
        super::pick_candidates(&usb_ports())
    }

    /// 自动握手的候选，规则见 [`super::pick_auto_candidates`]。
    pub fn auto_candidate_ports() -> Vec<String> {
        super::pick_auto_candidates(&usb_ports())
    }

    /// 打开口并握手，成功返回连接与固件版本。
    fn connect(port_name: &str) -> Result<(Link, String), String> {
        let mut port = serialport::new(port_name, BAUD)
            .timeout(REPLY_TIMEOUT)
            .open()
            .map_err(|e| format!("打开 {port_name} 失败: {e}"))?;
        // 固件以 DTR 判断主机在不在：没 DTR 不回话，DTR 掉即松开全部键
        port.write_data_terminal_ready(true)
            .map_err(|e| format!("置 DTR 失败: {e}"))?;
        let _ = port.clear(ClearBuffer::Input);

        let mut link = Link {
            port,
            buf: Vec::new(),
            held: HeldSet::new(),
            last_io: Instant::now(),
        };
        link.port
            .write_all(b"ID\n")
            .map_err(|e| format!("写 {port_name} 失败: {e}"))?;
        for _ in 0..ID_MAX_LINES {
            // 读超时就是不认这套协议，不必再等剩下几行
            let line = link
                .read_line()
                .map_err(|e| format!("{port_name} 没有按外设驱动固件应答（{e}）"))?;
            if let Some(firmware) = parse_id_reply(&line) {
                let firmware = firmware.to_string();
                return Ok((link, firmware));
            }
        }
        Err(format!("{port_name} 没有按外设驱动固件应答"))
    }

    /// 诊断用的全量扫描：每个 USB 串口都给结论，转串口芯片照样不碰。`in_use` 是外设模式正
    /// 开着的 `(口名, 固件)`，那个口独占打不开，直接报「正在使用」。
    pub fn scan_ports(in_use: Option<(&str, &str)>) -> Vec<super::PortScan> {
        use super::{PortScan, PortVerdict, UART_BRIDGE_VIDS};
        serialport::available_ports()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|p| match p.port_type {
                SerialPortType::UsbPort(usb) => Some((p.port_name, usb.vid, usb.pid)),
                _ => None,
            })
            .map(|(name, vid, pid)| {
                let verdict = match in_use {
                    Some((port, firmware)) if port == name => PortVerdict::InUse {
                        firmware: firmware.to_string(),
                    },
                    _ if UART_BRIDGE_VIDS.contains(&vid) => PortVerdict::Bridge,
                    _ => match connect(&name) {
                        Ok((_, firmware)) => PortVerdict::Stick { firmware },
                        Err(e) => PortVerdict::NotStick(e),
                    },
                };
                PortScan {
                    name,
                    vid,
                    pid,
                    verdict,
                }
            })
            .collect()
    }

    /// 只握手不留连接，给插拔监视用；返回固件版本。关口时 DTR 掉，固件会松开全部键，空闲时无害。
    pub fn probe(port_name: &str) -> Option<String> {
        connect(port_name).ok().map(|(_, firmware)| firmware)
    }

    /// 按下未松的键：键盘用 usage，鼠标键用 `0x100 | 序号`，供心跳判断「是否按着键」与断开时对账。
    type HeldSet = HashSet<u16>;

    struct Link {
        port: Box<dyn SerialPort>,
        buf: Vec<u8>,
        held: HeldSet,
        last_io: Instant,
    }

    impl Link {
        fn read_line(&mut self) -> Result<String, String> {
            let deadline = Instant::now() + LINE_DEADLINE;
            loop {
                if let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
                    let line: Vec<u8> = self.buf.drain(..=pos).collect();
                    return Ok(String::from_utf8_lossy(&line).trim().to_string());
                }
                if self.buf.len() > LINE_MAX || Instant::now() > deadline {
                    self.buf.clear();
                    return Err("回复不是一行一条的文本".to_string());
                }
                let mut chunk = [0u8; 128];
                match self.port.read(&mut chunk) {
                    Ok(0) => return Err("串口已关闭".to_string()),
                    Ok(n) => self.buf.extend_from_slice(&chunk[..n]),
                    Err(e) => return Err(format!("等回复失败: {e}")),
                }
            }
        }

        fn request(&mut self, cmd: &str) -> Result<String, String> {
            self.last_io = Instant::now();
            self.port
                .write_all(format!("{cmd}\n").as_bytes())
                .map_err(|e| format!("写串口失败: {e}"))?;
            self.read_line()
        }
    }

    struct Shared {
        link: Mutex<Link>,
        online: AtomicBool,
        stop: AtomicBool,
        on_offline: fn(),
    }

    impl Shared {
        fn lock(&self) -> std::sync::MutexGuard<'_, Link> {
            self.link.lock().unwrap_or_else(|e| e.into_inner())
        }

        fn go_offline(&self, reason: &str) {
            if self.online.swap(false, Ordering::SeqCst) {
                warn!("外设驱动断开：{reason}");
                (self.on_offline)();
            }
        }

        /// 下发一条按键类指令。`held_id` 为 `None` 表示没有按下态（滚轮）。
        fn send(&self, cmd: &str, held_id: Option<u16>, is_up: bool) -> HidSend {
            if !self.online.load(Ordering::SeqCst) {
                return HidSend::Failed;
            }
            let mut link = self.lock();
            match link.request(cmd) {
                Ok(reply) if reply.eq_ignore_ascii_case("OK") => {
                    if let Some(id) = held_id {
                        if is_up {
                            link.held.remove(&id);
                        } else {
                            link.held.insert(id);
                        }
                    }
                    HidSend::Sent
                }
                // 固件拒绝（如六个普通键已按满）不是断开，只这一条不生效。松开被拒也当它松了：
                // 留在 held 里心跳就再也不发，断线只能等下一次下发才发现
                Ok(reply) => {
                    warn!("外设驱动拒绝指令 `{cmd}`：{reply}");
                    if let (Some(id), true) = (held_id, is_up) {
                        link.held.remove(&id);
                    }
                    HidSend::Failed
                }
                Err(e) => {
                    drop(link);
                    self.go_offline(&e);
                    HidSend::Failed
                }
            }
        }
    }

    pub struct HidStickBackend {
        shared: Arc<Shared>,
        heartbeat: Option<JoinHandle<()>>,
        port_name: String,
        firmware: String,
    }

    impl HidStickBackend {
        /// 挨个候选口握手，第一个应答的就是；再松开残留。`on_offline` 在之后任何一次断开时
        /// 调用一次（下发线程或心跳线程）。
        pub fn open(on_offline: fn()) -> Result<Self, String> {
            let candidates = candidate_ports();
            if candidates.is_empty() {
                return Err("没有可握手的 USB 串口".to_string());
            }
            let mut failures = Vec::new();
            let (mut link, port_name, firmware) = candidates
                .into_iter()
                .find_map(|name| match connect(&name) {
                    Ok((link, firmware)) => Some((link, name, firmware)),
                    Err(e) => {
                        failures.push(e);
                        None
                    }
                })
                .ok_or_else(|| format!("没有口按外设驱动固件应答：{}", failures.join("；")))?;
            link.request("RESET")?;

            let shared = Arc::new(Shared {
                link: Mutex::new(link),
                online: AtomicBool::new(true),
                stop: AtomicBool::new(false),
                on_offline,
            });
            let heartbeat = {
                let shared = shared.clone();
                std::thread::Builder::new()
                    .name("hidstick-heartbeat".into())
                    .spawn(move || heartbeat_loop(&shared))
                    .map_err(|e| format!("启动心跳线程失败: {e}"))?
            };
            info!("外设驱动已连接：{port_name}，固件 {firmware}");
            Ok(Self {
                shared,
                heartbeat: Some(heartbeat),
                port_name,
                firmware,
            })
        }

        pub fn port_name(&self) -> &str {
            &self.port_name
        }

        pub fn firmware(&self) -> &str {
            &self.firmware
        }

        pub fn is_online(&self) -> bool {
            self.shared.online.load(Ordering::SeqCst)
        }

        /// 外面先发现了断开（插拔监视看到口没了）：走同一条断开路径，心跳不必再等。
        pub fn link_lost(&self, reason: &str) {
            self.shared.go_offline(reason);
        }

        pub fn send_key(&self, vk: u32, is_up: bool) -> HidSend {
            let Some(usage) = vk_to_usage(vk) else {
                return HidSend::Unmapped;
            };
            self.shared
                .send(&key_command(usage, is_up), Some(u16::from(usage)), is_up)
        }

        pub fn send_mouse(&self, button: MouseButton, is_up: bool) -> HidSend {
            let Some(cmd) = mouse_command(button, is_up) else {
                return HidSend::Unmapped;
            };
            self.shared.send(&cmd, Some(0x100 | button as u16), is_up)
        }

        pub fn send_wheel(&self, up: bool) -> HidSend {
            self.shared.send(wheel_command(up), None, false)
        }
    }

    impl Drop for HidStickBackend {
        fn drop(&mut self) {
            self.shared.stop.store(true, Ordering::SeqCst);
            if let Some(handle) = self.heartbeat.take() {
                let _ = handle.join();
            }
            // 切走模式时引擎已经松过键，这里再兜一次；之后关口 DTR 掉，固件也会松
            if self.shared.online.swap(false, Ordering::SeqCst) {
                let _ = self.shared.lock().request("RESET");
            }
            info!("外设驱动后端已释放（{}）", self.port_name);
        }
    }

    fn heartbeat_loop(shared: &Shared) {
        while !shared.stop.load(Ordering::SeqCst) && shared.online.load(Ordering::SeqCst) {
            std::thread::sleep(HEARTBEAT_TICK);
            let mut link = shared.lock();
            if !link.held.is_empty() || link.last_io.elapsed() < HEARTBEAT_EVERY {
                continue;
            }
            let result = link.request("PING");
            drop(link);
            match result {
                Ok(reply) if reply.eq_ignore_ascii_case("PONG") => {}
                Ok(reply) => shared.go_offline(&format!("心跳回复异常：{reply}")),
                Err(e) => shared.go_offline(&e),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_digits_and_symbols_map_to_keyboard_page() {
        assert_eq!(vk_to_usage(0x41), Some(0x04)); // A
        assert_eq!(vk_to_usage(0x5A), Some(0x1D)); // Z
        assert_eq!(vk_to_usage(0x31), Some(0x1E)); // 1
        assert_eq!(vk_to_usage(0x39), Some(0x26)); // 9
        assert_eq!(vk_to_usage(0x30), Some(0x27)); // 0
        assert_eq!(vk_to_usage(0x20), Some(0x2C)); // Space
        assert_eq!(vk_to_usage(0xC0), Some(0x35)); // `
        assert_eq!(vk_to_usage(0xBF), Some(0x38)); // /
    }

    #[test]
    fn function_keys_cover_f1_to_f24() {
        assert_eq!(vk_to_usage(0x70), Some(0x3A)); // F1
        assert_eq!(vk_to_usage(0x7B), Some(0x45)); // F12
        assert_eq!(vk_to_usage(0x7C), Some(0x68)); // F13
        assert_eq!(vk_to_usage(0x87), Some(0x73)); // F24
    }

    #[test]
    fn modifiers_keep_left_right_and_generic_falls_to_left() {
        assert_eq!(vk_to_usage(0xA0), Some(0xE1)); // LShift
        assert_eq!(vk_to_usage(0xA1), Some(0xE5)); // RShift
        assert_eq!(vk_to_usage(0xA3), Some(0xE4)); // RCtrl
        assert_eq!(vk_to_usage(0xA5), Some(0xE6)); // RAlt
        assert_eq!(vk_to_usage(0x5C), Some(0xE7)); // RWin
        assert_eq!(vk_to_usage(0x10), Some(0xE1)); // Shift
        assert_eq!(vk_to_usage(0x11), Some(0xE0)); // Ctrl
        assert_eq!(vk_to_usage(0x12), Some(0xE2)); // Alt
    }

    #[test]
    fn numpad_maps_separately_from_main_row() {
        assert_eq!(vk_to_usage(0x60), Some(0x62)); // 小键盘 0
        assert_eq!(vk_to_usage(0x61), Some(0x59)); // 小键盘 1
        assert_eq!(vk_to_usage(0x69), Some(0x61)); // 小键盘 9
        assert_eq!(vk_to_usage(0x6E), Some(0x63)); // 小键盘 .
        assert_eq!(vk_to_usage(0x6B), Some(0x57)); // 小键盘 +
    }

    #[test]
    fn navigation_cluster_maps() {
        assert_eq!(vk_to_usage(0x26), Some(0x52)); // Up
        assert_eq!(vk_to_usage(0x28), Some(0x51)); // Down
        assert_eq!(vk_to_usage(0x2E), Some(0x4C)); // Delete
        assert_eq!(vk_to_usage(0x22), Some(0x4E)); // PageDown
    }

    #[test]
    fn consumer_page_keys_are_unmapped() {
        assert_eq!(vk_to_usage(0xAD), None); // 静音
        assert_eq!(vk_to_usage(0xB3), None); // 播放 / 暂停
        assert_eq!(vk_to_usage(0x00), None);
    }

    #[test]
    fn commands_follow_firmware_line_protocol() {
        assert_eq!(key_command(0x04, false), "KD 0x04");
        assert_eq!(key_command(0xE5, true), "KU 0xE5");
        assert_eq!(
            mouse_command(MouseButton::Left, false).as_deref(),
            Some("MD left")
        );
        assert_eq!(
            mouse_command(MouseButton::X2, true).as_deref(),
            Some("MU x2")
        );
        assert_eq!(mouse_command(MouseButton::WheelUp, false), None);
        assert_eq!(wheel_command(true), "MW 1");
        assert_eq!(wheel_command(false), "MW -1");
    }

    fn ports(list: &[(&str, u16)]) -> Vec<(String, u16)> {
        list.iter().map(|(n, v)| (n.to_string(), *v)).collect()
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn candidates_skip_uart_bridges_and_put_espressif_first() {
        let found = pick_candidates(&ports(&[
            ("COM6", 0x1A86),  // CH343：外设驱动自己的烧录口
            ("COM3", 0x2341),  // Arduino 原生 USB
            ("COM9", 0x303A),  // 乐鑫原生 USB
            ("COM4", 0x10C4),  // CP210x
            ("COM12", 0x303A), // 另一块乐鑫板
        ]));
        assert_eq!(found, names(&["COM9", "COM12", "COM3"]));
    }

    #[test]
    fn slot_lookup_matches_vid_pid_within_range() {
        let slots = vec![
            (1, "HID\\VID_1532&PID_0098&REV_0200&MI_01&Col01".to_string()),
            (5, "HID\\VID_04F2&PID_1666&REV_0048&MI_01".to_string()),
            (
                18,
                "HID\\VID_303A&PID_4849&REV_0100&MI_02&Col02".to_string(),
            ),
        ];
        // 外设驱动的鼠标占了 18 号，键盘没分到槽
        assert_eq!(slot_of(&slots, MOUSE_SLOTS, 0x303A, 0x4849), Some(18));
        assert_eq!(slot_of(&slots, KEYBOARD_SLOTS, 0x303A, 0x4849), None);
        assert_eq!(slot_of(&slots, KEYBOARD_SLOTS, 0x04F2, 0x1666), Some(5));
        // 小写硬件 ID 也认
        let lower = vec![(3, "hid\\vid_303a&pid_4849&mi_02&col01".to_string())];
        assert_eq!(slot_of(&lower, KEYBOARD_SLOTS, 0x303A, 0x4849), Some(3));
    }

    #[test]
    fn port_scan_describes_each_verdict() {
        let scan = |verdict| PortScan {
            name: "COM9".to_string(),
            vid: 0x303A,
            pid: 0x4849,
            verdict,
        };
        let stick = scan(PortVerdict::Stick {
            firmware: "b5a8bacc".to_string(),
        });
        assert!(stick.is_stick());
        assert_eq!(
            stick.describe(),
            "COM9（303A:4849）：外设驱动，固件 b5a8bacc"
        );
        assert!(scan(PortVerdict::InUse {
            firmware: "x".to_string()
        })
        .is_stick());
        assert!(!scan(PortVerdict::Bridge).is_stick());
        assert!(scan(PortVerdict::NotStick("拒绝访问".to_string()))
            .describe()
            .ends_with("不是外设驱动（拒绝访问）"));
    }

    #[test]
    fn auto_candidates_only_take_espressif_ports() {
        let found = pick_auto_candidates(&ports(&[
            ("COM3", 0x2341), // Arduino Uno：拉 DTR 会复位，自动握手不碰
            ("COM9", 0x303A),
            ("COM6", 0x1A86),
        ]));
        assert_eq!(found, names(&["COM9"]));
    }

    #[test]
    fn tracker_forget_reprobes_the_same_port() {
        let mut tracker = StickTracker::default();
        assert_eq!(tracker.poll(&names(&["COM9"]), |_| true), Some(true));
        tracker.forget();
        assert!(!tracker.connected());
        // 口一直在，忘掉之后下一轮会重新握手
        assert_eq!(tracker.poll(&names(&["COM9"]), |_| true), Some(true));
    }

    #[test]
    fn tracker_probes_each_new_port_once() {
        let mut tracker = StickTracker::default();
        let mut probed = Vec::new();
        let none = tracker.poll(&names(&["COM3"]), |p| {
            probed.push(p.to_string());
            false
        });
        assert_eq!(none, None);
        // 没握上的口还在，不再打扰它
        assert_eq!(
            tracker.poll(&names(&["COM3"]), |_| panic!("不该重握")),
            None
        );
        assert_eq!(probed, names(&["COM3"]));
    }

    #[test]
    fn tracker_reports_connect_and_disconnect_once() {
        let mut tracker = StickTracker::default();
        assert_eq!(tracker.poll(&names(&["COM9"]), |p| p == "COM9"), Some(true));
        assert!(tracker.connected());
        // 连着时不碰它用着的口
        assert_eq!(
            tracker.poll(&names(&["COM9", "COM3"]), |_| panic!("连着时不握手")),
            None
        );
        assert_eq!(tracker.poll(&names(&["COM3"]), |_| false), Some(false));
        assert!(!tracker.connected());
        // 插回同一个口名：重新出现即重新握手
        assert_eq!(
            tracker.poll(&names(&["COM3", "COM9"]), |p| p == "COM9"),
            Some(true)
        );
    }

    #[test]
    fn tracker_moving_ports_in_one_poll_is_not_a_flip() {
        let mut tracker = StickTracker::default();
        assert_eq!(tracker.poll(&names(&["COM9"]), |p| p == "COM9"), Some(true));
        assert_eq!(tracker.poll(&names(&["COM10"]), |p| p == "COM10"), None);
        assert!(tracker.connected());
    }

    /// 真机：`cargo test -p win-input hidstick -- --ignored --test-threads=1`（两个真机测试并行会抢同一个口）。只敲 F13（没有程序绑它），不动鼠标。
    #[cfg(windows)]
    #[test]
    #[ignore = "需要插着外设驱动"]
    fn real_stick_types_f13_and_releases() {
        fn noop() {}
        let ports = candidate_ports();
        assert!(
            ports.iter().any(|p| probe(p).is_some()),
            "候选口里没握到外设驱动：{ports:?}"
        );
        let backend = HidStickBackend::open(noop).expect("打开外设驱动");
        assert!(!backend.firmware().is_empty());
        assert_eq!(backend.send_key(0x7C, false), HidSend::Sent);
        assert_eq!(backend.send_key(0x7C, true), HidSend::Sent);
        assert_eq!(backend.send_key(0xAD, false), HidSend::Unmapped);
        // 空闲超过一个心跳周期，心跳不能把连接判成断开
        std::thread::sleep(std::time::Duration::from_millis(1200));
        assert_eq!(backend.send_key(0x7C, false), HidSend::Sent);
        assert_eq!(backend.send_key(0x7C, true), HidSend::Sent);
    }

    /// 真机：诊断修复那两项会看到什么。只握手、读槽位，不发键。
    /// `cargo test -p win-input real_scan -- --ignored --nocapture --test-threads=1`
    #[cfg(windows)]
    #[test]
    #[ignore = "需要插着外设驱动"]
    fn real_scan_reports_ports_and_slots() {
        let scans = scan_ports(None);
        for scan in &scans {
            println!("{}", scan.describe());
        }
        let stick = scans.iter().find(|s| s.is_stick()).expect("没扫到外设驱动");
        assert!(scans
            .iter()
            .filter(|s| UART_BRIDGE_VIDS.contains(&s.vid))
            .all(|s| s.verdict == PortVerdict::Bridge));
        match crate::interception::device_hardware_ids() {
            None => println!("Interception 未运行"),
            Some(slots) => println!(
                "键盘槽 {:?}，鼠标槽 {:?}",
                slot_of(&slots, KEYBOARD_SLOTS, stick.vid, stick.pid),
                slot_of(&slots, MOUSE_SLOTS, stick.vid, stick.pid)
            ),
        }
    }

    #[test]
    fn id_reply_must_come_from_hand_firmware() {
        assert_eq!(
            parse_id_reply("ID xwink-hand b5a8bacc 84C7BB581CB4"),
            Some("b5a8bacc")
        );
        assert_eq!(parse_id_reply("ID other-board v1 x"), None);
        assert_eq!(parse_id_reply("OK"), None);
        assert_eq!(parse_id_reply("ID xwink-hand"), None);
    }
}
