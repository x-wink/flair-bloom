#[cfg(windows)]
use super::ddhid::DdHidBackend;
#[cfg(windows)]
use super::interception::InterceptionBackend;
#[cfg(windows)]
use std::path::PathBuf;
#[cfg(windows)]
use std::sync::atomic::AtomicBool;
#[cfg(windows)]
use std::sync::{Mutex, OnceLock};
#[cfg(windows)]
use tracing::{info, warn};
#[cfg(windows)]
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MAPVK_VK_TO_VSC_EX,
};

/// 写入 SendInput 的 dwExtraInfo，hook 据此过滤程序自身模拟的按键，消除竞态。
/// Interception 后端会把 `information` 字段透传到 `KBDLLHOOKSTRUCT.dwExtraInfo`。
/// DD 后端无法控制此字段，因此 DD 模式下需在校验层禁止「触发键 == 目标键」。
pub const SIM_MARKER: usize = 0x5148_5844;

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

    /// DD-HID 模式无法用 dwExtraInfo 过滤自身注入，需在校验层禁止
    /// 「目标键 == 触发键 / 停止键」的组合。
    pub fn requires_distinct_target(&self) -> bool {
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
            let mut guard = backend_cell.lock().unwrap();
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
            let mut guard = cell.lock().unwrap();
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
    pub fn requires_distinct_target(&self) -> bool {
        false
    }
}

#[cfg(windows)]
unsafe fn send_via_sendinput(vk: u32, flags: u32) {
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
                    if let Some(backend) = lock.lock().unwrap().as_ref() {
                        backend.send_key(vk, false);
                        return;
                    }
                }
            }
            MODE_DD_HID => {
                if let Some(lock) = DD_HID_BACKEND.get() {
                    if let Some(backend) = lock.lock().unwrap().as_ref() {
                        if !DD_KEY_DOWN_LOGGED.swap(true, std::sync::atomic::Ordering::SeqCst) {
                            info!("key_down 路由到 DD-HID 后端（vk={:#x}）", vk);
                        }
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
                    if let Some(backend) = lock.lock().unwrap().as_ref() {
                        backend.send_key(vk, true);
                        return;
                    }
                }
            }
            MODE_DD_HID => {
                if let Some(lock) = DD_HID_BACKEND.get() {
                    if let Some(backend) = lock.lock().unwrap().as_ref() {
                        if !DD_KEY_UP_LOGGED.swap(true, std::sync::atomic::Ordering::SeqCst) {
                            info!("key_up 路由到 DD-HID 后端（vk={:#x}）", vk);
                        }
                        backend.send_key(vk, true);
                        return;
                    }
                }
            }
            _ => {}
        }
        unsafe { send_via_sendinput(vk, KEYEVENTF_KEYUP) };
    }
    #[cfg(not(windows))]
    let _ = vk;
}
