//! ddxoft DD SDK 的 FFI 装载层，由 [`crate::ddhid`] 共用。
//!
//! 设计要点：
//! - DLL 在运行时通过 `LoadLibraryW` 加载，避免编译期链接到不存在的导入库；
//! - DD 协议要求首次调用 `DD_btn(0)`，返回 `1` 才表示内核驱动已就绪；
//! - 暴露键盘 `DD_key` / `DD_todc` 与鼠标 `DD_btn`。X1/X2 不在 DD_btn 值域，调用方
//!   按返回值决定是否回退到 SendInput。

#![cfg(windows)]

use qzh_profile::key_id::MouseButton;
use std::ffi::c_void;
use std::os::raw::c_int;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{info, warn};
use windows_sys::Win32::Foundation::FreeLibrary;
use windows_sys::Win32::Foundation::HMODULE;
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

type DdBtnFn = unsafe extern "C" fn(c_int) -> c_int;
type DdKeyFn = unsafe extern "C" fn(c_int, c_int) -> c_int;
type DdTodcFn = unsafe extern "C" fn(c_int) -> c_int;
type DdWhlFn = unsafe extern "C" fn(c_int) -> c_int;

pub struct DdFfi {
    handle: HMODULE,
    dd_btn: DdBtnFn,
    dd_key: DdKeyFn,
    dd_todc: DdTodcFn,
    dd_whl: Option<DdWhlFn>,
    diag_logged: AtomicBool,
    mouse_diag_logged: AtomicBool,
    mouse_x1x2_warned: AtomicBool,
    wheel_diag_logged: AtomicBool,
}

// SAFETY: HMODULE 在 64 位 Windows 上是地址不变的内核句柄
unsafe impl Send for DdFfi {}
// SAFETY: 同上
unsafe impl Sync for DdFfi {}

impl DdFfi {
    pub fn load(dll_path: &Path) -> Option<Self> {
        if !dll_path.exists() {
            warn!("DD DLL 不存在：{}", dll_path.display());
            return None;
        }
        let wide: Vec<u16> = dll_path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: wide 以 NUL 结尾
        let handle = unsafe { LoadLibraryW(wide.as_ptr()) };
        if handle.is_null() {
            warn!("LoadLibraryW 失败：{}", dll_path.display());
            return None;
        }
        // SAFETY: handle 有效，符号名 NUL 结尾，类型与 DLL 文档一致
        let dd_btn = match unsafe { resolve::<DdBtnFn>(handle, b"DD_btn\0") } {
            Some(f) => f,
            None => {
                unsafe { FreeLibrary(handle) };
                warn!("DD DLL 缺少 DD_btn 导出");
                return None;
            }
        };
        let dd_key = match unsafe { resolve::<DdKeyFn>(handle, b"DD_key\0") } {
            Some(f) => f,
            None => {
                unsafe { FreeLibrary(handle) };
                warn!("DD DLL 缺少 DD_key 导出");
                return None;
            }
        };
        let dd_todc = match unsafe { resolve::<DdTodcFn>(handle, b"DD_todc\0") } {
            Some(f) => f,
            None => {
                unsafe { FreeLibrary(handle) };
                warn!("DD DLL 缺少 DD_todc 导出");
                return None;
            }
        };
        // SAFETY: dd_btn 已解析，DD_btn(0) 是协议规定的自检调用
        let st = unsafe { dd_btn(0) };
        if st != 1 {
            warn!("DD_btn(0) 返回 {}，驱动未就绪", st);
            unsafe { FreeLibrary(handle) };
            return None;
        }
        // DD_whl 是可选扩展：两个已知 DLL 版本均有此导出，旧版不报错仅降级
        let dd_whl = unsafe { resolve::<DdWhlFn>(handle, b"DD_whl\0") };
        if dd_whl.is_none() {
            warn!("DD DLL 缺少 DD_whl 导出，滚轮将回退 SendInput");
        }
        Some(DdFfi {
            handle,
            dd_btn,
            dd_key,
            dd_todc,
            dd_whl,
            diag_logged: AtomicBool::new(false),
            mouse_diag_logged: AtomicBool::new(false),
            mouse_x1x2_warned: AtomicBool::new(false),
            wheel_diag_logged: AtomicBool::new(false),
        })
    }

