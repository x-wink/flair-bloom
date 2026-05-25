use interception_sys::*;
use tracing::{error, info};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{MapVirtualKeyW, MAPVK_VK_TO_VSC_EX};

pub struct InterceptionBackend {
    ctx: InterceptionContext,
    keyboard_device: InterceptionDevice,
}

unsafe impl Send for InterceptionBackend {}
unsafe impl Sync for InterceptionBackend {}

impl InterceptionBackend {
    pub fn new() -> Option<Self> {
        let ctx = unsafe { interception_create_context() };
        if ctx.is_null() {
            return None;
        }
        let device = find_keyboard()?;
        info!("Interception 后端初始化成功，键盘设备 ID: {}", device);
        Some(Self {
            ctx,
            keyboard_device: device,
        })
    }

    pub fn send_key(&self, vk: u32, is_up: bool) {
        let scan_ex = unsafe { MapVirtualKeyW(vk, MAPVK_VK_TO_VSC_EX) };
        let scan = (scan_ex & 0xFF) as u16;
        if scan == 0 {
            return;
        }
        let prefix = (scan_ex >> 8) & 0xFF;
        let mut state: u16 = if is_up {
            InterceptionKeyState_INTERCEPTION_KEY_UP as u16
        } else {
            InterceptionKeyState_INTERCEPTION_KEY_DOWN as u16
        };
        if prefix == 0xE0 {
            state |= InterceptionKeyState_INTERCEPTION_KEY_E0 as u16;
        }

        let stroke = InterceptionKeyStroke {
            code: scan,
            state,
            information: 0,
        };

        unsafe {
            interception_send(
                self.ctx,
                self.keyboard_device,
                &stroke as *const InterceptionKeyStroke as *const InterceptionStroke,
                1,
            );
        }
    }
}

impl Drop for InterceptionBackend {
    fn drop(&mut self) {
        unsafe { interception_destroy_context(self.ctx) };
        info!("Interception context 已销毁");
    }
}

fn find_keyboard() -> Option<InterceptionDevice> {
    for device in 1..=(INTERCEPTION_MAX_KEYBOARD as InterceptionDevice) {
        if unsafe { interception_is_keyboard(device) } != 0 {
            return Some(device);
        }
    }
    error!("Interception: 未找到键盘设备");
    None
}

/// 检测 Interception 驱动是否已安装（尝试创建 context）
pub fn is_driver_installed() -> bool {
    let ctx = unsafe { interception_create_context() };
    if ctx.is_null() {
        return false;
    }
    unsafe { interception_destroy_context(ctx) };
    true
}
