//! Windows 输入注入子系统：SendInput / Interception / DDSimple / DD-HID 四档后端。
//!
//! 统一入口：[`key_down`] / [`key_up`]（跨平台 stub）与 [`dispatch`]（Windows only）。
//! 非 Windows 平台提供空实现，不影响编译。

#[cfg(windows)]
mod dd_common;
#[cfg(windows)]
pub mod ddhid;
#[cfg(windows)]
pub mod ddsimple;
#[cfg(windows)]
pub mod interception;

#[cfg(windows)]
use ddhid::DdHidBackend;
#[cfg(windows)]
use ddsimple::DdSimpleBackend;
#[cfg(windows)]
use interception::InterceptionBackend;
use qzh_profile::key_id::KeyId;
#[cfg(any(test, windows))]
use qzh_profile::key_id::MouseButton;
#[cfg(any(test, windows))]
use std::collections::{HashMap, VecDeque};
#[cfg(windows)]
use std::path::PathBuf;
#[cfg(windows)]
use std::sync::atomic::AtomicBool;
#[cfg(any(test, windows))]
use std::sync::{Mutex, OnceLock};
#[cfg(any(test, windows))]
use std::time::{Duration, Instant};
#[cfg(windows)]
use tracing::{info, warn};
#[cfg(windows)]
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MAPVK_VK_TO_VSC_EX,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, MOUSEEVENTF_XDOWN,
    MOUSEEVENTF_XUP, MOUSEINPUT,
};
#[cfg(windows)]
use windows_sys::Win32::UI::WindowsAndMessaging::{XBUTTON1, XBUTTON2};

/// 写入 SendInput 的 dwExtraInfo，hook 据此过滤程序自身模拟的按键，消除竞态。
pub const SIM_MARKER: usize = 0x5148_5844;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputEvent {
    pub key: KeyId,
    pub is_up: bool,
}

impl InputEvent {
    pub fn down(key: KeyId) -> Self {
        Self { key, is_up: false }
    }

    pub fn up(key: KeyId) -> Self {
        Self { key, is_up: true }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchResult {
    Sent,
    Noop,
    Failed,
    FallbackSent,
}

impl DispatchResult {
    pub fn was_sent(self) -> bool {
        matches!(self, Self::Sent | Self::FallbackSent)
    }
}

#[cfg(any(test, windows))]
const SIM_TTL: Duration = Duration::from_millis(50);
#[cfg(any(test, windows))]
const RELAY_TTL: Duration = Duration::from_millis(200);

#[cfg(any(test, windows))]
type PendingMap = HashMap<(KeyId, bool), VecDeque<Instant>>;
#[cfg(any(test, windows))]
static PENDING_INJECTIONS: OnceLock<Mutex<PendingMap>> = OnceLock::new();
#[cfg(any(test, windows))]
static RELAY_INJECTIONS: OnceLock<Mutex<PendingMap>> = OnceLock::new();

#[cfg(any(test, windows))]
fn pending_map() -> &'static Mutex<PendingMap> {
    PENDING_INJECTIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(any(test, windows))]
fn relay_map() -> &'static Mutex<PendingMap> {
    RELAY_INJECTIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(any(test, windows))]
fn revive<T>(r: std::sync::LockResult<T>) -> T {
    r.unwrap_or_else(|e| e.into_inner())
}

#[cfg(any(test, windows))]
fn drop_expired(queue: &mut VecDeque<Instant>, now: Instant, ttl: Duration) {
    while let Some(&front) = queue.front() {
        if now.duration_since(front) > ttl {
            queue.pop_front();
        } else {
            break;
        }
    }
}

#[cfg(any(test, windows))]
fn record_injection_in(map: &'static Mutex<PendingMap>, key: KeyId, is_up: bool, ttl: Duration) {
    let mut map = revive(map.lock());
    let queue = map.entry((key, is_up)).or_default();
    let now = Instant::now();
    drop_expired(queue, now, ttl);
    queue.push_back(now);
}