    pub fn send_key(&self, vk: u32, is_up: bool) {
        // SAFETY: dd_todc 已解析
        let ddcode = unsafe { (self.dd_todc)(vk as c_int) };
        let first = !self.diag_logged.swap(true, Ordering::SeqCst);
        if ddcode == 0 {
            if first {
                warn!("DD_todc({:#x}) 返回 0，VK 无映射，键已丢弃", vk);
            }
            return;
        }
        let flag = if is_up { 2 } else { 1 };
        // SAFETY: dd_key 已解析
        let ret = unsafe { (self.dd_key)(ddcode, flag) };
        if first {
            info!(
                "DD 首次注入：vk={:#x} ddcode={} flag={} ret={}",
                vk, ddcode, flag, ret
            );
        }
    }

    /// 注入滚轮事件。`up=true` 时向上（正 delta），`up=false` 时向下。
    /// 返回 `true` 表示 DD 通道已处理，`false` 表示需回退 SendInput。
    pub fn send_wheel(&self, up: bool) -> bool {
        let Some(dd_whl) = self.dd_whl else {
            return false;
        };
        // DD_whl 接收有符号字节：正值向上，负值向下；按 HID 每格 1 单位
        let delta: c_int = if up { 1 } else { -1 };
        // SAFETY: dd_whl 已解析
        let ret = unsafe { dd_whl(delta) };
        if !self.wheel_diag_logged.swap(true, Ordering::SeqCst) {
            info!("DD 首次滚轮注入：up={} delta={} ret={}", up, delta, ret);
        }
        true
    }

    pub fn send_mouse(&self, button: MouseButton, is_up: bool) -> bool {
        let flag: c_int = match (button, is_up) {
            (MouseButton::Left, false) => 1,
            (MouseButton::Left, true) => 2,
            (MouseButton::Right, false) => 4,
            (MouseButton::Right, true) => 8,
            (MouseButton::Middle, false) => 16,
            (MouseButton::Middle, true) => 32,
            (MouseButton::X1 | MouseButton::X2, _) => {
                if !self.mouse_x1x2_warned.swap(true, Ordering::SeqCst) {
                    warn!("DD-HID 不支持 X1/X2 鼠标按钮，回退到 SendInput");
                }
                return false;
            }
            // WheelUp/WheelDown 由 dispatch 提前路由到 send_wheel，不应到达此处
            (MouseButton::WheelUp | MouseButton::WheelDown, _) => unreachable!(),
        };
        // SAFETY: dd_btn 已解析
        let ret = unsafe { (self.dd_btn)(flag) };
        if !self.mouse_diag_logged.swap(true, Ordering::SeqCst) {
            info!(
                "DD 首次鼠标注入：button={:?} is_up={} flag={} ret={}",
                button, is_up, flag, ret
            );
        }
        true
    }
}

impl Drop for DdFfi {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: handle 是 load() 中 LoadLibraryW 返回的唯一句柄
            unsafe { FreeLibrary(self.handle) };
        }
    }
}

unsafe fn resolve<T: Copy>(handle: HMODULE, name_with_nul: &[u8]) -> Option<T> {
    debug_assert!(name_with_nul.last() == Some(&0));
    debug_assert!(std::mem::size_of::<T>() == std::mem::size_of::<*const c_void>());
    // SAFETY: handle 存活、字符串 NUL 结尾
    let p = GetProcAddress(handle, name_with_nul.as_ptr());
    p.map(|f| std::mem::transmute_copy::<_, T>(&f))
}
