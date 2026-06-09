//! 绕过 DD DLL，直接向 DDSimple 驱动设备（`\\.\dd63330`）发 DeviceIoControl，
//! 将 `SIM_MARKER` 写入 `ExtraInformation`，使 hook 端可以用 `dwExtraInfo == SIM_MARKER`
//! 精确过滤，无需依赖时间窗口的 `PENDING_INJECTIONS` 队列。
//!
//! 逆向依据：
//! - 设备路径：dd63330.dll .rdata RVA 0x212f78（UTF-16LE `\\.\dd63330`）
//! - `IOCTL_KEYBOARD` 0x9c403c10：DD_key VA=0x180003ff5 `mov edx, 0x9c403c10`
//! - `IOCTL_MOUSE`    0x9c403c0c：DD_btn VA=0x1800040b6 `mov edx, 0x9c403c0c`
//! - DLL 中 `ExtraInformation` 在 KEYBOARD_INPUT_DATA（VA=0x180003f5a）和
//!   MOUSE_INPUT_DATA（VA=0x18000407a）均硬编码为 0，无法从外部注入。

#![cfg(windows)]

use crate::SIM_MARKER;
use qzh_profile::key_id::MouseButton;
use std::mem::size_of;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{info, warn};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, OPEN_EXISTING};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{MapVirtualKeyW, MAPVK_VK_TO_VSC_EX};

/// 键盘输入 IOCTL 码（驱动接收 KEYBOARD_INPUT_DATA，12 字节）
const IOCTL_KEYBOARD: u32 = 0x9c403c10;
/// 鼠标输入 IOCTL 码（驱动接收 MOUSE_INPUT_DATA，24 字节）
const IOCTL_MOUSE: u32 = 0x9c403c0c;

/// KEYBOARD_INPUT_DATA.Flags：按键抬起
const KEY_BREAK: u16 = 0x0001;
/// KEYBOARD_INPUT_DATA.Flags：E0 扩展键前缀（右 Ctrl/Alt/Insert 等）
const KEY_E0: u16 = 0x0002;

const GENERIC_WRITE: u32 = 0x4000_0000;
const FILE_SHARE_READ: u32 = 0x0000_0001;
const FILE_SHARE_WRITE: u32 = 0x0000_0002;

/// WDM `KEYBOARD_INPUT_DATA`（12 字节，layout 与内核结构体完全对应）
#[repr(C)]
struct KeyboardInputData {
    unit_id: u16,
    make_code: u16,
    flags: u16,
    reserved: u16,
    extra_information: u32,
}

/// WDM `MOUSE_INPUT_DATA`（24 字节）。`Buttons` union 拆分为 `button_flags` + `button_data`，
/// 在小端序下 offset 4/6 与联合体字段完全对齐。
#[repr(C)]
struct MouseInputData {
    unit_id: u16,
    flags: u16,
    button_flags: u16,
    button_data: u16,
    raw_buttons: u32,
    last_x: i32,
    last_y: i32,
    extra_information: u32,
}

/// 持有对 DDSimple 驱动的独立设备句柄，注入时写入 `SIM_MARKER`。
pub struct DdSimpleDirectIo {
    handle: HANDLE,
    kbd_first_logged: AtomicBool,
    mouse_first_logged: AtomicBool,
}

// SAFETY: DeviceIoControl 发送同步请求，每次调用使用栈上独立的输入缓冲区，
// 驱动端处理无线程关联状态，句柄跨线程调用安全。
unsafe impl Send for DdSimpleDirectIo {}
unsafe impl Sync for DdSimpleDirectIo {}

