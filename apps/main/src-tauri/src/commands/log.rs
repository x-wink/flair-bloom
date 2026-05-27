use crate::log_dir;
use tauri::{AppHandle, Manager};

#[tauri::command]
pub fn log_from_frontend(level: String, message: String) {
    match level.as_str() {
        "error" => tracing::error!(target: "frontend", "{}", message),
        "warn" => tracing::warn!(target: "frontend", "{}", message),
        "info" => tracing::info!(target: "frontend", "{}", message),
        _ => tracing::debug!(target: "frontend", "{}", message),
    }
}

/// 状态弹窗里「打开」按钮调用：仅允许打开应用受信任的几类目录
/// （安装 / 数据 / 日志 / 系统驱动目录），杜绝前端把任意路径塞过来。
#[tauri::command]
pub fn open_app_dir(app: AppHandle, kind: String) -> Result<(), String> {
    let dir = match kind.as_str() {
        "install" => std::env::current_exe()
            .map_err(|e| format!("无法定位安装目录: {e}"))?
            .parent()
            .ok_or("无法获取安装目录")?
            .to_path_buf(),
        "data" => app
            .path()
            .app_data_dir()
            .map_err(|e| format!("无法获取数据目录: {e}"))?,
        "log" => log_dir(),
        "drivers" => {
            #[cfg(target_os = "windows")]
            {
                let sysroot =
                    std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
                std::path::PathBuf::from(sysroot)
                    .join("System32")
                    .join("drivers")
            }
            #[cfg(not(target_os = "windows"))]
            {
                return Err("仅 Windows 平台存在系统驱动目录".to_string());
            }
        }
        other => return Err(format!("未知目录类型: {other}")),
    };
    open_dir_in_explorer(&dir)
}

fn open_dir_in_explorer(dir: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("无法创建目录: {e}"))?;
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(dir)
            .spawn()
            .map_err(|e| format!("无法打开文件夹: {}", e))?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(dir)
            .spawn()
            .map_err(|e| format!("无法打开文件夹: {e}"))?;
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(dir)
            .spawn()
            .map_err(|e| format!("无法打开文件夹: {}", e))?;
    }
    Ok(())
}
