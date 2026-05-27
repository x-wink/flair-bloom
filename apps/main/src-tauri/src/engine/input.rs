#[cfg(windows)]
use super::ddhid::DdHidBackend;
#[cfg(windows)]
use super::interception::InterceptionBackend;
#[cfg(windows)]
use std::collections::{HashMap, VecDeque};
#[cfg(windows)]
use std::path::PathBuf;
#[cfg(windows)]
use std::sync::atomic::AtomicBool;
#[cfg(windows)]
use std::sync::{Mutex, OnceLock};
#[cfg(windows)]
use std::time::{Duration, Instant};
#[cfg(windows)]
use tracing::{info, warn};
#[cfg(windows)]
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MAPVK_VK_TO_VSC_EX,
};

/// 写入 SendInput 的 dwExtraInfo，hook 据此过滤程序自身模拟的按键，消除竞态。
/// Interception 后端会把 `information` 字段透传到 `KBDLLHOOKSTRUCT.dwExtraInfo`。
/// DD-HID 后端无法控制此字段，改用应用层注入事件队列（PENDING_INJECTIONS）作为
/// SIM_MARKER 的等价机制。
pub const SIM_MARKER: usize = 0x5148_5844;

/// DD-HID 模式下注入事件的存活窗口。注入到 LL 钩子的真实延迟为 μs 级，50ms 留出
/// 几个数量级余量；超过即视为该 sim 事件被外部钩子吞掉，避免 PENDING 永不归零
/// 把后续物理事件错认成 sim。
#[cfg(windows)]
const SIM_TTL: Duration = Duration::from_millis(50);

/// `(vk, is_up) -> Instant 队列`：记录 DD-HID 后端注入的每次 down/up，hook 收到对应
/// 事件时按 FIFO 配对消费。down 与 up 分桶避免计数串扰。
#[cfg(windows)]
type PendingMap = HashMap<(u32, bool), VecDeque<Instant>>;
#[cfg(windows)]
static PENDING_INJECTIONS: OnceLock<Mutex<PendingMap>> = OnceLock::new();

#[cfg(windows)]
fn pending_map() -> &'static Mutex<PendingMap> {
    PENDING_INJECTIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 关键路径（hook 回调 + 连发线程）的锁兜底：若前一持锁者 panic 导致 Mutex
/// 中毒,强行复活并继续。按键工具最差故障是键盘卡死,值得给一层硬兜底；
/// 实际上当前所有持锁期间仅执行 HashMap/Option 操作,正常路径下不会中毒。
#[cfg(windows)]
fn revive<T>(r: std::sync::LockResult<T>) -> T {
    r.unwrap_or_else(|e| e.into_inner())
}

#[cfg(windows)]
fn drop_expired(queue: &mut VecDeque<Instant>, now: Instant) {
    while let Some(&front) = queue.front() {
        if now.duration_since(front) > SIM_TTL {
            queue.pop_front();
        } else {
            break;
        }
    }
}

#[cfg(windows)]
fn record_injection(vk: u32, is_up: bool) {
    let mut map = revive(pending_map().lock());
    let queue = map.entry((vk, is_up)).or_default();
    let now = Instant::now();
    drop_expired(queue, now);
    queue.push_back(now);
}

/// hook 端调用：若该 (vk, is_up) 对应有未过期的 sim 记录，pop 一条并返回 true
/// 表示这是程序自身注入的事件，应该被过滤掉。
#[cfg(windows)]
pub fn try_consume_injection(vk: u32, is_up: bool) -> bool {
    let mut map = revive(pending_map().lock());
    let Some(queue) = map.get_mut(&(vk, is_up)) else {
        return false;
    };
    drop_expired(queue, Instant::now());
    queue.pop_front().is_some()
}

/// 引擎重新设置规则、关闭全局开关或析构时清空，避免遗留记录把首个物理事件吞掉。
#[cfg(windows)]
pub fn clear_pending_injections() {
    if let Some(lock) = PENDING_INJECTIONS.get() {
        revive(lock.lock()).clear();
    }
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputMode {
    #[default]
    SendInput,
    Interception,
    DdHid,
}

#[cfg(windows)]
impl InputMode {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "sendinput" => Some(Self::SendInput),
            "interception" => Some(Self::Interception),
            "dd_hid" => Some(Self::DdHid),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SendInput => "sendinput",
            Self::Interception => "interception",
            Self::DdHid => "dd_hid",
        }
    }

    /// DD-HID 模式无法用 dwExtraInfo 过滤自身注入，但 Hold 模式靠 PENDING_INJECTIONS
    /// 队列识别 sim 事件，已经允许「目标键 == 触发键」。Toggle 模式则因为 sim KEYDOWN
    /// 必然要让 hook 处理（toggle 的本意），无法过滤自身，故仍禁止 target == trigger/stop。
    pub fn requires_distinct_target_for_toggle(&self) -> bool {
        matches!(self, Self::DdHid)
    }

    /// DD-HID 模式底层调用 DeviceIoControl，需要管理员权限。
    pub fn requires_admin(&self) -> bool {
        matches!(self, Self::DdHid)
    }
}

