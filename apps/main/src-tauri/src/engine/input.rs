#[cfg(windows)]
use super::interception::InterceptionBackend;
#[cfg(windows)]
use std::sync::Mutex;
#[cfg(windows)]
use std::sync::OnceLock;
#[cfg(windows)]
use tracing::{info, warn};
#[cfg(windows)]
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MAPVK_VK_TO_VSC_EX,
};

/// 写入 SendInput 的 dwExtraInfo，hook 据此过滤程序自身模拟的按键，消除竞态
pub const SIM_MARKER: usize = 0x5148_5844;

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputMode {
    #[default]
    SendInput,
    Interception,
}

#[cfg(windows)]
static INTERCEPTION_BACKEND: OnceLock<Mutex<Option<InterceptionBackend>>> = OnceLock::new();

#[cfg(windows)]
static CURRENT_MODE: OnceLock<std::sync::atomic::AtomicU8> = OnceLock::new();

#[cfg(windows)]
const MODE_SENDINPUT: u8 = 0;
#[cfg(windows)]
const MODE_INTERCEPTION: u8 = 1;

#[cfg(windows)]
pub fn init_backend(mode: InputMode) {
    let current = CURRENT_MODE.get_or_init(|| std::sync::atomic::AtomicU8::new(MODE_SENDINPUT));

    match mode {
        InputMode::Interception => {
            let backend_cell =
                INTERCEPTION_BACKEND.get_or_init(|| Mutex::new(InterceptionBackend::new()));
            let mut guard = backend_cell.lock().unwrap();
            // 若之前初始化失败（驱动未安装），重试一次
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
        InputMode::SendInput => {
            current.store(MODE_SENDINPUT, std::sync::atomic::Ordering::SeqCst);
            info!("输入后端已切换为 SendInput 模式");
        }
    }
}

#[cfg(windows)]
pub fn current_mode() -> InputMode {
    let mode = CURRENT_MODE
        .get()
        .map(|a| a.load(std::sync::atomic::Ordering::SeqCst))
        .unwrap_or(MODE_SENDINPUT);
    if mode == MODE_INTERCEPTION {
        InputMode::Interception
    } else {
        InputMode::SendInput
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
        if mode == MODE_INTERCEPTION {
            if let Some(lock) = INTERCEPTION_BACKEND.get() {
                if let Some(backend) = lock.lock().unwrap().as_ref() {
                    backend.send_key(vk, false);
                    return;
                }
            }
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
        if mode == MODE_INTERCEPTION {
            if let Some(lock) = INTERCEPTION_BACKEND.get() {
                if let Some(backend) = lock.lock().unwrap().as_ref() {
                    backend.send_key(vk, true);
                    return;
                }
            }
        }
        unsafe { send_via_sendinput(vk, KEYEVENTF_KEYUP) };
    }
    #[cfg(not(windows))]
    let _ = vk;
}