/// DD 系列（DD-HID / DDSimple）注入的**唯一**自注入过滤依据：注入前登记 `(key, is_up)`，
/// hook 回灌时 `try_consume_injection` 配平消费。
///
/// 坑（务必知悉，勿重蹈）：DD 驱动把 `ExtraInformation` 写死为 0，注入事件回到 LL hook 时
/// `dwExtraInfo` 恒为 0，`SIM_MARKER` **无法幸存**——故 DD 路径既不设置也不依赖 SIM。
/// 不要再为 DD 加任何「直写 SIM」的旁路（历史上的 `dd_direct` DeviceIoControl 旁路已移除，
/// 缘由见 `ddsimple.rs` 顶部注释）。
///
/// 这套时间窗口队列是**尽力而为、不保证 100%**：对同键规则（`trigger == target`）无法可靠
/// 区分「用户真实按键」与「自身注入回灌」，高负载 / 丢事件时会偶发误判（连发自停或停不掉）。
/// 这是 DD 驱动的固有缺陷，已决定**不再强行修复**——同键 Toggle 由
/// `requires_distinct_target_for_toggle` 在配置层拦截，同键 Hold 则接受该不可靠性。
/// 彻底消除需 Raw Input 按来源区分 DD 虚拟设备与物理设备，非当前范围。
#[cfg(any(test, windows))]
fn record_injection(key: KeyId, is_up: bool) {
    record_injection_in(pending_map(), key, is_up, SIM_TTL);
}

#[cfg(any(test, windows))]
fn record_relay_injection(key: KeyId, is_up: bool) {
    record_injection_in(relay_map(), key, is_up, RELAY_TTL);
}

#[cfg(any(test, windows))]
fn try_consume_from(
    map: &'static Mutex<PendingMap>,
    key: KeyId,
    is_up: bool,
    ttl: Duration,
) -> bool {
    let mut map = revive(map.lock());
    let Some(queue) = map.get_mut(&(key, is_up)) else {
        return false;
    };
    drop_expired(queue, Instant::now(), ttl);
    queue.pop_front().is_some()
}

/// hook 端调用：若该 (KeyId, is_up) 对应有未过期的 sim 记录，pop 一条并返回 true。
#[cfg(any(test, windows))]
pub fn try_consume_injection(key: KeyId, is_up: bool) -> bool {
    try_consume_from(pending_map(), key, is_up, SIM_TTL)
}

#[cfg(all(not(windows), not(test)))]
pub fn try_consume_injection(_key: KeyId, _is_up: bool) -> bool {
    false
}

/// WebView relay 端调用：若该事件是应用刚注入后由 DOM 回灌的模拟事件，pop 一条并返回 true。
#[cfg(any(test, windows))]
pub fn try_consume_relay_injection(key: KeyId, is_up: bool) -> bool {
    try_consume_from(relay_map(), key, is_up, RELAY_TTL)
}

#[cfg(all(not(windows), not(test)))]
pub fn try_consume_relay_injection(_key: KeyId, _is_up: bool) -> bool {
    false
}

/// 清空注入队列（引擎重置规则或关闭时调用）。
#[cfg(any(test, windows))]
pub fn clear_pending_injections() {
    if let Some(lock) = PENDING_INJECTIONS.get() {
        revive(lock.lock()).clear();
    }
}

#[cfg(all(not(windows), not(test)))]
pub fn clear_pending_injections() {}

#[cfg(any(test, windows))]
pub fn clear_relay_injections() {
    if let Some(lock) = RELAY_INJECTIONS.get() {
        revive(lock.lock()).clear();
    }
}

#[cfg(all(not(windows), not(test)))]
pub fn clear_relay_injections() {}

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputMode {
    #[default]
    SendInput,
    Interception,
    #[serde(rename = "ddsimple")]
    DdSimple,
    DdHid,
}

#[cfg(windows)]
impl InputMode {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "sendinput" => Some(Self::SendInput),
            "interception" => Some(Self::Interception),
            "ddsimple" | "dd_simple" => Some(Self::DdSimple),
            "dd_hid" => Some(Self::DdHid),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SendInput => "sendinput",
            Self::Interception => "interception",
            Self::DdSimple => "ddsimple",
            Self::DdHid => "dd_hid",
        }
    }

    /// DD 系列（DDSimple / DD-HID）驱动注入无法携带 `SIM_MARKER`（驱动把 ExtraInformation
    /// 写死为 0），自注入只能靠 `PENDING_INJECTIONS` 时间窗口队列过滤。该队列对
    /// `target == trigger / stop` 的 Toggle 规则无法可靠区分「用户真实按下停止键」与
    /// 「自身注入回灌」，会导致连发自停或停不掉。故 DD 系列禁止 Toggle 目标键与启动/停止键相同。
    pub fn requires_distinct_target_for_toggle(&self) -> bool {
        matches!(self, Self::DdHid | Self::DdSimple)
    }

    /// 鼠标侧键（X1/X2）作为目标键是否被禁止。仅 DD-HID（63340 `DD_btn` 不支持侧键值域）
    /// 受限；DDSimple 的 `dd63330` 走 `MOUSE_INPUT_DATA.ButtonFlags`，原生支持 X1/X2。
    pub fn forbids_side_button_target(&self) -> bool {
        matches!(self, Self::DdHid)
    }

    pub fn requires_admin(&self) -> bool {
        !matches!(self, Self::SendInput)
    }
}