impl DdSimpleDirectIo {
    /// 打开 `\\.\dd63330` 设备，返回 `None` 表示驱动未就绪或句柄冲突。
    ///
    /// 使用 `FILE_SHARE_READ | FILE_SHARE_WRITE` 允许与 DLL 的句柄共存；
    /// `GENERIC_WRITE` 满足 DeviceIoControl 写入需求。
    pub fn new() -> Option<Self> {
        let path: Vec<u16> = "\\\\.\\dd63330"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: path NUL 结尾；其余参数均为合法 Windows API 值
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null::<SECURITY_ATTRIBUTES>(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            warn!("DDSimple 直接 IO：无法打开 \\\\.\\dd63330，降级为 DLL 模式");
            return None;
        }

        // DDSimple 驱动要求先经过 DD_btn(0) 握手才接受注入 IOCTL；独立句柄未握手时
        // DeviceIoControl 静默返回 0，导致注入丢失。发无害探针（零位移、零按钮的
        // MOUSE_INPUT_DATA）确认驱动接受本句柄，否则关闭句柄、降级为 DLL 模式。
        let mut probe = MouseInputData {
            unit_id: 1,
            flags: 0,
            button_flags: 0,
            button_data: 0,
            raw_buttons: 0,
            last_x: 0,
            last_y: 0,
            extra_information: SIM_MARKER as u32,
        };
        let mut probe_bytes = 0u32;
        // SAFETY: handle 有效；probe 是正确布局的 MOUSE_INPUT_DATA（24 字节）
        let probe_ok = unsafe {
            DeviceIoControl(
                handle,
                IOCTL_MOUSE,
                std::ptr::addr_of_mut!(probe).cast(),
                size_of::<MouseInputData>() as u32,
                std::ptr::null_mut(),
                0,
                &mut probe_bytes,
                std::ptr::null_mut(),
            )
        };
        if probe_ok == 0 {
            warn!("DDSimple 直接 IO：探针 IOCTL 失败（驱动需握手），降级为 DLL 模式");
            // SAFETY: handle 是刚才 CreateFileW 返回的有效句柄
            unsafe { CloseHandle(handle) };
            return None;
        }

        info!("DDSimple 直接 IO：探针 IOCTL 成功，ExtraInformation 将写入 SIM_MARKER");
        Some(Self {
            handle,
            kbd_first_logged: AtomicBool::new(false),
            mouse_first_logged: AtomicBool::new(false),
        })
    }

    /// 注入键盘事件，`ExtraInformation = SIM_MARKER`。
    ///
    /// 返回 `Some(ok)`：已尝试 DeviceIoControl（`true` = 驱动确认，`false` = 驱动拒绝）。
    /// 返回 `None`：此 VK 无法通过 `MapVirtualKeyW` 得到 scan code（E1 前缀或无效键），
    /// 调用方应降级到 DLL 路径。
    pub fn send_key(&self, vk: u32, is_up: bool) -> Option<bool> {
        // SAFETY: MapVirtualKeyW 对任意 u32 vk 安全
        let scan_ex = unsafe { MapVirtualKeyW(vk, MAPVK_VK_TO_VSC_EX) };
        let scan = (scan_ex & 0xFF) as u16;
        let prefix = (scan_ex >> 8) as u8;

        if scan == 0 || prefix == 0xE1 {
            // Pause/Break 等极少数使用 E1 序列或无 scan code 的键，交给 DLL 处理
            return None;
        }

        let mut flags = if is_up { KEY_BREAK } else { 0 };
        if prefix == 0xE0 {
            flags |= KEY_E0;
        }

        let mut data = KeyboardInputData {
            unit_id: 1,
            make_code: scan,
            flags,
            reserved: 0,
            extra_information: SIM_MARKER as u32,
        };
        let mut bytes = 0u32;
        // SAFETY: handle 有效；data 是正确布局的 KEYBOARD_INPUT_DATA（12 字节）
        let ok = unsafe {
            DeviceIoControl(
                self.handle,
                IOCTL_KEYBOARD,
                std::ptr::addr_of_mut!(data).cast(),
                size_of::<KeyboardInputData>() as u32,
                std::ptr::null_mut(),
                0,
                &mut bytes,
                std::ptr::null_mut(),
            )
        };
        let succeeded = ok != 0;
        if !self.kbd_first_logged.swap(true, Ordering::SeqCst) {
            if succeeded {
                info!(
                    "DDSimple 直接 IO 首次键盘注入：vk={:#x} scan={:#x} flags={:#x}",
                    vk, scan, flags
                );
            } else {
                warn!(
                    "DDSimple 直接 IO 键盘 DeviceIoControl 失败：vk={:#x} scan={:#x}",
                    vk, scan
                );
            }
        }
        Some(succeeded)
    }

