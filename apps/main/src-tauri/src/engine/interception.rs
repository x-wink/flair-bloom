use super::input::SIM_MARKER;
use interception_sys::*;
use std::os::raw::c_uint;
use tracing::{error, info};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{MapVirtualKeyW, MAPVK_VK_TO_VSC_EX};

pub struct InterceptionBackend {
    ctx: InterceptionContext,
    keyboard_device: InterceptionDevice,
}

// SAFETY: InterceptionContext 是 Interception DLL 内部分配的不透明指针,
// 文档允许跨线程使用;keyboard_device 是数值 ID。本结构无可变全局状态,
// interception_send 内部对 IOCTL 串行化,故跨线程发送/共享安全。
unsafe impl Send for InterceptionBackend {}
// SAFETY: 同上
unsafe impl Sync for InterceptionBackend {}

impl InterceptionBackend {
    pub fn new() -> Option<Self> {
        // SAFETY: interception_create_context 文档允许无参调用,失败返回 null
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
        // SAFETY: MapVirtualKeyW 对任意 u32 都安全,无效 VK 返回 0
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

        // 将 SIM_MARKER 写入 information，驱动会把它转写为 KBDLLHOOKSTRUCT.dwExtraInfo
        // 低级钩子据此过滤自身注入事件，避免触发键 == 目标键时 toggle 自停
        let stroke = InterceptionKeyStroke {
            code: scan,
            state,
            information: SIM_MARKER as c_uint,
        };

        // SAFETY: ctx 是构造时已校验的有效 context;keyboard_device 来自
        // find_keyboard 校验;stroke 在调用期间存活于本栈帧;
        // InterceptionKeyStroke 与 InterceptionStroke 内存布局兼容（FFI 文档约定）
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
        // SAFETY: ctx 是 new() 中 create_context 返回的有效指针,
        // Drop 是它唯一的释放路径
        unsafe { interception_destroy_context(self.ctx) };
        info!("Interception context 已销毁");
    }
}

fn find_keyboard() -> Option<InterceptionDevice> {
    for device in 1..=(INTERCEPTION_MAX_KEYBOARD as InterceptionDevice) {
        // SAFETY: device 在文档定义的有效范围内,interception_is_keyboard 仅做查询
        if unsafe { interception_is_keyboard(device) } != 0 {
            return Some(device);
        }
    }
    error!("Interception: 未找到键盘设备");
    None
}

/// 检测 Interception 驱动是否已安装（尝试创建 context）
pub fn is_driver_installed() -> bool {
    // SAFETY: interception_create_context 文档允许无参调用,失败返回 null
    let ctx = unsafe { interception_create_context() };
    if ctx.is_null() {
        return false;
    }
    // SAFETY: ctx 是上面 create_context 返回的有效指针
    unsafe { interception_destroy_context(ctx) };
    true
}
