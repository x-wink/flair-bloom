//! DD Simple 后端，使用 `dd63330.dll`，独立于 DD-HID 驱动安装/卸载链路。
//!
//! 注入优先级：
//! 1. `DdSimpleDirectIo`（直接 DeviceIoControl，写入 SIM_MARKER）— hook 可精确过滤
//! 2. `DdFfi`（DLL 调用，ExtraInformation 硬编码为 0）— 由 PENDING_INJECTIONS 队列兜底
//!
//! DLL 始终参与初始化（`DD_btn(0)` 自检确认驱动就绪），之后的注入尽量走直接 IO。
#![cfg(windows)]

use crate::dd_common::{DdFfi, DdSideButtonMode};
use crate::dd_direct::DdSimpleDirectIo;
use qzh_profile::key_id::MouseButton;
use std::path::Path;
use tracing::info;

pub const DLL_VERSION: &str = "63330";
pub const DLL_NAME: &str = "dd63330.dll";

pub struct DdSimpleBackend {
    ffi: DdFfi,
    direct_io: Option<DdSimpleDirectIo>,
}

impl DdSimpleBackend {
    pub fn new(resources_dir: &Path) -> Option<Self> {
        let dll = resources_dir.join(DLL_NAME);
        // DLL 加载失败 = 驱动未安装，整体不可用
        let ffi = DdFfi::load(&dll, DdSideButtonMode::SimpleMouseInputDataFlags)?;
        let direct_io = DdSimpleDirectIo::new();
        if direct_io.is_some() {
            info!("DD Simple 后端初始化成功（直接 IO 模式，ExtraInformation = SIM_MARKER）");
        } else {
            info!("DD Simple 后端初始化成功（DLL 模式，PENDING_INJECTIONS 兜底）");
        }
        Some(Self { ffi, direct_io })
    }

    /// 当前注入路径是否为直接 IO（即 SIM_MARKER 已写入，调用方无需 `record_injection`）。
    pub fn has_direct_io(&self) -> bool {
        self.direct_io.is_some()
    }

    pub fn send_key(&self, vk: u32, is_up: bool) -> bool {
        if let Some(direct) = &self.direct_io {
            // None = 该 VK 无 scan code（Pause 等极少数键）：直接返回 false。
            // 不降级到 DLL，因为调用方在 has_direct_io()==true 时已跳过 record_injection，
            // 若此时 DLL 成功注入却无 PENDING_INJECTIONS 记录，hook 端会误判为物理按键。
            return direct.send_key(vk, is_up).unwrap_or(false);
        }
        self.ffi.send_key(vk, is_up)
    }

    pub fn send_mouse(&self, button: MouseButton, is_up: bool) -> bool {
        if let Some(direct) = &self.direct_io {
            return direct.send_mouse(button, is_up);
        }
        self.ffi.send_mouse(button, is_up)
    }

    pub fn send_wheel(&self, up: bool) -> bool {
        if let Some(direct) = &self.direct_io {
            return direct.send_wheel(up);
        }
        self.ffi.send_wheel(up)
    }
}
