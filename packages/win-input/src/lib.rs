//! Windows 输入注入子系统：SendInput / Interception / DD-HID 三档后端。
//!
//! 统一入口：[`key_down`] / [`key_up`]（跨平台 stub）与 [`dispatch`]（Windows only）。
//! 非 Windows 平台提供空实现，不影响编译。

#[cfg(windows)]
mod dd_common;
#[cfg(windows)]
pub mod ddhid;
#[cfg(windows)]
pub mod interception;

#[cfg(windows)]
use ddhid::DdHidBackend;
#[cfg(windows)]
use interception::InterceptionBackend;
use qzh_profile::key_id::KeyId;
#[cfg(any(test, windows))]
use qzh_profile::key_id::MouseButton;
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

#[cfg(windows)]
const SIM_TTL: Duration = Duration::from_millis(50);

#[cfg(windows)]
type PendingMap = HashMap<(KeyId, bool), VecDeque<Instant>>;
#[cfg(windows)]
static PENDING_INJECTIONS: OnceLock<Mutex<PendingMap>> = OnceLock::new();

#[cfg(windows)]
fn pending_map() -> &'static Mutex<PendingMap> {
    PENDING_INJECTIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

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
fn record_injection(key: KeyId, is_up: bool) {
    let mut map = revive(pending_map().lock());
    let queue = map.entry((key, is_up)).or_default();
    let now = Instant::now();
    drop_expired(queue, now);
    queue.push_back(now);
}

/// hook 端调用：若该 (KeyId, is_up) 对应有未过期的 sim 记录，pop 一条并返回 true。
#[cfg(windows)]
pub fn try_consume_injection(key: KeyId, is_up: bool) -> bool {
    let mut map = revive(pending_map().lock());
    let Some(queue) = map.get_mut(&(key, is_up)) else {
        return false;
    };
    drop_expired(queue, Instant::now());
    queue.pop_front().is_some()
}

/// 清空注入队列（引擎重置规则或关闭时调用）。
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
    #[allow(clippy::should_implement_trait)]
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

    pub fn requires_distinct_target_for_toggle(&self) -> bool {
        matches!(self, Self::DdHid)
    }

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