    /// 注入鼠标按钮事件（不含滚轮），`ExtraInformation = SIM_MARKER`。
    pub fn send_mouse(&self, button: MouseButton, is_up: bool) -> bool {
        let button_flags: u16 = match (button, is_up) {
            (MouseButton::Left, false) => 0x0001,
            (MouseButton::Left, true) => 0x0002,
            (MouseButton::Right, false) => 0x0004,
            (MouseButton::Right, true) => 0x0008,
            (MouseButton::Middle, false) => 0x0010,
            (MouseButton::Middle, true) => 0x0020,
            (MouseButton::X1, false) => 0x0040,
            (MouseButton::X1, true) => 0x0080,
            (MouseButton::X2, false) => 0x0100,
            (MouseButton::X2, true) => 0x0200,
            (MouseButton::WheelUp | MouseButton::WheelDown, _) => unreachable!(),
        };
        self.ioctl_mouse(button_flags, 0)
    }

    /// 注入滚轮事件，`ExtraInformation = SIM_MARKER`。
    /// `ButtonFlags = MOUSE_WHEEL (0x0400)`，`ButtonData = ±120`（标准滚轮 delta）。
    pub fn send_wheel(&self, up: bool) -> bool {
        let button_data = if up { 120u16 } else { (-120i16) as u16 };
        self.ioctl_mouse(0x0400, button_data)
    }

    fn ioctl_mouse(&self, button_flags: u16, button_data: u16) -> bool {
        let mut data = MouseInputData {
            unit_id: 1,
            flags: 0,
            button_flags,
            button_data,
            raw_buttons: 0,
            last_x: 0,
            last_y: 0,
            extra_information: SIM_MARKER as u32,
        };
        let mut bytes = 0u32;
        // SAFETY: handle 有效；data 是正确布局的 MOUSE_INPUT_DATA（24 字节）
        let ok = unsafe {
            DeviceIoControl(
                self.handle,
                IOCTL_MOUSE,
                std::ptr::addr_of_mut!(data).cast(),
                size_of::<MouseInputData>() as u32,
                std::ptr::null_mut(),
                0,
                &mut bytes,
                std::ptr::null_mut(),
            )
        };
        let succeeded = ok != 0;
        if !self.mouse_first_logged.swap(true, Ordering::SeqCst) {
            if succeeded {
                info!(
                    "DDSimple 直接 IO 首次鼠标注入：button_flags={:#06x} button_data={}",
                    button_flags, button_data as i16
                );
            } else {
                warn!(
                    "DDSimple 直接 IO 鼠标 DeviceIoControl 失败：button_flags={:#06x}",
                    button_flags
                );
            }
        }
        succeeded
    }
}

impl Drop for DdSimpleDirectIo {
    fn drop(&mut self) {
        if self.handle != INVALID_HANDLE_VALUE {
            // SAFETY: handle 是 new() 中 CreateFileW 返回的有效句柄
            unsafe { CloseHandle(self.handle) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_input_data_is_12_bytes() {
        assert_eq!(size_of::<KeyboardInputData>(), 12);
    }

    #[test]
    fn mouse_input_data_is_24_bytes() {
        assert_eq!(size_of::<MouseInputData>(), 24);
    }

    #[test]
    fn sim_marker_fits_in_u32_without_truncation() {
        assert_eq!(SIM_MARKER as u32 as usize, SIM_MARKER);
    }
}
