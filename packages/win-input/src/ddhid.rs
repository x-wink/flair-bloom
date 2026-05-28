//! DD-HID（HID-Class 虚拟设备版本）后端，DLL 名称：`ddhid.63340.dll`。

#![cfg(windows)]

use crate::dd_common::DdFfi;
use qzh_profile::key_id::MouseButton;
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

    pub fn send_mouse(&self, button: MouseButton, is_up: bool) -> bool {
        self.ffi.send_mouse(button, is_up)
    }
}