#[cfg(windows)]
static INTERCEPTION_BACKEND: OnceLock<Mutex<Option<InterceptionBackend>>> = OnceLock::new();
#[cfg(windows)]
static DD_HID_BACKEND: OnceLock<Mutex<Option<DdHidBackend>>> = OnceLock::new();
#[cfg(windows)]
static DD_SIMPLE_BACKEND: OnceLock<Mutex<Option<DdSimpleBackend>>> = OnceLock::new();
#[cfg(windows)]
static CURRENT_MODE: OnceLock<std::sync::atomic::AtomicU8> = OnceLock::new();
#[cfg(windows)]
static RESOURCES_DIR: OnceLock<PathBuf> = OnceLock::new();

#[cfg(windows)]
static DD_KEY_DOWN_LOGGED: AtomicBool = AtomicBool::new(false);
#[cfg(windows)]
static DD_KEY_UP_LOGGED: AtomicBool = AtomicBool::new(false);
#[cfg(windows)]
static DD_FALLBACK_LOGGED: AtomicBool = AtomicBool::new(false);
#[cfg(windows)]
static INTERCEPTION_MOUSE_FALLBACK_LOGGED: AtomicBool = AtomicBool::new(false);

#[cfg(any(test, windows))]
const MODE_SENDINPUT: u8 = 0;
#[cfg(any(test, windows))]
const MODE_INTERCEPTION: u8 = 1;
#[cfg(any(test, windows))]
const MODE_DD_HID: u8 = 2;
#[cfg(any(test, windows))]
const MODE_DD_SIMPLE: u8 = 3;

#[cfg(windows)]
fn u8_to_mode(v: u8) -> InputMode {
    match v {
        MODE_INTERCEPTION => InputMode::Interception,
        MODE_DD_HID => InputMode::DdHid,
        MODE_DD_SIMPLE => InputMode::DdSimple,
        _ => InputMode::SendInput,
    }
}

#[cfg(any(test, windows))]
fn is_wheel_button(button: MouseButton) -> bool {
    matches!(button, MouseButton::WheelUp | MouseButton::WheelDown)
}

#[cfg(any(test, windows))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DispatchRoute {
    SendInput,
    Noop,
    InterceptionKeyboard(u32),
    InterceptionWheel { up: bool },
    InterceptionMouse(MouseButton),
    DdHidKeyboard(u32),
    DdHidWheel { up: bool },
    DdHidMouse(MouseButton),
    DdSimpleKeyboard(u32),
    DdSimpleWheel { up: bool },
    DdSimpleMouse(MouseButton),
}

#[cfg(any(test, windows))]
fn resolve_route(mode: u8, key: KeyId, is_up: bool) -> DispatchRoute {
    match (mode, key) {
        (MODE_INTERCEPTION, KeyId::Keyboard(vk)) => DispatchRoute::InterceptionKeyboard(vk),
        (MODE_INTERCEPTION, KeyId::Mouse(btn)) if is_wheel_button(btn) => {
            if is_up {
                DispatchRoute::Noop
            } else {
                DispatchRoute::InterceptionWheel {
                    up: matches!(btn, MouseButton::WheelUp),
                }
            }
        }
        (MODE_INTERCEPTION, KeyId::Mouse(btn)) => DispatchRoute::InterceptionMouse(btn),
        (MODE_DD_HID, KeyId::Keyboard(vk)) => DispatchRoute::DdHidKeyboard(vk),
        (MODE_DD_HID, KeyId::Mouse(btn)) if is_wheel_button(btn) => {
            if is_up {
                DispatchRoute::Noop
            } else {
                DispatchRoute::DdHidWheel {
                    up: matches!(btn, MouseButton::WheelUp),
                }
            }
        }
        (MODE_DD_HID, KeyId::Mouse(btn)) => DispatchRoute::DdHidMouse(btn),
        (MODE_DD_SIMPLE, KeyId::Keyboard(vk)) => DispatchRoute::DdSimpleKeyboard(vk),
        (MODE_DD_SIMPLE, KeyId::Mouse(btn)) if is_wheel_button(btn) => {
            if is_up {
                DispatchRoute::Noop
            } else {
                DispatchRoute::DdSimpleWheel {
                    up: matches!(btn, MouseButton::WheelUp),
                }
            }
        }
        (MODE_DD_SIMPLE, KeyId::Mouse(btn)) => DispatchRoute::DdSimpleMouse(btn),
        _ => DispatchRoute::SendInput,
    }
}

