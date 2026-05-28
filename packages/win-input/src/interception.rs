//! Interception 驱动通道。

#![cfg(windows)]

use crate::SIM_MARKER;
use interception_sys::*;
use qzh_profile::key_id::MouseButton;
use std::os::raw::c_uint;
use tracing::{error, info, warn};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{MapVirtualKeyW, MAPVK_VK_TO_VSC_EX};

pub struct InterceptionBackend {
    ctx: InterceptionContext,
    keyboard_device: InterceptionDevice,
    mouse_device: Option<InterceptionDevice>,
}

// SAFETY: InterceptionContext 是 Interception DLL 内部分配的不透明指针，文档允许跨线程使用
unsafe impl Send for InterceptionBackend {}
// SAFETY: 同上
unsafe impl Sync for InterceptionBackend {}

impl InterceptionBackend {
    pub fn new() -> Option<Self> {
        // SAFETY: interception_create_context 失败返回 null
        let ctx = unsafe { interception_create_context() };
        if ctx.is_null() {
            return None;
        }
        let keyboard_device = find_keyboard()?;
        let mouse_device = find_mouse();
        info!(
            "Interception 后端初始化成功，键盘 ID: {}, 鼠标 ID: {:?}",
            keyboard_device, mouse_device
        );
        Some(Self {
            ctx,
            keyboard_device,
            mouse_device,
        })
    }

    pub fn send_key(&self, vk: u32, is_up: bool) {
        // SAFETY: MapVirtualKeyW 对任意 u32 安全
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
            information: SIM_MARKER as c_uint,
        };
        // SAFETY: ctx/keyboard_device 已验证；stroke 在调用期间在栈上
        unsafe {
            interception_send(
                self.ctx,
                self.keyboard_device,
                &stroke as *const InterceptionKeyStroke as *const InterceptionStroke,
                1,
            );
        }
    }

    pub fn send_mouse(&self, button: MouseButton, is_up: bool) -> bool {
        let Some(mouse_device) = self.mouse_device else {
            return false;
        };
        let state: u16 = match (button, is_up) {
            (MouseButton::Left, false) => {
                InterceptionMouseState_INTERCEPTION_MOUSE_LEFT_BUTTON_DOWN as u16
            }
            (MouseButton::Left, true) => {
                InterceptionMouseState_INTERCEPTION_MOUSE_LEFT_BUTTON_UP as u16
            }
            (MouseButton::Right, false) => {
                InterceptionMouseState_INTERCEPTION_MOUSE_RIGHT_BUTTON_DOWN as u16
            }
            (MouseButton::Right, true) => {
                InterceptionMouseState_INTERCEPTION_MOUSE_RIGHT_BUTTON_UP as u16
            }
            (MouseButton::Middle, false) => {
                InterceptionMouseState_INTERCEPTION_MOUSE_MIDDLE_BUTTON_DOWN as u16
            }
            (MouseButton::Middle, true) => {
                InterceptionMouseState_INTERCEPTION_MOUSE_MIDDLE_BUTTON_UP as u16
            }
            (MouseButton::X1, false) => {
                InterceptionMouseState_INTERCEPTION_MOUSE_BUTTON_4_DOWN as u16
            }
            (MouseButton::X1, true) => {
                InterceptionMouseState_INTERCEPTION_MOUSE_BUTTON_4_UP as u16
            }
            (MouseButton::X2, false) => {
                InterceptionMouseState_INTERCEPTION_MOUSE_BUTTON_5_DOWN as u16
            }
            (MouseButton::X2, true) => {
                InterceptionMouseState_INTERCEPTION_MOUSE_BUTTON_5_UP as u16
            }
        };
        let stroke = InterceptionMouseStroke {
            state,
            flags: 0,
            rolling: 0,
            x: 0,
            y: 0,
            information: SIM_MARKER as c_uint,
        };
        // SAFETY: ctx/mouse_device 已验证；stroke 在调用期间在栈上
        unsafe {
            interception_send(
                self.ctx,
                mouse_device,
                &stroke as *const InterceptionMouseStroke as *const InterceptionStroke,
                1,
            );
        }
        true
    }
}

impl Drop for InterceptionBackend {
    fn drop(&mut self) {
        // SAFETY: ctx 是 new() 中 create_context 返回的唯一句柄
        unsafe { interception_destroy_context(self.ctx) };
        info!("Interception context 已销毁");
    }
}

fn find_keyboard() -> Option<InterceptionDevice> {
    for device in 1..=(INTERCEPTION_MAX_KEYBOARD as InterceptionDevice) {
        // SAFETY: device 在有效范围内
        if unsafe { interception_is_keyboard(device) } != 0 {
            return Some(device);
        }
    }
    error!("Interception: 未找到键盘设备");
    None
}

fn find_mouse() -> Option<InterceptionDevice> {
    let start = (INTERCEPTION_MAX_KEYBOARD as InterceptionDevice) + 1;
    let end = INTERCEPTION_MAX_DEVICE as InterceptionDevice;
    for device in start..=end {
        // SAFETY: device 在有效范围内
        if unsafe { interception_is_mouse(device) } != 0 {
            return Some(device);
        }
    }
    warn!("Interception: 未找到鼠标设备，鼠标注入将被丢弃");
    None
}

/// 检测 Interception 驱动是否已安装（尝试创建 context）。
pub fn is_driver_installed() -> bool {
    // SAFETY: interception_create_context 失败返回 null
    let ctx = unsafe { interception_create_context() };
    if ctx.is_null() {
        return false;
    }
    // SAFETY: ctx 是上面返回的有效指针
    unsafe { interception_destroy_context(ctx) };
    true
}
