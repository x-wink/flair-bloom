use crate::log_dir;

#[tauri::command]
pub fn log_from_frontend(level: String, message: String) {
    match level.as_str() {
        "error" => tracing::error!(target: "frontend", "{}", message),
        "warn" => tracing::warn!(target: "frontend", "{}", message),
        "info" => tracing::info!(target: "frontend", "{}", message),
        _ => tracing::debug!(target: "frontend", "{}", message),
    }
}

#[tauri::command]
pub fn open_log_dir() -> Result<(), String> {
    let dir = log_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("无法创建日志目录: {e}"))?;
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&dir)
            .spawn()
            .map_err(|e| format!("无法打开日志文件夹: {}", e))?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&dir)
            .spawn()
            .map_err(|e| format!("无法打开日志文件夹: {e}"))?;
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(&dir)
            .spawn()
            .map_err(|e| format!("无法打开日志文件夹: {}", e))?;
    }
    Ok(())
}