/// 注册资源目录（供 DD DLL 定位）。
#[cfg(windows)]
pub fn set_resources_dir(dir: PathBuf) {
    let _ = RESOURCES_DIR.set(dir);
}

#[cfg(windows)]
pub fn init_backend(mode: InputMode) {
    let current = CURRENT_MODE.get_or_init(|| std::sync::atomic::AtomicU8::new(MODE_SENDINPUT));
    DD_KEY_DOWN_LOGGED.store(false, std::sync::atomic::Ordering::SeqCst);
    DD_KEY_UP_LOGGED.store(false, std::sync::atomic::Ordering::SeqCst);
    DD_FALLBACK_LOGGED.store(false, std::sync::atomic::Ordering::SeqCst);
    INTERCEPTION_MOUSE_FALLBACK_LOGGED.store(false, std::sync::atomic::Ordering::SeqCst);

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
        InputMode::DdSimple => {
            let Some(dir) = RESOURCES_DIR.get() else {
                warn!("DD Simple 切换失败：资源目录未注册");
                current.store(MODE_SENDINPUT, std::sync::atomic::Ordering::SeqCst);
                return;
            };
            let cell = DD_SIMPLE_BACKEND.get_or_init(|| Mutex::new(DdSimpleBackend::new(dir)));
            let mut guard = revive(cell.lock());
            if guard.is_none() {
                *guard = DdSimpleBackend::new(dir);
            }
            if guard.is_some() {
                current.store(MODE_DD_SIMPLE, std::sync::atomic::Ordering::SeqCst);
                info!("输入后端已切换为 DD Simple 模式");
            } else {
                current.store(MODE_SENDINPUT, std::sync::atomic::Ordering::SeqCst);
                warn!("DD Simple 加载失败，降级为 SendInput 模式");
            }
        }
        InputMode::SendInput => {
            if let Some(lock) = DD_HID_BACKEND.get() {
                if revive(lock.lock()).take().is_some() {
                    info!("DD-HID 后端已释放");
                }
            }
            if let Some(lock) = DD_SIMPLE_BACKEND.get() {
                if revive(lock.lock()).take().is_some() {
                    info!("DD Simple 后端已释放");
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

    pub fn forbids_side_button_target(&self) -> bool {
        false
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(_s: &str) -> Option<Self> {
        Some(Self::SendInput)
    }

    pub fn as_str(&self) -> &'static str {
        "sendinput"
    }
}

#[cfg(windows)]
unsafe fn send_kbd_via_sendinput(vk: u32, flags: u32) -> bool {
    // SAFETY: MapVirtualKeyW 对任意 u32 安全
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
    // SAFETY: input 是栈上完整初始化的 INPUT_KEYBOARD
    SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) == 1
}

#[cfg(windows)]
unsafe fn send_wheel_via_sendinput(up: bool) -> bool {
    // WHEEL_DELTA = 120 per notch；向下用补码表示负值
    let mouse_data: u32 = if up { 120u32 } else { (-120i32) as u32 };
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: mouse_data,
                dwFlags: MOUSEEVENTF_WHEEL,
                time: 0,
                dwExtraInfo: SIM_MARKER,
            },
        },
    };
    // SAFETY: input 是栈上完整初始化的 INPUT_MOUSE
    SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) == 1
}

#[cfg(windows)]
unsafe fn send_mouse_via_sendinput(button: MouseButton, is_up: bool) -> bool {
    let (dw_flags, mouse_data) = match (button, is_up) {
        (MouseButton::Left, false) => (MOUSEEVENTF_LEFTDOWN, 0),
        (MouseButton::Left, true) => (MOUSEEVENTF_LEFTUP, 0),
        (MouseButton::Right, false) => (MOUSEEVENTF_RIGHTDOWN, 0),
        (MouseButton::Right, true) => (MOUSEEVENTF_RIGHTUP, 0),
        (MouseButton::Middle, false) => (MOUSEEVENTF_MIDDLEDOWN, 0),
        (MouseButton::Middle, true) => (MOUSEEVENTF_MIDDLEUP, 0),
        (MouseButton::X1, false) => (MOUSEEVENTF_XDOWN, XBUTTON1 as i32),
        (MouseButton::X1, true) => (MOUSEEVENTF_XUP, XBUTTON1 as i32),
        (MouseButton::X2, false) => (MOUSEEVENTF_XDOWN, XBUTTON2 as i32),
        (MouseButton::X2, true) => (MOUSEEVENTF_XUP, XBUTTON2 as i32),
        // WheelUp/WheelDown 由 dispatch 提前处理，不应到达此处
        (MouseButton::WheelUp | MouseButton::WheelDown, _) => unreachable!(),
    };
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: mouse_data as u32,
                dwFlags: dw_flags,
                time: 0,
                dwExtraInfo: SIM_MARKER,
            },
        },
    };
    // SAFETY: input 是栈上完整初始化的 INPUT_MOUSE
    SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) == 1
}

