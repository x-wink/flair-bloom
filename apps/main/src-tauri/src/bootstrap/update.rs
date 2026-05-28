//! 更新锁 + 静默更新 + 启动期待安装包检测。
//!
//! `UpdateLock` 由 lib.rs `.manage()` 注入，`commands/app.rs` 的 `check_update`
//! 命令通过 `State<UpdateLock>` 获取它。

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Manager;
use tracing::{info, warn};

/// 保证同一时刻只有一个更新任务在运行。
pub struct UpdateLock(pub AtomicBool);

impl UpdateLock {
    pub fn acquire(&self) -> Option<UpdateLockGuard<'_>> {
        if self
            .0
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            Some(UpdateLockGuard(&self.0))
        } else {
            None
        }
    }
}

pub struct UpdateLockGuard<'a>(&'a AtomicBool);

impl Drop for UpdateLockGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// 启动时的更新检查流程：先尝试应用待安装包，再后台静默检查新版本。
pub async fn check_for_updates(app: tauri::AppHandle) {
    if crate::commands::app::try_apply_pending_update(&app).await {
        return;
    }

    let lock = app.state::<UpdateLock>();
    let _guard = match lock.acquire() {
        Some(g) => g,
        None => return,
    };

    if let Err(e) = do_silent_update(&app).await {
        warn!("silent update failed: {}", e);
    }
}

async fn do_silent_update(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri::Emitter;
    use tauri_plugin_updater::UpdaterExt;

    let updater = app.updater().map_err(|e| format!("{e}"))?;
    let update = match updater.check().await {
        Ok(Some(u)) => u,
        Ok(None) => {
            info!("app is up to date");
            return Ok(());
        }
        Err(e) => return Err(format!("update check failed: {e}")),
    };

    let version = update.version.clone();
    let notes = update.body.clone();
    info!("update available: {}", version);
    let _ = app.emit("update-downloading", &version);

    let bytes = update
        .download(|_, _| {}, || {})
        .await
        .map_err(|e| format!("download failed: {e}"))?;

    let dir = app
        .path()
        .app_local_data_dir()
        .map(|d| d.join("pending_update"))
        .map_err(|e| format!("can't get data dir: {e}"))?;

    std::fs::create_dir_all(&dir).map_err(|e| format!("{e}"))?;
    std::fs::write(dir.join("installer"), &bytes).map_err(|e| format!("{e}"))?;
    std::fs::write(dir.join("version"), &version).map_err(|e| format!("{e}"))?;

    let _ = app.emit(
        "update-ready",
        serde_json::json!({ "version": version, "notes": notes }),
    );
    Ok(())
}
