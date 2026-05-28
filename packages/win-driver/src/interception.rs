//! Interception 驱动安装/卸载。

#[cfg(windows)]
use crate::elevation::run_elevated_exe;
#[cfg(windows)]
use std::path::Path;

/// 安装 Interception 驱动（调用 `install-interception.exe /install`）。
#[cfg(windows)]
pub async fn install(resource_dir: &Path) -> Result<(), String> {
    let exe = resource_dir.join("install-interception.exe");
    run_elevated_exe(exe, Some("/install")).await
}

/// 卸载 Interception 驱动（调用 `install-interception.exe /uninstall`）。
#[cfg(windows)]
pub async fn uninstall(resource_dir: &Path) -> Result<(), String> {
    let exe = resource_dir.join("install-interception.exe");
    run_elevated_exe(exe, Some("/uninstall")).await
}
