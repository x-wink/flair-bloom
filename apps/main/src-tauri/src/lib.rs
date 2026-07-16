//! Tauri 桥接 + 业务装配入口。
//!
//! 各子系统的初始化逻辑集中在 `bootstrap/` 模块，本文件只做 Builder 装配与 invoke_handler 注册。

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tracing::info;

mod bootstrap;
mod commands;
mod engine;
mod tray;

use bootstrap::{
    agreement::{check_agreement, AGREEMENT_VERSION},
    input::{init_input_backend, wait_for_predecessor_exit},
    logging,
    profile::load_or_init_profile,
    update::{check_for_updates, UpdateLock},
};
use commands::{
    app::{
        agree_license, check_update, exit_app, minimize_to_float, needs_agreement, show_main_panel,
        toggle_autostart,
    },
    ddhid_diagnostic::export_dd_hid_diagnostic_report,
    driver::{
        install_dd_hid_driver, install_driver, is_dd_hid_driver_installed, is_driver_installed,
        is_elevated, relaunch_as_admin, uninstall_dd_hid_driver, uninstall_driver,
    },
    engine::{
        get_active_rules, get_global_enabled, get_input_mode, get_rules, relay_key_event,
        set_global_enabled, set_global_hotkeys, set_input_mode, set_rules, EngineState,
    },
    import_profile::{import_external_config, preview_import, scan_import_configs},
    log::{log_from_frontend, open_app_dir},
    profile::{
        delete_profile, export_profile, fork_active_profile, get_active_profile_path,
        import_qzh_profile, init_default_profile, list_profiles, load_profile, rename_profile,
        save_profile,
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

pub(crate) const PANEL_LABEL: &str = "panel";
pub(crate) const FLOAT_LABEL: &str = "float";

pub(crate) fn show_panel<R: Runtime>(app: &AppHandle<R>) {
    if let Some(panel) = app.get_webview_window(PANEL_LABEL) {
        let _ = panel.show();
        let _ = panel.unminimize();
        let _ = panel.set_focus();
    }
}

/// 进入面板模式：显示主面板、隐藏浮窗。所有"呼出主面板"入口都应走这里。
pub(crate) fn enter_panel_mode<R: Runtime>(app: &AppHandle<R>) {
    show_panel(app);
    if let Some(float) = app.get_webview_window(FLOAT_LABEL) {
        let _ = float.hide();
        // 通知浮窗停止轮询，避免隐藏后仍空转
        let _ = float.emit("float-active", false);
    }
}

/// 进入浮窗模式：隐藏主面板、显示常驻浮窗。所有"隐藏主面板"入口都应走这里。
pub(crate) fn enter_float_mode<R: Runtime>(app: &AppHandle<R>) {
    // 先确保浮窗能显示再隐藏面板；浮窗缺失时保留面板，避免应用整体不可见。
    if let Some(float) = app.get_webview_window(FLOAT_LABEL) {
        let _ = float.show();
        let _ = float.set_focus();
        // 通知浮窗开始轮询激活规则
        let _ = float.emit("float-active", true);
        if let Some(panel) = app.get_webview_window(PANEL_LABEL) {
            let _ = panel.hide();
        }
    } else if let Some(panel) = app.get_webview_window(PANEL_LABEL) {
        let _ = panel.show();
    }
}

pub fn log_dir() -> std::path::PathBuf {
    logging::log_dir()
}

pub fn run() {
    // 提权重启时，新实例先等旧实例退出释放单实例锁，再继续装配（含单实例插件）。
    wait_for_predecessor_exit();

    let dir = logging::log_dir();
    std::fs::create_dir_all(&dir).ok();
    logging::init(&dir);
    logging::cleanup_old_logs(&dir);

    let burst_engine = Arc::new(BurstEngine::new());
    let engine_for_listener = burst_engine.clone();
    let engine_for_tray = burst_engine.clone();
    let engine_for_exit = burst_engine.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            enter_panel_mode(app);
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_dialog::init())
        .manage(EngineState(burst_engine.clone()))
        .manage(UpdateLock(AtomicBool::new(false)))
        .invoke_handler(tauri::generate_handler![
            set_global_enabled,
            get_global_enabled,
            set_global_hotkeys,
            set_rules,
            get_rules,
            get_active_rules,
            relay_key_event,
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
            export_profile,
            import_qzh_profile,
            needs_agreement,
            agree_license,
            check_update,
            exit_app,
            show_main_panel,
            minimize_to_float,
            toggle_autostart,
            log_from_frontend,
            open_app_dir,
            get_app_status,
            diagnose_environment,
            repair_dd_hid_residue,
            repair_interception_residue,
            repair_corrupted_profiles,
            repair_clean_logs,
            export_dd_hid_diagnostic_report,
            scan_import_configs,
            preview_import,
            import_external_config,
        ])
        .setup(move |app| {
            // 全局热键回调：同步托盘菜单与前端开关状态
            // 注意：这两个回调均由 WH_KEYBOARD_LL hook 线程同步调用。
            // 在 hook 返回前，WebView2（若聚焦）无法向主线程 STA 投递 COM 消息，
            // 直接在 hook 线程执行 emit/tray 操作会造成死锁并导致 hook 超时被移除。
            // 解决方案：将 UI 操作派发到 Tauri 异步运行时，hook 立即返回。
            {
                let handle = app.handle().clone();
                burst_engine.set_on_global_changed(move |enabled| {
                    let handle = handle.clone();
                    tauri::async_runtime::spawn(async move {
                        crate::tray::refresh_menu(&handle);
                        let _ = handle.emit("global-enabled-changed", enabled);
                    });
                });
            }

            // 面板显隐热键回调
            {
                let handle = app.handle().clone();
                burst_engine.set_on_panel_toggle(move || {
                    let handle = handle.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Some(win) = handle.get_webview_window(PANEL_LABEL) {
                            let visible = win.is_visible().unwrap_or(false);
                            let minimized = win.is_minimized().unwrap_or(false);
                            if visible && !minimized {
                                enter_float_mode(&handle);
                            } else {
                                enter_panel_mode(&handle);
                            }
                        }
                    });
                });
            }

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
        .build(tauri::generate_context!())
        .expect("error while building FlairBloom")
        .run(move |_app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                engine_for_exit.shutdown();
            }
        });
}
