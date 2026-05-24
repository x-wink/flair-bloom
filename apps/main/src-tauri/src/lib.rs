use std::sync::Arc;
use tauri::{Emitter, Manager};
use tracing::{info, warn};

mod commands;
mod engine;
mod tray;

use commands::engine::{get_global_enabled, get_rules, set_global_enabled, set_rules, EngineState};
use engine::{burst::start_listener, BurstEngine};

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let burst_engine = Arc::new(BurstEngine::new());
    let engine_for_listener = burst_engine.clone();
    let engine_for_tray = burst_engine.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {}))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(EngineState(burst_engine))
        .invoke_handler(tauri::generate_handler![
            set_global_enabled,
            get_global_enabled,
            set_rules,
            get_rules,
        ])
        .setup(|app| {
            tray::setup_tray(app.handle(), engine_for_tray)?;
            if let Some(panel) = app.get_webview_window("panel") {
                panel.show()?;
            }
            start_listener(engine_for_listener);

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                check_for_updates(handle).await;
            });

            info!("FlairBloom started");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running FlairBloom");
}

async fn check_for_updates(app: tauri::AppHandle) {
    use tauri_plugin_updater::UpdaterExt;
    match app.updater() {
        Ok(updater) => match updater.check().await {
            Ok(Some(update)) => {
                info!("update available: {}", update.version);
                let _ = app.emit("update-available", &update.version);
            }
            Ok(None) => info!("app is up to date"),
            Err(e) => warn!("update check failed: {}", e),
        },
        Err(e) => warn!("updater not available: {}", e),
    }
}
