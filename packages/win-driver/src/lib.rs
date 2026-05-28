//! Windows 驱动安装卸载 + ShellExecuteEx 提权 + PowerShell 编码。
//!
//! 本 crate 不依赖 Tauri AppHandle，调用方负责从 Tauri 路径解析器获取资源目录后传入。

pub mod dd_hid;
pub mod elevation;
pub mod interception;
pub mod judge;
pub mod path_util;
pub mod powershell;