#[cfg(windows)]
fn u8_to_mode(v: u8) -> InputMode {
    match v {
        MODE_INTERCEPTION => InputMode::Interception,
        MODE_DD_HID => InputMode::DdHid,
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
    DdKeyboard(u32),
    DdWheel { up: bool },
    DdMouse(MouseButton),
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
        (MODE_DD_HID, KeyId::Keyboard(vk)) => DispatchRoute::DdKeyboard(vk),
        (MODE_DD_HID, KeyId::Mouse(btn)) if is_wheel_button(btn) => {
            if is_up {
                DispatchRoute::Noop
            } else {
                DispatchRoute::DdWheel {
                    up: matches!(btn, MouseButton::WheelUp),
                }
            }
        }
        (MODE_DD_HID, KeyId::Mouse(btn)) => DispatchRoute::DdMouse(btn),
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
        InputMode::SendInput => {
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

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(_s: &str) -> Option<Self> {
        Some(Self::SendInput)
    }

    pub fn as_str(&self) -> &'static str {
        "sendinput"
    }
}

#[cfg(windows)]
fn build_kbd_sendinput(vk: u32, flags: u32) -> INPUT {
    // SAFETY: MapVirtualKeyW 对任意 u32 安全
    let scan_ex = unsafe { MapVirtualKeyW(vk, MAPVK_VK_TO_VSC_EX) };
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
    INPUT {
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
    }
}

#[cfg(windows)]
unsafe fn send_kbd_via_sendinput(vk: u32, flags: u32) {
    let input = build_kbd_sendinput(vk, flags);
    // SAFETY: input 是栈上完整初始化的 INPUT_KEYBOARD
    SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
}

#[cfg(windows)]
fn build_wheel_sendinput(up: bool) -> INPUT {
    // WHEEL_DELTA = 120 per notch；向下用补码表示负值
    let mouse_data: u32 = if up { 120u32 } else { (-120i32) as u32 };
    INPUT {
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
    }
}

#[cfg(windows)]
unsafe fn send_wheel_via_sendinput(up: bool) {
    let input = build_wheel_sendinput(up);
    // SAFETY: input 是栈上完整初始化的 INPUT_MOUSE
    SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
}

#[cfg(windows)]
fn build_mouse_sendinput(button: MouseButton, is_up: bool) -> INPUT {
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
    INPUT {
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
    }
}

#[cfg(windows)]
unsafe fn send_mouse_via_sendinput(button: MouseButton, is_up: bool) {
    let input = build_mouse_sendinput(button, is_up);
    // SAFETY: input 是栈上完整初始化的 INPUT_MOUSE
    SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
}

pub fn key_down(key: KeyId) {
    #[cfg(windows)]
    {
        dispatch(key, false);
    }
    #[cfg(not(windows))]
    let _ = key;
}

pub fn key_up(key: KeyId) {
    #[cfg(windows)]
    {
        dispatch(key, true);
    }
    #[cfg(not(windows))]
    let _ = key;
}

pub fn key_events(events: &[(KeyId, bool)]) {
    #[cfg(windows)]
    {
        dispatch_batch(events);
    }
    #[cfg(not(windows))]
    let _ = events;
}

#[cfg(windows)]
fn send_via_sendinput(key: KeyId, is_up: bool) {
    // SAFETY: 各 send_*_via_sendinput 的 Safety 契约由调用方保证
    match key {
        KeyId::Keyboard(vk) => unsafe {
            let flags = if is_up { KEYEVENTF_KEYUP } else { 0 };
            send_kbd_via_sendinput(vk, flags);
        },
        KeyId::Mouse(MouseButton::WheelUp | MouseButton::WheelDown) => {
            if !is_up {
                let up = matches!(key, KeyId::Mouse(MouseButton::WheelUp));
                unsafe { send_wheel_via_sendinput(up) };
            }
        }
        KeyId::Mouse(btn) => unsafe {
            send_mouse_via_sendinput(btn, is_up);
        },
    }
}

#[cfg(windows)]
fn build_sendinput_event(key: KeyId, is_up: bool) -> Option<INPUT> {
    match key {
        KeyId::Keyboard(vk) => {
            let flags = if is_up { KEYEVENTF_KEYUP } else { 0 };
            Some(build_kbd_sendinput(vk, flags))
        }
        KeyId::Mouse(MouseButton::WheelUp | MouseButton::WheelDown) => {
            if is_up {
                None
            } else {
                let up = matches!(key, KeyId::Mouse(MouseButton::WheelUp));
                Some(build_wheel_sendinput(up))
            }
        }
        KeyId::Mouse(btn) => Some(build_mouse_sendinput(btn, is_up)),
    }
}

#[cfg(windows)]
fn send_batch_via_sendinput(events: &[(KeyId, bool)]) {
    let inputs: Vec<_> = events
        .iter()
        .filter_map(|&(key, is_up)| build_sendinput_event(key, is_up))
        .collect();
    if inputs.is_empty() {
        return;
    }
    // SAFETY: inputs 是完整初始化的 INPUT 数组，长度和元素大小均正确。
    unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        );
    }
}

#[cfg(windows)]
fn dispatch_batch(events: &[(KeyId, bool)]) {
    if events.is_empty() {
        return;
    }
    let mode = CURRENT_MODE
        .get()
        .map(|a| a.load(std::sync::atomic::Ordering::SeqCst))
        .unwrap_or(MODE_SENDINPUT);
    if mode == MODE_SENDINPUT {
        send_batch_via_sendinput(events);
        return;
    }
    for &(key, is_up) in events {
        dispatch(key, is_up);
    }
}

