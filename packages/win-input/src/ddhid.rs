//! DD-HID（HID-Class 虚拟设备版本）后端，DLL 名称：`ddhid.[`DLL_VERSION`].dll`。

#![cfg(windows)]

use crate::dd_common::DdFfi;
use qzh_profile::key_id::MouseButton;
use std::path::Path;
use tracing::info;

macro_rules! dd_hid_version {
    () => {
        "63340"
    };
}

/// DD-HID DLL 的版本号，同时也是驱动服务名（`ddhid63340.sys`）的后缀。
pub const DLL_VERSION: &str = dd_hid_version!();
pub const DLL_NAME: &str = concat!("ddhid.", dd_hid_version!(), ".dll");

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