pub fn key_down(key: KeyId) {
    let _ = key_event(InputEvent::down(key));
}

pub fn key_up(key: KeyId) {
    let _ = key_event(InputEvent::up(key));
}

pub fn key_event(event: InputEvent) -> DispatchResult {
    #[cfg(windows)]
    {
        dispatch(event.key, event.is_up)
    }
    #[cfg(not(windows))]
    {
        let _ = event;
        DispatchResult::Noop
    }
}

pub fn key_events(events: &[InputEvent]) -> Vec<DispatchResult> {
    events.iter().copied().map(key_event).collect()
}

#[cfg(windows)]
fn send_via_sendinput(key: KeyId, is_up: bool) -> DispatchResult {
    // SAFETY: 各 send_*_via_sendinput 的 Safety 契约由调用方保证
    let sent = match key {
        KeyId::Keyboard(vk) => unsafe {
            let flags = if is_up { KEYEVENTF_KEYUP } else { 0 };
            send_kbd_via_sendinput(vk, flags)
        },
        KeyId::Mouse(MouseButton::WheelUp | MouseButton::WheelDown) => {
            if !is_up {
                let up = matches!(key, KeyId::Mouse(MouseButton::WheelUp));
                unsafe { send_wheel_via_sendinput(up) }
            } else {
                return DispatchResult::Noop;
            }
        }
        KeyId::Mouse(btn) => unsafe { send_mouse_via_sendinput(btn, is_up) },
    };
    if sent {
        record_relay_injection(key, is_up);
        DispatchResult::Sent
    } else {
        DispatchResult::Failed
    }
}

