//! 协议同意 / 检查更新 / 退出。

use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_store::StoreExt;
use tracing::{info, warn};

use crate::bootstrap::{
    agreement::AGREEMENT_VERSION,
    update::{build_updater, proxy_github_download_url, UpdateLock},
};

const PENDING_UPDATE_DIR: &str = "pending_update";

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

#[tauri::command]
pub fn exit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub async fn check_update(app: AppHandle, lock: State<'_, UpdateLock>) -> Result<(), String> {
    let _guard = lock.acquire().ok_or("更新正在进行中")?;
    do_check_update(&app).await
}

async fn do_check_update(app: &AppHandle) -> Result<(), String> {
    let updater = build_updater(app).map_err(|e| format!("更新模块不可用: {e}"))?;
    let mut update = match updater.check().await {
        Ok(Some(u)) => u,
        Ok(None) => {
            let _ = app.emit("update-not-available", ());
            return Ok(());
        }
        Err(e) => {
            warn!("update check failed: {}", e);
            return Err(format!("检查更新失败: {e}"));
        }
    };

    let version = update.version.clone();
    let notes = update.body.clone();
    info!("update available: {}", version);
    proxy_github_download_url(app, &mut update);
    let _ = app.emit("update-downloading", &version);

    let bytes = update
        .download(
            |_chunk, _total| {},
            || {
                info!("update downloaded");
            },
        )
        .await
        .map_err(|e| {
            warn!("update download failed: {}", e);
            format!("下载更新失败: {e}")
        })?;

    save_pending_update(app, &version, &bytes)?;
    let _ = app.emit(
        "update-ready",
        serde_json::json!({ "version": version, "notes": notes }),
    );
    Ok(())
}

fn save_pending_update(app: &AppHandle, version: &str, bytes: &[u8]) -> Result<(), String> {
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("无法获取应用数据目录: {e}"))?
        .join(PENDING_UPDATE_DIR);
    std::fs::create_dir_all(&dir).map_err(|e| format!("无法创建更新目录: {e}"))?;
    std::fs::write(dir.join("installer"), bytes).map_err(|e| format!("保存安装包失败: {e}"))?;
    std::fs::write(dir.join("version"), version).map_err(|e| format!("保存版本信息失败: {e}"))?;
    Ok(())
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

    let mut update = match updater.check().await {
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
    proxy_github_download_url(app, &mut update);

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