#[cfg(windows)]
fn dispatch(key: KeyId, is_up: bool) {
    let mode = CURRENT_MODE
        .get()
        .map(|a| a.load(std::sync::atomic::Ordering::SeqCst))
        .unwrap_or(MODE_SENDINPUT);
    match resolve_route(mode, key, is_up) {
        DispatchRoute::SendInput => send_via_sendinput(key, is_up),
        DispatchRoute::Noop => {}
        DispatchRoute::InterceptionKeyboard(vk) => {
            if let Some(lock) = INTERCEPTION_BACKEND.get() {
                if let Some(backend) = revive(lock.lock()).as_ref() {
                    backend.send_key(vk, is_up);
                    return;
                }
            }
            send_via_sendinput(key, is_up);
        }
        DispatchRoute::InterceptionWheel { up } => {
            if let Some(lock) = INTERCEPTION_BACKEND.get() {
                if let Some(backend) = revive(lock.lock()).as_ref() {
                    if backend.send_wheel(up) {
                        return;
                    }
                    if !INTERCEPTION_MOUSE_FALLBACK_LOGGED
                        .swap(true, std::sync::atomic::Ordering::SeqCst)
                    {
                        warn!("Interception 未识别鼠标设备，滚轮回退 SendInput");
                    }
                }
            }
            unsafe { send_wheel_via_sendinput(up) };
        }
        DispatchRoute::InterceptionMouse(btn) => {
            if let Some(lock) = INTERCEPTION_BACKEND.get() {
                if let Some(backend) = revive(lock.lock()).as_ref() {
                    if backend.send_mouse(btn, is_up) {
                        return;
                    }
                    if !INTERCEPTION_MOUSE_FALLBACK_LOGGED
                        .swap(true, std::sync::atomic::Ordering::SeqCst)
                    {
                        warn!("Interception 模式但未识别鼠标设备，鼠标连发回退 SendInput");
                    }
                }
            }
            send_via_sendinput(key, is_up);
        }
        DispatchRoute::DdKeyboard(vk) => {
            if let Some(lock) = DD_HID_BACKEND.get() {
                if let Some(backend) = revive(lock.lock()).as_ref() {
                    log_dd_route(is_up, key);
                    record_injection(key, is_up);
                    backend.send_key(vk, is_up);
                    return;
                }
            }
            if !DD_FALLBACK_LOGGED.swap(true, std::sync::atomic::Ordering::SeqCst) {
                warn!("当前模式 DD-HID 但后端不存在，回退 SendInput");
            }
            send_via_sendinput(key, is_up);
        }
        DispatchRoute::DdWheel { up } => {
            if let Some(lock) = DD_HID_BACKEND.get() {
                if let Some(backend) = revive(lock.lock()).as_ref() {
                    log_dd_route(false, key);
                    record_injection(key, false);
                    if backend.send_wheel(up) {
                        return;
                    }
                    try_consume_injection(key, false);
                }
            }
            if !DD_FALLBACK_LOGGED.swap(true, std::sync::atomic::Ordering::SeqCst) {
                warn!("当前模式 DD-HID 但滚轮回退 SendInput");
            }
            unsafe { send_wheel_via_sendinput(up) };
        }
        DispatchRoute::DdMouse(btn) => {
            let mut backend_seen = false;
            if let Some(lock) = DD_HID_BACKEND.get() {
                if let Some(backend) = revive(lock.lock()).as_ref() {
                    backend_seen = true;
                    log_dd_route(is_up, key);
                    // 先登记再发送：hook 可能在 send_mouse 返回前就收到 LL 事件，
                    // 若顺序颠倒会把模拟事件误判为物理输入触发连发或停止连发。
                    // 若 DD 不支持此按钮（X1/X2）会返回 false，随即撤销登记，
                    // 避免 50ms TTL 内把后续物理 X1/X2 事件误消费。
                    record_injection(key, is_up);
                    if backend.send_mouse(btn, is_up) {
                        return;
                    }
                    // DD 不支持（X1/X2）→ 回退 SendInput（SIM_MARKER 路径），撤销预登记
                    try_consume_injection(key, is_up);
                }
            }
            if !backend_seen && !DD_FALLBACK_LOGGED.swap(true, std::sync::atomic::Ordering::SeqCst)
            {
                warn!("当前模式 DD-HID 但后端不存在，回退 SendInput");
            }
            send_via_sendinput(key, is_up);
        }
    }
}

#[cfg(windows)]
fn log_dd_route(is_up: bool, key: KeyId) {
    let logged = if is_up {
        &DD_KEY_UP_LOGGED
    } else {
        &DD_KEY_DOWN_LOGGED
    };
    if !logged.swap(true, std::sync::atomic::Ordering::SeqCst) {
        let dir = if is_up { "key_up" } else { "key_down" };
        info!("{} 路由到 DD-HID 后端（key={:?}）", dir, key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mouse(button: MouseButton) -> KeyId {
        KeyId::Mouse(button)
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
            DispatchRoute::DdWheel { up: false }
        );
        assert_eq!(
            resolve_route(MODE_DD_HID, mouse(MouseButton::WheelUp), false),
            DispatchRoute::DdWheel { up: true }
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