#[cfg(windows)]
fn dispatch(key: KeyId, is_up: bool) -> DispatchResult {
    let mode = CURRENT_MODE
        .get()
        .map(|a| a.load(std::sync::atomic::Ordering::SeqCst))
        .unwrap_or(MODE_SENDINPUT);
    match resolve_route(mode, key, is_up) {
        DispatchRoute::SendInput => send_via_sendinput(key, is_up),
        DispatchRoute::Noop => DispatchResult::Noop,
        DispatchRoute::InterceptionKeyboard(vk) => {
            if let Some(lock) = INTERCEPTION_BACKEND.get() {
                if let Some(backend) = revive(lock.lock()).as_ref() {
                    if backend.send_key(vk, is_up) {
                        record_relay_injection(key, is_up);
                        return DispatchResult::Sent;
                    }
                    return DispatchResult::Failed;
                }
            }
            send_via_sendinput(key, is_up)
        }
        DispatchRoute::InterceptionWheel { up } => {
            if let Some(lock) = INTERCEPTION_BACKEND.get() {
                if let Some(backend) = revive(lock.lock()).as_ref() {
                    if backend.send_wheel(up) {
                        record_relay_injection(key, false);
                        return DispatchResult::Sent;
                    }
                    if !INTERCEPTION_MOUSE_FALLBACK_LOGGED
                        .swap(true, std::sync::atomic::Ordering::SeqCst)
                    {
                        warn!("Interception 未识别鼠标设备，滚轮回退 SendInput");
                    }
                }
            }
            let result = if unsafe { send_wheel_via_sendinput(up) } {
                DispatchResult::FallbackSent
            } else {
                DispatchResult::Failed
            };
            if result.was_sent() {
                record_relay_injection(key, false);
            }
            result
        }
        DispatchRoute::InterceptionMouse(btn) => {
            if let Some(lock) = INTERCEPTION_BACKEND.get() {
                if let Some(backend) = revive(lock.lock()).as_ref() {
                    if backend.send_mouse(btn, is_up) {
                        record_relay_injection(key, is_up);
                        return DispatchResult::Sent;
                    }
                    if !INTERCEPTION_MOUSE_FALLBACK_LOGGED
                        .swap(true, std::sync::atomic::Ordering::SeqCst)
                    {
                        warn!("Interception 模式但未识别鼠标设备，鼠标连发回退 SendInput");
                    }
                }
            }
            send_via_sendinput(key, is_up)
        }
        DispatchRoute::DdHidKeyboard(vk) => {
            let mut backend_seen = false;
            if let Some(lock) = DD_HID_BACKEND.get() {
                if let Some(backend) = revive(lock.lock()).as_ref() {
                    backend_seen = true;
                    log_dd_route("DD-HID", is_up, key);
                    record_injection(key, is_up);
                    if backend.send_key(vk, is_up) {
                        record_relay_injection(key, is_up);
                        return DispatchResult::Sent;
                    }
                    // DD 注入失败（VK 无映射 / 驱动拒绝）→ 撤销预登记，回退 SendInput，
                    // 与鼠标路由对称，避免按键被直接丢弃。
                    try_consume_injection(key, is_up);
                }
            }
            if !backend_seen && !DD_FALLBACK_LOGGED.swap(true, std::sync::atomic::Ordering::SeqCst)
            {
                warn!("当前模式 DD-HID 但后端不存在，回退 SendInput");
            }
            send_via_sendinput(key, is_up)
        }
        DispatchRoute::DdHidWheel { up } => {
            if let Some(lock) = DD_HID_BACKEND.get() {
                if let Some(backend) = revive(lock.lock()).as_ref() {
                    log_dd_route("DD-HID", false, key);
                    record_injection(key, false);
                    if backend.send_wheel(up) {
                        record_relay_injection(key, false);
                        return DispatchResult::Sent;
                    }
                    try_consume_injection(key, false);
                }
            }
            if !DD_FALLBACK_LOGGED.swap(true, std::sync::atomic::Ordering::SeqCst) {
                warn!("当前模式 DD-HID 但滚轮回退 SendInput");
            }
            let result = if unsafe { send_wheel_via_sendinput(up) } {
                DispatchResult::FallbackSent
            } else {
                DispatchResult::Failed
            };
            if result.was_sent() {
                record_relay_injection(key, false);
            }
            result
        }
        DispatchRoute::DdHidMouse(btn) => {
            let mut backend_seen = false;
            if let Some(lock) = DD_HID_BACKEND.get() {
                if let Some(backend) = revive(lock.lock()).as_ref() {
                    backend_seen = true;
                    log_dd_route("DD-HID", is_up, key);
                    // 先登记再发送：hook 可能在 send_mouse 返回前就收到 LL 事件，
                    // 若顺序颠倒会把模拟事件误判为物理输入触发连发或停止连发。
                    // 若 DD 不支持此按钮（X1/X2）会返回 false，随即撤销登记，
                    // 避免 50ms TTL 内把后续物理 X1/X2 事件误消费。
                    record_injection(key, is_up);
                    if backend.send_mouse(btn, is_up) {
                        record_relay_injection(key, is_up);
                        return DispatchResult::Sent;
                    }
                    // DD 不支持（X1/X2）→ 回退 SendInput（SIM_MARKER 路径），撤销预登记
                    try_consume_injection(key, is_up);
                }
            }
            if !backend_seen && !DD_FALLBACK_LOGGED.swap(true, std::sync::atomic::Ordering::SeqCst)
            {
                warn!("当前模式 DD-HID 但后端不存在，回退 SendInput");
            }
            send_via_sendinput(key, is_up)
        }
        DispatchRoute::DdSimpleKeyboard(vk) => {
            let mut backend_seen = false;
            if let Some(lock) = DD_SIMPLE_BACKEND.get() {
                if let Some(backend) = revive(lock.lock()).as_ref() {
                    backend_seen = true;
                    log_dd_route("DD Simple", is_up, key);
                    // dd63330 驱动注入时把 ExtraInformation 写死为 0（内核键盘注入函数
                    // VA 0x140003d13 显式清零），回到 LL hook 时 dwExtraInfo 恒为 0，SIM_MARKER
                    // 无法幸存。故一律先登记 PENDING_INJECTIONS，由时间窗口队列过滤自身注入回灌，
                    // 否则 hook 会把注入误判为物理按键。
                    record_injection(key, is_up);
                    if backend.send_key(vk, is_up) {
                        record_relay_injection(key, is_up);
                        return DispatchResult::Sent;
                    }
                    // VK 无 scan code / 驱动拒绝 → 撤销登记，回退 SendInput，避免按键被直接丢弃。
                    try_consume_injection(key, is_up);
                }
            }
            if !backend_seen && !DD_FALLBACK_LOGGED.swap(true, std::sync::atomic::Ordering::SeqCst)
            {
                warn!("当前模式 DD Simple 但后端不存在，回退 SendInput");
            }
            send_via_sendinput(key, is_up)
        }
        DispatchRoute::DdSimpleWheel { up } => {
            if let Some(lock) = DD_SIMPLE_BACKEND.get() {
                if let Some(backend) = revive(lock.lock()).as_ref() {
                    log_dd_route("DD Simple", false, key);
                    // 同键盘：dd63330 驱动把 ExtraInformation 写死为 0，SIM_MARKER 无法幸存，
                    // 一律登记 PENDING_INJECTIONS 兜底自注入回灌。
                    record_injection(key, false);
                    if backend.send_wheel(up) {
                        record_relay_injection(key, false);
                        return DispatchResult::Sent;
                    }
                    try_consume_injection(key, false);
                }
            }
            if !DD_FALLBACK_LOGGED.swap(true, std::sync::atomic::Ordering::SeqCst) {
                warn!("当前模式 DD Simple 但滚轮回退 SendInput");
            }
            let result = if unsafe { send_wheel_via_sendinput(up) } {
                DispatchResult::FallbackSent
            } else {
                DispatchResult::Failed
            };
            if result.was_sent() {
                record_relay_injection(key, false);
            }
            result
        }
        DispatchRoute::DdSimpleMouse(btn) => {
            let mut backend_seen = false;
            if let Some(lock) = DD_SIMPLE_BACKEND.get() {
                if let Some(backend) = revive(lock.lock()).as_ref() {
                    backend_seen = true;
                    log_dd_route("DD Simple", is_up, key);
                    // 同键盘/滚轮：dd63330 驱动把 ExtraInformation 写死为 0，SIM_MARKER 不可用，
                    // 一律登记 PENDING_INJECTIONS 兜底自注入回灌。
                    record_injection(key, is_up);
                    if backend.send_mouse(btn, is_up) {
                        record_relay_injection(key, is_up);
                        return DispatchResult::Sent;
                    }
                    try_consume_injection(key, is_up);
                }
            }
            if !backend_seen && !DD_FALLBACK_LOGGED.swap(true, std::sync::atomic::Ordering::SeqCst)
            {
                warn!("当前模式 DD Simple 但后端不存在，回退 SendInput");
            }
            send_via_sendinput(key, is_up)
        }
    }
}

