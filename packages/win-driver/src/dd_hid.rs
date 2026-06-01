//! DD-HID 驱动安装/卸载 + 残留检测。

#[cfg(windows)]
use crate::{
    elevation::{run_elevated_exe, run_elevated_exe_capture},
    powershell,
};
#[cfg(windows)]
use std::path::Path;
#[cfg(windows)]
use tracing::warn;

/// DD-HID 驱动版本号，驱动相关文件名均以此为后缀。
pub const DD_HID_VERSION: &str = "63340";
/// Windows 服务名及内核驱动名前缀（不含 `.sys`）。
pub const DD_HID_SERVICE_NAME: &str = "ddhid63340";
const DD_HID_SYS_NAME: &str = "ddhid63340.sys";

/// `ddhid63340.sys` 的绝对路径（基于 `%SystemRoot%`）。
#[cfg(windows)]
pub fn dd_hid_sys_path() -> std::path::PathBuf {
    let sysroot = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    std::path::Path::new(&sysroot)
        .join("System32")
        .join("drivers")
        .join(DD_HID_SYS_NAME)
}

/// `ddhid63340.sys` 是否已落盘。
#[cfg(windows)]
pub fn dd_hid_sys_installed() -> bool {
    dd_hid_sys_path().exists()
}

#[cfg(not(windows))]
pub fn dd_hid_sys_installed() -> bool {
    false
}

/// `ERROR_SUCCESS_REBOOT_REQUIRED`：安装成功但系统需重启后才能完全生效。
/// SetupAPI 在覆盖安装/更新已在用驱动文件时会返回此码（0xBC3 = 3011）。
#[cfg(windows)]
const REBOOT_REQUIRED: u32 = 0xBC3;

/// 安装 DD-HID 驱动（调用 `ddc.exe`）。
///
/// 返回 `Ok(true)` 表示安装成功但 Windows 要求重启（`0xBC3`），
/// `Ok(false)` 表示安装成功且无需重启，`Err` 表示安装失败。
#[cfg(windows)]
pub async fn install(resource_dir: &Path) -> Result<bool, String> {
    let exe = resource_dir.join("ddhid-driver").join("ddc.exe");
    match run_elevated_exe_capture(exe, None).await? {
        0 => Ok(false),
        REBOOT_REQUIRED => Ok(true),
        n => Err(format!("ddc.exe 返回错误码 {n}")),
    }
}

/// 卸载 DD-HID 驱动（调用 `ddc.exe -u`），失败时兜底调用 pnputil。
#[cfg(windows)]
pub async fn uninstall(resource_dir: &Path) -> Result<(bool, Result<(), String>), String> {
    let exe = resource_dir.join("ddhid-driver").join("ddc.exe");
    let exe_result = run_elevated_exe(exe, Some("-u")).await;
    let mut pending_reboot = false;
    if dd_hid_sys_installed() {
        match pnputil_uninstall().await {
            Ok(0) | Ok(1) => {}
            Ok(2) => pending_reboot = true,
            Ok(n) => warn!("pnputil 卸载返回未知退出码 {n}"),
            Err(e) => warn!("pnputil 卸载兜底失败：{}", e),
        }
        if dd_hid_sys_installed() {
            pending_reboot = true;
        }
    }
    Ok((pending_reboot, exe_result))
}

#[cfg(windows)]
async fn pnputil_uninstall() -> Result<u32, String> {
    let oem_list = find_dd_hid_oem_inf();
    if oem_list.is_empty() {
        return Ok(1);
    }
    let oem_array = powershell::ps_string_array(&oem_list);
    let script = format!(
        "$ErrorActionPreference='Continue';\n\
         $hardFail=$false;\n\
         foreach ($oem in {oem_array}) {{\n\
             try {{ & pnputil.exe /delete-driver $oem /uninstall /force | Out-Null }}\n\
             catch {{ $hardFail=$true }}\n\
             if ($LASTEXITCODE -ne 0) {{ $hardFail=$true }}\n\
         }}\n\
         if ($hardFail) {{ exit 2 }}\n\
         exit 0",
    );
    powershell::run_script_elevated(&script).await
}

/// 列出 `%SystemRoot%\INF\` 下归属 ddhid63340 的 OEM INF 编号。
pub fn find_dd_hid_oem_inf() -> Vec<String> {
    let inf_dir = std::env::var("SystemRoot")
        .map(|r| std::path::Path::new(&r).join("INF"))
        .unwrap_or_else(|_| std::path::PathBuf::from("C:\\Windows\\INF"));
    let entries = match std::fs::read_dir(&inf_dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy().to_lowercase();
        if !name_str.starts_with("oem") || !name_str.ends_with(".inf") {
            continue;
        }
        let Ok(content) = std::fs::read(entry.path()) else {
            continue;
        };
        let utf8 = String::from_utf8_lossy(&content).to_lowercase();
        let utf16 = if content.len() >= 2 && content[0] == 0xFF && content[1] == 0xFE {
            let u16s: Vec<u16> = content[2..]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16_lossy(&u16s).to_lowercase()
        } else {
            String::new()
        };
        if utf8.contains(DD_HID_SERVICE_NAME) || utf16.contains(DD_HID_SERVICE_NAME) {
            out.push(name_str);
        }
    }
    out.sort();
    out
}

/// 扫描 `%SystemRoot%\System32\DriverStore\FileRepository\` 下的 ddhid 目录。
pub fn list_dd_hid_driverstore() -> Vec<String> {
    let base = std::env::var("SystemRoot")
        .map(|r| {
            std::path::Path::new(&r)
                .join("System32")
                .join("DriverStore")
                .join("FileRepository")
        })
        .unwrap_or_else(|_| {
            std::path::PathBuf::from("C:\\Windows\\System32\\DriverStore\\FileRepository")
        });
    let entries = match std::fs::read_dir(&base) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if name.starts_with("ddhid") {
            out.push(name);
        }
    }
    out.sort();
    out
}
