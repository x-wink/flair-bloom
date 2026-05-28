//! Tauri 桥接 + 业务装配入口。
//!
//! 各子系统的初始化逻辑集中在 `bootstrap/` 模块，本文件只做 Builder 装配与 invoke_handler 注册。

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tracing::info;

mod bootstrap;
mod commands;
mod engine;
mod tray;

use bootstrap::{
    agreement::{check_agreement, AGREEMENT_VERSION},
    input::init_input_backend,
    logging,
    profile::load_or_init_profile,
    update::{check_for_updates, UpdateLock},
};
use commands::{
    app::{agree_license, check_update, exit_app, needs_agreement},
    driver::{
        install_dd_hid_driver, install_driver, is_dd_hid_driver_installed, is_driver_installed,
        is_elevated, relaunch_as_admin, uninstall_dd_hid_driver, uninstall_driver,
    },
    engine::{
        get_active_rules, get_global_enabled, get_input_mode, get_rules, set_global_enabled,
        set_input_mode, set_rules, EngineState,
    },
    log::{log_from_frontend, open_app_dir},
    profile::{
        delete_profile, fork_active_profile, get_active_profile_path, init_default_profile,
        list_profiles, load_profile, rename_profile, save_profile,
    },
    repair::{
        diagnose_environment, repair_clean_logs, repair_corrupted_profiles, repair_dd_hid_residue,
        repair_interception_residue,
    },
    status::get_app_status,
};
use engine::{start_listener, BurstEngine};

pub const APP_NAME: &str = "FlairBloom";
pub const APP_NAME_CN: &str = "气质花按键助手";
pub const APP_IDENTIFIER: &str = "fun.xwink.flairbloom";
pub(crate) const STORE_PATH: &str = "settings.json";

pub fn log_dir() -> std::path::PathBuf {
    logging::log_dir()
}

pub fn run() {
    let dir = logging::log_dir();
    std::fs::create_dir_all(&dir).ok();
    logging::init(&dir);
    logging::cleanup_old_logs(&dir);

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
        .manage(EngineState(burst_engine.clone()))
        .manage(UpdateLock(AtomicBool::new(false)))
        .invoke_handler(tauri::generate_handler![
            set_global_enabled,
            get_global_enabled,
            set_rules,
            get_rules,
            get_active_rules,
            get_input_mode,
            set_input_mode,
            is_driver_installed,
            install_driver,
            uninstall_driver,
            is_dd_hid_driver_installed,
            install_dd_hid_driver,
            uninstall_dd_hid_driver,
            is_elevated,
            relaunch_as_admin,
            save_profile,
            load_profile,
            list_profiles,
            init_default_profile,
            get_active_profile_path,
            rename_profile,
            delete_profile,
            fork_active_profile,
            needs_agreement,
            agree_license,
            check_update,
            exit_app,
            log_from_frontend,
            open_app_dir,
            get_app_status,
            diagnose_environment,
            repair_dd_hid_residue,
            repair_interception_residue,
            repair_corrupted_profiles,
            repair_clean_logs,
        ])
        .setup(move |app| {
            let need_agreement = check_agreement(app.handle());
            load_or_init_profile(app.handle(), &burst_engine);
            init_input_backend(app.handle());
            tray::setup_tray(app.handle(), engine_for_tray)?;

            if let Some(panel) = app.get_webview_window("panel") {
                panel.show()?;
                if need_agreement {
                    let _ = panel.emit("show-agreement", AGREEMENT_VERSION);
                }
            }

            start_listener(engine_for_listener);

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                check_for_updates(handle).await;
            });

            info!("{} started", APP_NAME);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running FlairBloom");
}