#[cfg(windows)]
fn log_dd_route(backend_name: &str, is_up: bool, key: KeyId) {
    let logged = if is_up {
        &DD_KEY_UP_LOGGED
    } else {
        &DD_KEY_DOWN_LOGGED
    };
    if !logged.swap(true, std::sync::atomic::Ordering::SeqCst) {
        let dir = if is_up { "key_up" } else { "key_down" };
        info!("{} 路由到 {} 后端（key={:?}）", dir, backend_name, key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mouse(button: MouseButton) -> KeyId {
        KeyId::Mouse(button)
    }

    #[test]
    fn pending_injection_is_consumed_once_and_keeps_direction() {
        clear_pending_injections();
        let key = KeyId::Keyboard(0x51);

        record_injection(key, false);

        assert!(!try_consume_injection(key, true));
        assert!(try_consume_injection(key, false));
        assert!(!try_consume_injection(key, false));
    }

    #[test]
    fn relay_injection_is_consumed_once_and_keeps_direction() {
        clear_relay_injections();
        let key = KeyId::Keyboard(0x45);

        record_relay_injection(key, false);

        assert!(!try_consume_relay_injection(key, true));
        assert!(try_consume_relay_injection(key, false));
        assert!(!try_consume_relay_injection(key, false));
    }

    #[cfg(windows)]
    #[test]
    fn ddsimple_mode_uses_canonical_string_and_legacy_alias() {
        assert_eq!(InputMode::from_str("ddsimple"), Some(InputMode::DdSimple));
        assert_eq!(InputMode::from_str("dd_simple"), Some(InputMode::DdSimple));
        assert_eq!(InputMode::DdSimple.as_str(), "ddsimple");
    }

    #[cfg(windows)]
    #[test]
    fn non_sendinput_modes_require_admin() {
        assert!(!InputMode::SendInput.requires_admin());
        assert!(InputMode::Interception.requires_admin());
        assert!(InputMode::DdSimple.requires_admin());
        assert!(InputMode::DdHid.requires_admin());
    }

    #[cfg(windows)]
    #[test]
    fn dd_series_forbids_same_key_toggle() {
        // DD 系列驱动注入无法携带 SIM_MARKER（驱动清零 ExtraInformation），
        // 同键 Toggle 无法可靠区分自注入与物理停止键，故两者都禁止。
        assert!(InputMode::DdSimple.requires_distinct_target_for_toggle());
        assert!(InputMode::DdHid.requires_distinct_target_for_toggle());
        // 非 DD 系列不受限。
        assert!(!InputMode::SendInput.requires_distinct_target_for_toggle());
        assert!(!InputMode::Interception.requires_distinct_target_for_toggle());
    }

    #[cfg(windows)]
    #[test]
    fn only_ddhid_forbids_side_button_target() {
        // DDSimple 的 dd63330 走 MOUSE_INPUT_DATA.ButtonFlags，原生支持 X1/X2 侧键。
        assert!(InputMode::DdHid.forbids_side_button_target());
        assert!(!InputMode::DdSimple.forbids_side_button_target());
        assert!(!InputMode::SendInput.forbids_side_button_target());
        assert!(!InputMode::Interception.forbids_side_button_target());
    }

    #[test]
    fn interception_wheel_down_routes_to_wheel_backend() {
        assert_eq!(
            resolve_route(MODE_INTERCEPTION, mouse(MouseButton::WheelDown), false),
            DispatchRoute::InterceptionWheel { up: false }
        );
    }

    #[test]
    fn interception_wheel_up_routes_to_wheel_backend() {
        assert_eq!(
            resolve_route(MODE_INTERCEPTION, mouse(MouseButton::WheelUp), false),
            DispatchRoute::InterceptionWheel { up: true }
        );
    }

    #[test]
    fn interception_wheel_release_is_noop() {
        assert_eq!(
            resolve_route(MODE_INTERCEPTION, mouse(MouseButton::WheelDown), true),
            DispatchRoute::Noop
        );
        assert_eq!(
            resolve_route(MODE_INTERCEPTION, mouse(MouseButton::WheelUp), true),
            DispatchRoute::Noop
        );
    }

    #[test]
    fn interception_regular_mouse_routes_to_mouse_backend() {
        assert_eq!(
            resolve_route(MODE_INTERCEPTION, mouse(MouseButton::Left), false),
            DispatchRoute::InterceptionMouse(MouseButton::Left)
        );
        assert_eq!(
            resolve_route(MODE_INTERCEPTION, mouse(MouseButton::X2), true),
            DispatchRoute::InterceptionMouse(MouseButton::X2)
        );
    }

    #[test]
    fn dd_hid_wheel_routes_to_wheel_backend() {
        assert_eq!(
            resolve_route(MODE_DD_HID, mouse(MouseButton::WheelDown), false),
            DispatchRoute::DdHidWheel { up: false }
        );
        assert_eq!(
            resolve_route(MODE_DD_HID, mouse(MouseButton::WheelUp), false),
            DispatchRoute::DdHidWheel { up: true }
        );
    }

    #[test]
    fn dd_hid_wheel_release_is_noop() {
        assert_eq!(
            resolve_route(MODE_DD_HID, mouse(MouseButton::WheelDown), true),
            DispatchRoute::Noop
        );
        assert_eq!(
            resolve_route(MODE_DD_HID, mouse(MouseButton::WheelUp), true),
            DispatchRoute::Noop
        );
    }

    #[test]
    fn ddsimple_keyboard_routes_to_simple_backend() {
        assert_eq!(
            resolve_route(MODE_DD_SIMPLE, KeyId::Keyboard(0x51), false),
            DispatchRoute::DdSimpleKeyboard(0x51)
        );
    }

    #[test]
    fn ddsimple_wheel_routes_to_simple_backend() {
        assert_eq!(
            resolve_route(MODE_DD_SIMPLE, mouse(MouseButton::WheelDown), false),
            DispatchRoute::DdSimpleWheel { up: false }
        );
        assert_eq!(
            resolve_route(MODE_DD_SIMPLE, mouse(MouseButton::WheelUp), false),
            DispatchRoute::DdSimpleWheel { up: true }
        );
    }

    #[test]
    fn ddsimple_wheel_release_is_noop() {
        assert_eq!(
            resolve_route(MODE_DD_SIMPLE, mouse(MouseButton::WheelDown), true),
            DispatchRoute::Noop
        );
        assert_eq!(
            resolve_route(MODE_DD_SIMPLE, mouse(MouseButton::WheelUp), true),
            DispatchRoute::Noop
        );
    }

    #[test]
    fn ddsimple_regular_mouse_routes_to_simple_backend() {
        assert_eq!(
            resolve_route(MODE_DD_SIMPLE, mouse(MouseButton::Left), false),
            DispatchRoute::DdSimpleMouse(MouseButton::Left)
        );
        assert_eq!(
            resolve_route(MODE_DD_SIMPLE, mouse(MouseButton::X2), true),
            DispatchRoute::DdSimpleMouse(MouseButton::X2)
        );
    }

    #[test]
    fn sendinput_mode_uses_sendinput_route() {
        assert_eq!(
            resolve_route(MODE_SENDINPUT, KeyId::Keyboard(0x51), false),
            DispatchRoute::SendInput
        );
        assert_eq!(
            resolve_route(MODE_SENDINPUT, mouse(MouseButton::WheelDown), false),
            DispatchRoute::SendInput
        );
    }
}
