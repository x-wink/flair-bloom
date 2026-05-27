use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_store::StoreExt;
use tauri_plugin_updater::UpdaterExt;
use tracing::{info, warn};

const STORE_PATH: &str = "settings.json";
const AGREEMENT_VERSION: &str = "1.2";
const PENDING_UPDATE_DIR: &str = "pending_update";

pub struct UpdateLock(pub AtomicBool);

impl UpdateLock {
    pub fn acquire(&self) -> Option<UpdateGuard<'_>> {
        self.0
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| UpdateGuard(&self.0))
    }
}

pub struct UpdateGuard<'a>(&'a AtomicBool);

impl Drop for UpdateGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

#[tauri::command]
pub fn needs_agreement(app: AppHandle) -> Result<bool, String> {
    let store = app
        .store(STORE_PATH)
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
        .store(STORE_PATH)
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
    let updater = app.updater().map_err(|e| format!("更新模块不可用: {e}"))?;
    let update = match updater.check().await {
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

/// 将下载好的安装包保存到磁盘，供下次启动时自动安装。
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

/// 检查磁盘上是否存在待安装包，若存在且版本匹配则立即安装（应用将自动重启）。
/// 返回 true 表示安装已触发，调用方应终止后续逻辑。
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

    // 当前版本已经 >= 保存的版本，说明更新已生效或文件残留，清理
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

    // 需要重新获取 Update 对象才能调用 install（用于平台安装逻辑）
    let updater = match app.updater() {
        Ok(u) => u,
        Err(e) => {
            warn!("更新模块不可用: {}", e);
            return false;
        }
    };

    let update = match updater.check().await {
        Ok(Some(u)) if u.version == saved_version => u,
        Ok(Some(u)) => {
            // 服务器版本已更新，旧包过期
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
            // 网络不通，保留文件下次重试
            warn!("检查更新失败，待安装包保留下次重试: {}", e);
            return false;
        }
    };

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
    // 与 commands/profile.rs::now_secs 同理：时钟早于 UNIX epoch 是 invariant 违反，
    // 静默返回 0 会污染 agreed_at 等合规留痕字段，宁可显式 panic 留现场。
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("系统时钟早于 UNIX epoch")
        .as_secs()
}

fn version_ge(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> { s.split('.').filter_map(|p| p.parse().ok()).collect() };
    parse(a) >= parse(b)
}
