//! 协议同意 / 检查更新 / 退出。

use tauri::{AppHandle, Manager, State};
use tauri_plugin_store::StoreExt;
use tracing::{info, warn};

use crate::bootstrap::{
    agreement::AGREEMENT_VERSION,
    update::{build_updater, check_and_download, CheckTrigger, UpdateLock},
};
use crate::commands::engine::EngineState;

pub(crate) const PENDING_UPDATE_DIR: &str = "pending_update";

#[tauri::command]
pub fn needs_agreement(app: AppHandle) -> Result<bool, String> {
    let store = app
        .store(crate::STORE_PATH)
        .map_err(|e| format!("无法读取存储: {e}"))?;
    let agreed = store
        .get("agreed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let version = store
        .get("agreement_version")
        .and_then(|v| v.as_str().map(|s| s.to_string()));
    Ok(!agreed || version.as_deref() != Some(AGREEMENT_VERSION))
}

#[tauri::command]
pub fn agree_license(app: AppHandle) -> Result<(), String> {
    let store = app
        .store(crate::STORE_PATH)
        .map_err(|e| format!("无法读取存储: {e}"))?;
    store.set("agreed", serde_json::json!(true));
    store.set("agreed_at", serde_json::json!(now_secs()));
    store.set("agreement_version", serde_json::json!(AGREEMENT_VERSION));
    store.set(
        "app_version_at_agree",
        serde_json::json!(env!("CARGO_PKG_VERSION")),
    );
    store.save().map_err(|e| format!("保存协议状态失败: {e}"))?;
    Ok(())
}

/// 浮窗"放大"按钮 / 其它呼出入口：显示主面板并隐藏浮窗。
#[tauri::command]
pub fn show_main_panel(app: AppHandle) {
    crate::enter_panel_mode(&app);
}

/// 主面板"最小化到浮窗"：隐藏主面板并显示常驻浮窗。
#[tauri::command]
pub fn minimize_to_float(app: AppHandle) {
    crate::enter_float_mode(&app);
}

#[tauri::command]
pub fn exit_app(app: AppHandle, engine: State<EngineState>) {
    engine.0.shutdown();
    app.exit(0);
}

#[tauri::command]
pub fn toggle_autostart(app: AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    let launch = app.autolaunch();
    let enabled = launch.is_enabled().unwrap_or(false);
    if enabled {
        launch
            .disable()
            .map_err(|e| format!("禁用开机自启失败: {e}"))?;
    } else {
        launch
            .enable()
            .map_err(|e| format!("启用开机自启失败: {e}"))?;
    }
    Ok(launch.is_enabled().unwrap_or(!enabled))
}

/// 用户主动检查更新：一律走「检查 → 下载 → 弹公告」，与「自动更新」开关无关——
/// 开关只决定启动时要不要自动下载，用户点了就是表达了要更新的意图。
#[tauri::command]
pub async fn check_update(app: AppHandle, lock: State<'_, UpdateLock>) -> Result<(), String> {
    let _guard = lock.acquire().ok_or("更新正在进行中")?;
    check_and_download(&app, CheckTrigger::Manual).await
}

/// 立即安装已下载的更新包。安装器会接管并重启应用，正常路径下本命令不返回。
#[tauri::command]
pub async fn apply_pending_update(
    app: AppHandle,
    lock: State<'_, UpdateLock>,
) -> Result<(), String> {
    let _guard = lock.acquire().ok_or("更新正在进行中")?;
    if try_apply_pending_update(&app).await {
        Ok(())
    } else {
        Err("没有可安装的更新包，或安装未能启动".to_string())
    }
}

/// 检查待安装包并在版本匹配时立即安装（应用将自动重启）。
/// 返回 true 表示安装已触发。
pub async fn try_apply_pending_update(app: &AppHandle) -> bool {
    let dir = match app
        .path()
        .app_local_data_dir()
        .map(|d| d.join(PENDING_UPDATE_DIR))
    {
        Ok(d) => d,
        Err(e) => {
            warn!("无法获取应用数据目录: {}", e);
            return false;
        }
    };

    let installer_path = dir.join("installer");
    let version_path = dir.join("version");

    if !installer_path.exists() || !version_path.exists() {
        return false;
    }

    let saved_version = match std::fs::read_to_string(&version_path) {
        Ok(v) => v.trim().to_string(),
        Err(e) => {
            warn!("读取待安装版本失败: {}", e);
            return false;
        }
    };

    if version_ge(env!("CARGO_PKG_VERSION"), &saved_version) {
        info!(
            "待安装版本 {} 已过期（当前 {}），清理",
            saved_version,
            env!("CARGO_PKG_VERSION")
        );
        let _ = std::fs::remove_dir_all(&dir);
        return false;
    }

    let saved_bytes = match std::fs::read(&installer_path) {
        Ok(b) => b,
        Err(e) => {
            warn!("读取安装包文件失败: {}", e);
            return false;
        }
    };

    let updater = match build_updater(app) {
        Ok(u) => u,
        Err(e) => {
            warn!("更新模块不可用: {}", e);
            return false;
        }
    };

    let update = match updater.check().await {
        Ok(Some(u)) if u.version == saved_version => u,
        Ok(Some(u)) => {
            info!(
                "服务器版本 {} 与已下载版本 {} 不匹配，清理旧安装包",
                u.version, saved_version
            );
            let _ = std::fs::remove_dir_all(&dir);
            return false;
        }
        Ok(None) => {
            info!("服务器端无可用更新，清理待安装包 {}", saved_version);
            let _ = std::fs::remove_dir_all(&dir);
            return false;
        }
        Err(e) => {
            warn!("检查更新失败，待安装包保留下次重试: {}", e);
            return false;
        }
    };

    // 安装包已经在本地，install 不走网络，不需要改写下载地址
    match update.install(saved_bytes) {
        Ok(_) => {
            info!("更新安装完成，应用即将重启");
            let _ = std::fs::remove_dir_all(&dir);
            true
        }
        Err(e) => {
            warn!("安装更新失败: {}", e);
            false
        }
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("系统时钟早于 UNIX epoch")
        .as_secs()
}

fn version_ge(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> { s.split('.').filter_map(|p| p.parse().ok()).collect() };
    parse(a) >= parse(b)
}