#[cfg(windows)]
static INTERCEPTION_BACKEND: OnceLock<Mutex<Option<InterceptionBackend>>> = OnceLock::new();
#[cfg(windows)]
static DD_HID_BACKEND: OnceLock<Mutex<Option<DdHidBackend>>> = OnceLock::new();

#[cfg(windows)]
static CURRENT_MODE: OnceLock<std::sync::atomic::AtomicU8> = OnceLock::new();

#[cfg(windows)]
static RESOURCES_DIR: OnceLock<PathBuf> = OnceLock::new();

/// 仅在第一次进入 DD 路径时打印一次「key_down 命中 DD 后端」诊断，避免连发循环刷屏
#[cfg(windows)]
static DD_KEY_DOWN_LOGGED: AtomicBool = AtomicBool::new(false);
#[cfg(windows)]
static DD_KEY_UP_LOGGED: AtomicBool = AtomicBool::new(false);
#[cfg(windows)]
static DD_FALLBACK_LOGGED: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
const MODE_SENDINPUT: u8 = 0;
#[cfg(windows)]
const MODE_INTERCEPTION: u8 = 1;
#[cfg(windows)]
const MODE_DD_HID: u8 = 2;

#[cfg(windows)]
fn u8_to_mode(v: u8) -> InputMode {
    match v {
        MODE_INTERCEPTION => InputMode::Interception,
        MODE_DD_HID => InputMode::DdHid,
        _ => InputMode::SendInput,
    }
}

/// 注册资源目录（由 lib.rs 在启动时调用一次），供 DD DLL 定位
#[cfg(windows)]
pub fn set_resources_dir(dir: PathBuf) {
    let _ = RESOURCES_DIR.set(dir);
}

#[cfg(windows)]
pub fn init_backend(mode: InputMode) {
    let current = CURRENT_MODE.get_or_init(|| std::sync::atomic::AtomicU8::new(MODE_SENDINPUT));

    // 切换模式时重置 DD 诊断旗标，便于下一轮再观察是否被路由到 DD
    DD_KEY_DOWN_LOGGED.store(false, std::sync::atomic::Ordering::SeqCst);
    DD_KEY_UP_LOGGED.store(false, std::sync::atomic::Ordering::SeqCst);
    DD_FALLBACK_LOGGED.store(false, std::sync::atomic::Ordering::SeqCst);

    match mode {
        InputMode::Interception => {
            let backend_cell =
                INTERCEPTION_BACKEND.get_or_init(|| Mutex::new(InterceptionBackend::new()));
            let mut guard = revive(backend_cell.lock());
            if guard.is_none() {
                *guard = InterceptionBackend::new();
            }
            if guard.is_some() {
                current.store(MODE_INTERCEPTION, std::sync::atomic::Ordering::SeqCst);
                info!("输入后端已切换为 Interception 驱动模式");
            } else {
                current.store(MODE_SENDINPUT, std::sync::atomic::Ordering::SeqCst);
                warn!("Interception 驱动未安装，降级为 SendInput 模式");
            }
        }
        InputMode::DdHid => {
            let Some(dir) = RESOURCES_DIR.get() else {
                warn!("DD-HID 切换失败：资源目录未注册");
                current.store(MODE_SENDINPUT, std::sync::atomic::Ordering::SeqCst);
                return;
            };
            let cell = DD_HID_BACKEND.get_or_init(|| Mutex::new(DdHidBackend::new(dir)));
            let mut guard = revive(cell.lock());
            if guard.is_none() {
                *guard = DdHidBackend::new(dir);
            }
            if guard.is_some() {
                current.store(MODE_DD_HID, std::sync::atomic::Ordering::SeqCst);
                info!("输入后端已切换为 DD-HID 模式");
            } else {
                current.store(MODE_SENDINPUT, std::sync::atomic::Ordering::SeqCst);
                warn!("DD-HID 加载失败，降级为 SendInput 模式");
            }
        }
        InputMode::SendInput => {
            // 切回 SendInput 时主动释放其他后端持有的句柄，避免 DLL 句柄/驱动 context
            // 在切换后泄漏，并让后续卸载操作不残留任何用户态引用。
            if let Some(lock) = DD_HID_BACKEND.get() {
                if revive(lock.lock()).take().is_some() {
                    info!("DD-HID 后端已释放");
                }
            }
            if let Some(lock) = INTERCEPTION_BACKEND.get() {
                if revive(lock.lock()).take().is_some() {
                    info!("Interception 后端已释放");
                }
            }
            current.store(MODE_SENDINPUT, std::sync::atomic::Ordering::SeqCst);
            info!("输入后端已切换为 SendInput 模式");
        }
    }
}

#[cfg(windows)]
pub fn current_mode() -> InputMode {
    let v = CURRENT_MODE
        .get()
        .map(|a| a.load(std::sync::atomic::Ordering::SeqCst))
        .unwrap_or(MODE_SENDINPUT);
    u8_to_mode(v)
}

