//! ddxoft DD SDK 的 FFI 装载层，由 [`super::dd`] 与 [`super::ddhid`] 共用。
//!
//! 设计要点：
//! - DLL 在运行时通过 `LoadLibraryW` 加载，避免编译期链接到不存在的导入库；
//! - DD 协议要求首次调用 `DD_btn(0)`，返回 `1` 才表示内核驱动已就绪；
//! - 仅暴露键盘相关函数（`DD_key` / `DD_todc`），鼠标功能暂不使用。

#![cfg(windows)]

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

pub struct DdFfi {
    handle: HMODULE,
    dd_key: DdKeyFn,
    dd_todc: DdTodcFn,
    /// 仅在首次注入时打印一次诊断日志，避免连发循环把日志塞满
    diag_logged: AtomicBool,
}

// SAFETY: HMODULE 在 64 位 Windows 上是地址不变的内核句柄,DLL 一旦加载到进程
// 地址空间生命周期覆盖整个 DdFfi（仅在 Drop 中 FreeLibrary）;函数指针解析后
// 地址固定不变。DdFfi 内部无可变全局状态,DD 内部对每次调用使用其驱动 IOCTL
// 串行化,故跨线程发送/共享安全。
unsafe impl Send for DdFfi {}
// SAFETY: 同上
unsafe impl Sync for DdFfi {}

impl DdFfi {
    /// 加载 DD DLL，调用 `DD_btn(0)` 自检；返回 `None` 表示驱动未就绪或导出符号缺失
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
        // SAFETY: wide 以 NUL 结尾,LoadLibraryW 仅在调用期间读取该指针
        let handle = unsafe { LoadLibraryW(wide.as_ptr()) };
        if handle.is_null() {
            warn!("LoadLibraryW 失败：{}", dll_path.display());
            return None;
        }

        // SAFETY: handle 来自上面 LoadLibraryW 成功返回,resolve 的契约：
        // 字节串 NUL 结尾(b"DD_btn\0")且类型 DdBtnFn 与 DD DLL 文档签名一致
        let dd_btn = match unsafe { resolve::<DdBtnFn>(handle, b"DD_btn\0") } {
            Some(f) => f,
            None => {
                // SAFETY: handle 是上面 LoadLibraryW 返回的有效句柄
                unsafe { FreeLibrary(handle) };
                warn!("DD DLL 缺少 DD_btn 导出");
                return None;
            }
        };
        // SAFETY: 同上
        let dd_key = match unsafe { resolve::<DdKeyFn>(handle, b"DD_key\0") } {
            Some(f) => f,
            None => {
                // SAFETY: handle 仍有效
                unsafe { FreeLibrary(handle) };
                warn!("DD DLL 缺少 DD_key 导出");
                return None;
            }
        };
        // SAFETY: 同上
        let dd_todc = match unsafe { resolve::<DdTodcFn>(handle, b"DD_todc\0") } {
            Some(f) => f,
            None => {
                // SAFETY: handle 仍有效
                unsafe { FreeLibrary(handle) };
                warn!("DD DLL 缺少 DD_todc 导出");
                return None;
            }
        };

        // DD 协议自检：返回 1 表示驱动已加载并可正常通信
        // SAFETY: dd_btn 已通过 resolve 解析为正确签名的函数指针,DD_btn 文档允许传 0
        let st = unsafe { dd_btn(0) };
        if st != 1 {
            warn!("DD_btn(0) 返回 {}，驱动未就绪", st);
            // SAFETY: handle 仍有效
            unsafe { FreeLibrary(handle) };
            return None;
        }

        Some(DdFfi {
            handle,
            dd_key,
            dd_todc,
            diag_logged: AtomicBool::new(false),
        })
    }

    /// 注入虚拟键。`is_up = false` 为按下，`true` 为抬起。
    /// 首次调用打印诊断信息，便于排查「驱动加载成功但游戏无响应」之类问题。
    /// 失败（VK 无对应 DD 码）静默忽略，与 `SendInput` 的扫描码缺失行为一致。
    pub fn send_key(&self, vk: u32, is_up: bool) {
        // SAFETY: dd_todc 是从 DD DLL 解析的函数指针,接受任意 c_int VK
        let ddcode = unsafe { (self.dd_todc)(vk as c_int) };
        let first = !self.diag_logged.swap(true, Ordering::SeqCst);
        if ddcode == 0 {
            if first {
                warn!("DD_todc({:#x}) 返回 0，VK 无映射，键已丢弃", vk);
            }
            return;
        }
        let flag = if is_up { 2 } else { 1 };
        // SAFETY: dd_key 是从 DD DLL 解析的函数指针,(ddcode, 1|2) 是 DD 协议
        // 文档定义的合法参数（按下/抬起）
        let ret = unsafe { (self.dd_key)(ddcode, flag) };
        if first {
            info!(
                "DD 首次注入：vk={:#x} ddcode={} flag={} ret={}",
                vk, ddcode, flag, ret
            );
        }
    }
}

impl Drop for DdFfi {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: handle 是 load() 中 LoadLibraryW 返回的有效句柄,
            // 此处 Drop 是它唯一的释放路径
            unsafe { FreeLibrary(self.handle) };
        }
    }
}

/// 从已加载 DLL 中解析符号并按调用方指定的 ABI 类型返回函数指针。
///
/// # Safety
///
/// - `handle` 必须仍然存活（未 `FreeLibrary`）
/// - `name_with_nul` 必须以 `\0` 结尾,且为合法 C 字符串
/// - 类型 `T` 必须与 DLL 中该符号的实际 ABI 与签名完全匹配,否则后续调用 UB
unsafe fn resolve<T: Copy>(handle: HMODULE, name_with_nul: &[u8]) -> Option<T> {
    debug_assert!(name_with_nul.last() == Some(&0));
    debug_assert!(std::mem::size_of::<T>() == std::mem::size_of::<*const c_void>());
    // SAFETY: 上文 # Safety 契约保证 handle 存活、字符串 NUL 结尾
    let p = GetProcAddress(handle, name_with_nul.as_ptr());
    // SAFETY: GetProcAddress 返回的函数指针与调用方声明类型 T 必须按 # Safety
    // 契约由调用方保证一致;debug_assert 校验大小相等以防低级错配
    p.map(|f| std::mem::transmute_copy::<_, T>(&f))
}
