//! DD-HID（HID-Class 虚拟设备版本）后端，DLL 名称：`ddhid.63340.dll`。
//!
//! 底层是 PnP 注册的 WHQL 签名 HID 驱动，由 `ddc.exe` 完成安装/卸载，无需 SCM 启动；
//! DLL 调用同样要求宿主进程具备管理员权限（DeviceIoControl 入口受 ACL 保护）。

#![cfg(windows)]

use super::dd_common::DdFfi;
use qzh_format::key_id::MouseButton;
use std::path::Path;
use tracing::info;

pub const DLL_NAME: &str = "ddhid.63340.dll";

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

    /// 返回 `true` 表示由 DD 完成；`false` 表示 X1/X2 等 DD SDK 不支持的按钮，
    /// 调用方需回退到 SendInput。
    pub fn send_mouse(&self, button: MouseButton, is_up: bool) -> bool {
        self.ffi.send_mouse(button, is_up)
    }
}