#[cfg(not(windows))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMode {
    #[default]
    SendInput,
}

#[cfg(not(windows))]
impl InputMode {
    pub fn requires_distinct_target_for_toggle(&self) -> bool {
        false
    }
}

#[cfg(windows)]
/// 通过 `SendInput` 发送一个键盘事件,自动从 VK 推导扩展扫描码与 E0 前缀。
///
/// # Safety
///
/// `vk` 必须是 Win32 文档允许的虚拟键码;`flags` 必须是 `KEYBDINPUT.dwFlags`
/// 合法位的子集（KEYEVENTF_KEYUP 等）。本函数仅写本地栈上的 `INPUT` 然后
/// 调用 `SendInput`,不持有任何外部指针。
unsafe fn send_via_sendinput(vk: u32, flags: u32) {
    // SAFETY: MapVirtualKeyW 对任意 u32 都安全,无效 VK 返回 0
    let scan_ex = MapVirtualKeyW(vk, MAPVK_VK_TO_VSC_EX);
    let scan = (scan_ex & 0xFF) as u16;
    let prefix = (scan_ex >> 8) & 0xFF;
    let (w_vk, w_scan, scan_flags) = if scan == 0 || prefix == 0xE1 {
        (vk as u16, 0u16, 0u32)
    } else {
        let ext = if prefix == 0xE0 {
            KEYEVENTF_EXTENDEDKEY
        } else {
            0
        };
        (0u16, scan, KEYEVENTF_SCANCODE | ext)
    };
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: w_vk,
                wScan: w_scan,
                dwFlags: scan_flags | flags,
                time: 0,
                dwExtraInfo: SIM_MARKER,
            },
        },
    };
    // SAFETY: input 是栈上初始化完整的 INPUT_KEYBOARD,&input 在调用期间有效;
    // size_of::<INPUT>() 与 SendInput 的 cbSize 参数语义匹配
    SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
}

pub fn key_down(vk: u32) {
    #[cfg(windows)]
    {
        let mode = CURRENT_MODE
            .get()
            .map(|a| a.load(std::sync::atomic::Ordering::SeqCst))
            .unwrap_or(MODE_SENDINPUT);
        match mode {
            MODE_INTERCEPTION => {
                if let Some(lock) = INTERCEPTION_BACKEND.get() {
                    if let Some(backend) = revive(lock.lock()).as_ref() {
                        backend.send_key(vk, false);
                        return;
                    }
                }
            }
            MODE_DD_HID => {
                if let Some(lock) = DD_HID_BACKEND.get() {
                    if let Some(backend) = revive(lock.lock()).as_ref() {
                        if !DD_KEY_DOWN_LOGGED.swap(true, std::sync::atomic::Ordering::SeqCst) {
                            info!("key_down 路由到 DD-HID 后端（vk={:#x}）", vk);
                        }
                        // 注入前登记，hook 端按 FIFO 消费以过滤自循环
                        record_injection(vk, false);
                        backend.send_key(vk, false);
                        return;
                    }
                }
                if !DD_FALLBACK_LOGGED.swap(true, std::sync::atomic::Ordering::SeqCst) {
                    warn!("当前模式 DD-HID 但后端不存在，回退 SendInput");
                }
            }
            _ => {}
        }
        // SAFETY: send_via_sendinput 的 # Safety 契约对 (vk, 0) 成立——
        // vk 来自上层规则配置,0 是 KEYBDINPUT.dwFlags 的合法值（按下事件）
        unsafe { send_via_sendinput(vk, 0) };
    }
    #[cfg(not(windows))]
    let _ = vk;
}

pub fn key_up(vk: u32) {
    #[cfg(windows)]
    {
        let mode = CURRENT_MODE
            .get()
            .map(|a| a.load(std::sync::atomic::Ordering::SeqCst))
            .unwrap_or(MODE_SENDINPUT);
        match mode {
            MODE_INTERCEPTION => {
                if let Some(lock) = INTERCEPTION_BACKEND.get() {
                    if let Some(backend) = revive(lock.lock()).as_ref() {
                        backend.send_key(vk, true);
                        return;
                    }
                }
            }
            MODE_DD_HID => {
                if let Some(lock) = DD_HID_BACKEND.get() {
                    if let Some(backend) = revive(lock.lock()).as_ref() {
                        if !DD_KEY_UP_LOGGED.swap(true, std::sync::atomic::Ordering::SeqCst) {
                            info!("key_up 路由到 DD-HID 后端（vk={:#x}）", vk);
                        }
                        record_injection(vk, true);
                        backend.send_key(vk, true);
                        return;
                    }
                }
            }
            _ => {}
        }
        // SAFETY: send_via_sendinput 的 # Safety 契约对 (vk, KEYEVENTF_KEYUP) 成立
        unsafe { send_via_sendinput(vk, KEYEVENTF_KEYUP) };
    }
    #[cfg(not(windows))]
    let _ = vk;
}
