use tauri::{AppHandle, Emitter};
use tauri_plugin_store::StoreExt;
use tauri_plugin_updater::UpdaterExt;
use tracing::{info, warn};

const STORE_PATH: &str = "settings.json";
const AGREEMENT_VERSION: &str = "1.0";

#[tauri::command]
pub fn needs_agreement(app: AppHandle) -> Result<bool, String> {
    let store = app
        .store(STORE_PATH)
        .map_err(|e| format!("无法读取存储: {}", e))?;
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
        .store(STORE_PATH)
        .map_err(|e| format!("无法读取存储: {}", e))?;
    store.set("agreed", serde_json::json!(true));
    store.set("agreed_at", serde_json::json!(now_secs()));
    store.set("agreement_version", serde_json::json!(AGREEMENT_VERSION));
    store.set(
        "app_version_at_agree",
        serde_json::json!(env!("CARGO_PKG_VERSION")),
    );
    store.save().map_err(|e| format!("保存协议状态失败: {}", e))?;
    Ok(())
}

#[tauri::command]
pub fn exit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<(), String> {
    let updater = app
        .updater()
        .map_err(|e| format!("更新模块不可用: {}", e))?;
    match updater.check().await {
        Ok(Some(update)) => {
            info!("update available: {}", update.version);
            let _ = app.emit("update-available", &update.version);
        }
        Ok(None) => {
            let _ = app.emit("update-not-available", ());
        }
        Err(e) => {
            warn!("update check failed: {}", e);
            return Err(format!("检查更新失败: {}", e));
        }
    }
    Ok(())
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
