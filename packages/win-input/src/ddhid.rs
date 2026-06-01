//! DD-HID（HID-Class 虚拟设备版本）后端，DLL 名称：`ddhid.63340.dll`。

#![cfg(windows)]

use crate::dd_common::DdFfi;
use qzh_profile::key_id::MouseButton;
use std::path::Path;
use tracing::{info, warn};

/// DD-HID 驱动版本号，与 [`win_driver::dd_hid::DD_HID_VERSION`] 保持同步。
pub const DD_HID_VERSION: &str = "63340";
pub const DLL_NAME: &str = "ddhid.63340.dll";
pub(crate) const DD_HID_SERVICE_NAME: &str = "ddhid63340";

// sc.exe 服务相关错误码
const ERROR_SERVICE_ALREADY_RUNNING: u32 = 1056;
const ERROR_SERVICE_NOT_ACTIVE: u32 = 1062;
const ERROR_SERVICE_DOES_NOT_EXIST: u32 = 1060;

pub struct DdHidBackend {
    ffi: DdFfi,
}

impl DdHidBackend {
    pub fn new(resources_dir: &Path) -> Option<Self> {
        let dll = resources_dir.join(DLL_NAME);
        let ffi = DdFfi::load(&dll)?;
        info!("DD-HID 后端初始化成功");
        Some(Self { ffi })
    }

    pub fn send_key(&self, vk: u32, is_up: bool) {
        self.ffi.send_key(vk, is_up);
    }

    pub fn send_mouse(&self, button: MouseButton, is_up: bool) -> bool {
        self.ffi.send_mouse(button, is_up)
    }

    pub fn send_wheel(&self, up: bool) -> bool {
        self.ffi.send_wheel(up)
    }
}

/// 启用并启动 DD-HID 驱动服务。
///
/// 每次切入 DD-HID 模式时调用：上次退出已将服务设为 disabled，此处先恢复
/// 为 demand-start 再 start，`ERROR_SERVICE_ALREADY_RUNNING` 时静默忽略。
pub(crate) fn start_service() {
    use core::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::System::Services::{
        ChangeServiceConfigW, CloseServiceHandle, OpenSCManagerW, OpenServiceW, StartServiceW,
        SC_MANAGER_CONNECT, SERVICE_CHANGE_CONFIG, SERVICE_DEMAND_START, SERVICE_NO_CHANGE,
        SERVICE_START,
    };

    let svc_name: Vec<u16> = DD_HID_SERVICE_NAME
        .encode_utf16()
        .chain(Some(0))
        .collect();

    // SAFETY: SCM API 按文档调用；所有句柄在函数结束前显式关闭
    unsafe {
        let scm = OpenSCManagerW(null(), null(), SC_MANAGER_CONNECT);
        if scm.is_null() {
            warn!("DD-HID 服务启动：无法打开 SCM (Win32 错误 {})", GetLastError());
            return;
        }

        let svc = OpenServiceW(scm, svc_name.as_ptr(), SERVICE_START | SERVICE_CHANGE_CONFIG);
        if svc.is_null() {
            let err = GetLastError();
            CloseServiceHandle(scm);
            warn!(
                "DD-HID 服务启动：无法打开服务 {} (Win32 错误 {})",
                DD_HID_SERVICE_NAME, err
            );
            return;
        }

        // 将 disabled 改回 demand-start，以便 StartServiceW 能够执行
        if ChangeServiceConfigW(
            svc,
            SERVICE_NO_CHANGE,
            SERVICE_DEMAND_START,
            SERVICE_NO_CHANGE,
            null(),
            null(),
            null_mut(),
            null(),
            null(),
            null(),
            null(),
        ) == 0
        {
            warn!(
                "DD-HID 服务启动：设置启动类型失败 (Win32 错误 {})",
                GetLastError()
            );
        }

        let r = StartServiceW(svc, 0, null());
        if r == 0 {
            let err = GetLastError();
            if err != ERROR_SERVICE_ALREADY_RUNNING {
                warn!("DD-HID 服务启动：StartServiceW 失败 (Win32 错误 {})", err);
            }
        } else {
            info!("DD-HID 驱动服务已启动");
        }

        CloseServiceHandle(svc);
        CloseServiceHandle(scm);
    }
}

/// 停止并禁用 DD-HID 驱动服务（将 Start 设为 4/disabled）。
///
/// 离开 DD-HID 模式时调用，确保关机或快速启动后服务不会自动加载。
pub(crate) fn stop_and_disable_service() {
    use core::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::System::Services::{
        ChangeServiceConfigW, CloseServiceHandle, ControlService, OpenSCManagerW, OpenServiceW,
        SC_MANAGER_CONNECT, SERVICE_CHANGE_CONFIG, SERVICE_CONTROL_STOP, SERVICE_DISABLED,
        SERVICE_NO_CHANGE, SERVICE_STATUS, SERVICE_STOP,
    };

    let svc_name: Vec<u16> = DD_HID_SERVICE_NAME
        .encode_utf16()
        .chain(Some(0))
        .collect();

    // SAFETY: SCM API 按文档调用；所有句柄在函数结束前显式关闭
    unsafe {
        let scm = OpenSCManagerW(null(), null(), SC_MANAGER_CONNECT);
        if scm.is_null() {
            warn!("DD-HID 服务停用：无法打开 SCM (Win32 错误 {})", GetLastError());
            return;
        }

        let svc = OpenServiceW(scm, svc_name.as_ptr(), SERVICE_STOP | SERVICE_CHANGE_CONFIG);
        if svc.is_null() {
            let err = GetLastError();
            CloseServiceHandle(scm);
            if err != ERROR_SERVICE_DOES_NOT_EXIST {
                warn!(
                    "DD-HID 服务停用：无法打开服务 {} (Win32 错误 {})",
                    DD_HID_SERVICE_NAME, err
                );
            }
            return;
        }

        let mut status: SERVICE_STATUS = core::mem::zeroed();
        if ControlService(svc, SERVICE_CONTROL_STOP, &mut status) == 0 {
            let err = GetLastError();
            if err != ERROR_SERVICE_NOT_ACTIVE {
                warn!("DD-HID 服务停用：ControlService 失败 (Win32 错误 {}，忽略)", err);
            }
        }

        if ChangeServiceConfigW(
            svc,
            SERVICE_NO_CHANGE,
            SERVICE_DISABLED,
            SERVICE_NO_CHANGE,
            null(),
            null(),
            null_mut(),
            null(),
            null(),
            null(),
            null(),
        ) == 0
        {
            warn!(
                "DD-HID 服务停用：禁用服务失败 (Win32 错误 {})",
                GetLastError()
            );
        } else {
            info!("DD-HID 驱动服务已停止并禁用");
        }

        CloseServiceHandle(svc);
        CloseServiceHandle(scm);
    }
}
