//! DD Simple 后端，使用 `dd63330.dll`，独立于 DD-HID 驱动安装/卸载链路。
#![cfg(windows)]

use crate::dd_common::{DdFfi, DdSideButtonMode};
use qzh_profile::key_id::MouseButton;
use std::path::Path;
use tracing::info;

pub const DLL_VERSION: &str = "63330";
pub const DLL_NAME: &str = "dd63330.dll";

pub struct DdSimpleBackend {
    ffi: DdFfi,
}

impl DdSimpleBackend {
    pub fn new(resources_dir: &Path) -> Option<Self> {
        let dll = resources_dir.join(DLL_NAME);
        let ffi = DdFfi::load(&dll, DdSideButtonMode::SimpleMouseInputDataFlags)?;
        info!("DD Simple 后端初始化成功");
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
