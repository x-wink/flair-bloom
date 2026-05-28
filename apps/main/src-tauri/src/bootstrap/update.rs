//! 更新锁 + 静默更新 + 启动期待安装包检测。
//!
//! `UpdateLock` 由 lib.rs `.manage()` 注入，`commands/app.rs` 的 `check_update`
//! 命令通过 `State<UpdateLock>` 获取它。

use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;
use tracing::{info, warn};

pub const DEFAULT_GITHUB_PROXY: &str = "https://gh-proxy.com/";
pub const UPDATE_DOWNLOAD_PROGRESS_EVENT: &str = "update-download-progress";
pub const UPDATE_DOWNLOAD_FAILED_EVENT: &str = "update-download-failed";

const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(200);

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
    let updater = build_updater(app)?;
    let mut update = match updater.check().await {
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
    proxy_github_download_url(app, &mut update);
    let _ = app.emit("update-downloading", &version);

    let bytes = download_update(app, &update, &version)
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

pub fn build_updater(app: &tauri::AppHandle) -> Result<tauri_plugin_updater::Updater, String> {
    let Some(proxy_prefix) = github_proxy_prefix(app) else {
        return app.updater_builder().build().map_err(|e| format!("{e}"));
    };
    let endpoints = configured_update_endpoints(app)?
        .into_iter()
        .map(|url| proxy_github_url(&url, &proxy_prefix))
        .collect();

    app.updater_builder()
        .endpoints(endpoints)
        .map_err(|e| format!("{e}"))?
        .build()
        .map_err(|e| format!("{e}"))
}

pub async fn download_update(
    app: &tauri::AppHandle,
    update: &tauri_plugin_updater::Update,
    version: &str,
) -> Result<Vec<u8>, tauri_plugin_updater::Error> {
    let handle = app.clone();
    let mut downloaded = 0u64;
    let mut last_total = None;
    let mut last_percent = None;
    let mut last_emit = Instant::now() - PROGRESS_EMIT_INTERVAL;

    emit_update_download_progress(app, version, 0, None, false);

    let result = update
        .download(
            |chunk, total| {
                downloaded = downloaded.saturating_add(chunk as u64);
                last_total = total;
                let percent =
                    total.and_then(|total| downloaded.saturating_mul(100).checked_div(total));
                let percent_changed = match (percent, last_percent) {
                    (Some(current), Some(last)) => current > last,
                    (Some(_), None) => true,
                    _ => false,
                };
                let now = Instant::now();
                if percent_changed || now.duration_since(last_emit) >= PROGRESS_EMIT_INTERVAL {
                    emit_update_download_progress(&handle, version, downloaded, total, false);
                    last_emit = now;
                    if percent.is_some() {
                        last_percent = percent;
                    }
                }
            },
            || {
                info!("update downloaded");
            },
        )
        .await;

    match result {
        Ok(bytes) => {
            let final_size = bytes.len() as u64;
            emit_update_download_progress(
                app,
                version,
                final_size,
                last_total.or(Some(final_size)),
                true,
            );
            Ok(bytes)
        }
        Err(e) => {
            let _ = app.emit(
                UPDATE_DOWNLOAD_FAILED_EVENT,
                serde_json::json!({ "version": version, "message": e.to_string() }),
            );
            Err(e)
        }
    }
}

pub fn proxy_github_download_url(
    app: &tauri::AppHandle,
    update: &mut tauri_plugin_updater::Update,
) {
    let Some(proxy_prefix) = github_proxy_prefix(app) else {
        return;
    };

    let proxied = proxy_github_url(&update.download_url, &proxy_prefix);
    if proxied != update.download_url {
        info!("update download routed through GitHub proxy: {proxy_prefix}");
        update.download_url = proxied;
    }
}

fn emit_update_download_progress(
    app: &tauri::AppHandle,
    version: &str,
    downloaded: u64,
    total: Option<u64>,
    done: bool,
) {
    let percent = total.and_then(|total| {
        if total == 0 {
            None
        } else {
            Some(((downloaded as f64 / total as f64) * 100.0).clamp(0.0, 100.0))
        }
    });
    let _ = app.emit(
        UPDATE_DOWNLOAD_PROGRESS_EVENT,
        serde_json::json!({
            "version": version,
            "downloaded": downloaded,
            "total": total,
            "percent": percent,
            "done": done,
        }),
    );
}

fn github_proxy_prefix(_app: &tauri::AppHandle) -> Option<String> {
    Some(DEFAULT_GITHUB_PROXY.to_string())
}

fn configured_update_endpoints(app: &tauri::AppHandle) -> Result<Vec<tauri::Url>, String> {
    let config = app.config();
    let updater = config
        .plugins
        .0
        .get("updater")
        .ok_or_else(|| "缺少 updater 配置".to_string())?;
    let endpoints = updater
        .get("endpoints")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "缺少 updater.endpoints 配置".to_string())?;
    let endpoints = endpoints
        .iter()
        .filter_map(|endpoint| endpoint.as_str())
        .map(parse_url)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(endpoints)
}

fn parse_url(url: &str) -> Result<tauri::Url, String> {
    tauri::Url::parse(url).map_err(|e| format!("无效的更新地址: {e}"))
}

fn normalize_proxy_prefix(prefix: &str) -> String {
    if prefix.ends_with('/') {
        prefix.to_string()
    } else {
        format!("{prefix}/")
    }
}

fn proxy_github_url(url: &tauri::Url, proxy_prefix: &str) -> tauri::Url {
    let Some(host) = url.host_str() else {
        return url.clone();
    };
    if proxy_host(proxy_prefix).is_some_and(|proxy_host| host.eq_ignore_ascii_case(&proxy_host))
        || !is_github_host(host)
    {
        return url.clone();
    }

    let proxied = format!("{}{}", normalize_proxy_prefix(proxy_prefix), url.as_str());
    match tauri::Url::parse(&proxied) {
        Ok(url) => url,
        Err(e) => {
            warn!("invalid GitHub proxy URL: {}", e);
            url.clone()
        }
    }
}

fn proxy_host(proxy_prefix: &str) -> Option<String> {
    tauri::Url::parse(proxy_prefix)
        .ok()
        .and_then(|url| url.host_str().map(|host| host.to_string()))
}

fn is_github_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("github.com")
        || host.eq_ignore_ascii_case("www.github.com")
        || host.eq_ignore_ascii_case("api.github.com")
        || host.eq_ignore_ascii_case("codeload.github.com")
        || host.ends_with(".githubusercontent.com")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxies_github_release_url() {
        let url =
            tauri::Url::parse("https://github.com/x-wink/flair-bloom/releases/download/v1/a.zip")
                .unwrap();

        assert_eq!(
            proxy_github_url(&url, "https://gh-proxy.com/").as_str(),
            "https://gh-proxy.com/https://github.com/x-wink/flair-bloom/releases/download/v1/a.zip"
        );
    }

    #[test]
    fn leaves_non_github_url_unchanged() {
        let url = tauri::Url::parse("https://example.com/latest.json").unwrap();

        assert_eq!(proxy_github_url(&url, "https://gh-proxy.com/"), url);
    }

    #[test]
    fn does_not_proxy_proxy_url_again() {
        let url = tauri::Url::parse(
            "https://gh-proxy.com/https://github.com/x-wink/flair-bloom/releases/latest/download/latest.json",
        )
        .unwrap();

        assert_eq!(proxy_github_url(&url, "https://gh-proxy.com/"), url);
    }
}
