//! DD Simple 后端，使用 `dd63330.dll`，独立于 DD-HID 驱动安装/卸载链路。
//!
//! 注入统一走 `DdFfi`（DLL 调用，内部 `DeviceIoControl` 到 `\\.\dd63330`）。
//!
//! **自注入回灌过滤（经反汇编 dd63330 内核驱动确认）**：驱动的键盘注入函数
//! （VA 0x140003d13 `mov dword [staging+8], 0`）与鼠标注入函数（VA 0x140004072
//! `mov dword [staging+0x14], 0`）都把 staging 结构的 `ExtraInformation` **显式写死为 0**，
//! 从不读取来源缓冲区的对应字段。因此注入事件回到 LL hook 时 `dwExtraInfo` 恒为 0，
//! 无法像 SendInput/Interception 那样用 `SIM_MARKER` 精确过滤。本后端的自注入只能靠
//! `PENDING_INJECTIONS` 时间窗口队列兜底（见 `lib.rs` 各 DdSimple* 路由，与 DD-HID 一致）。
//!
//! 历史教训：曾有一条「直接 DeviceIoControl 写入 SIM_MARKER」的 `dd_direct` 旁路，企图绕过
//! DLL 实现精确过滤——但驱动层同样清零 ExtraInformation，该旁路无任何收益且会诱使调用方
//! 关闭 PENDING_INJECTIONS 登记，导致 `trigger==target` 规则把自身注入误判为物理按键而自停。
//! 已整体移除，不要重新引入。
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
        // DLL 加载失败 = 驱动未安装，整体不可用
        let ffi = DdFfi::load(&dll, DdSideButtonMode::SimpleMouseInputDataFlags)?;
        info!("DD Simple 后端初始化成功（自注入由 PENDING_INJECTIONS 过滤）");
        Some(Self { ffi })
    }

    pub fn send_key(&self, vk: u32, is_up: bool) -> bool {
        self.ffi.send_key(vk, is_up)
    }

    pub fn send_mouse(&self, button: MouseButton, is_up: bool) -> bool {
        self.ffi.send_mouse(button, is_up)
    }

    pub fn send_wheel(&self, up: bool) -> bool {
        self.ffi.send_wheel(up)
    }
}
