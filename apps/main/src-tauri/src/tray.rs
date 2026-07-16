use crate::commands::engine::EngineState;
use crate::commands::profile::{
    activate_profile_file, list_profiles_brief, ACTIVE_PATH_KEY, DEFAULT_PROFILE_NAME,
};
use crate::engine::BurstEngine;
use std::path::{Path, PathBuf};
use std::sync::{atomic::Ordering, Arc};
use tauri::{
    menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, Wry,
};
use tauri_plugin_store::StoreExt;
use tracing::warn;

/// 「切换配置」菜单项 id 前缀，后接配置文件绝对路径。
const PROFILE_MENU_PREFIX: &str = "profile:";

fn active_profile_path(app: &AppHandle) -> Option<String> {
    app.store(crate::STORE_PATH).ok().and_then(|s| {
        s.get(ACTIVE_PATH_KEY)
            .and_then(|v| v.as_str().map(String::from))
    })
}

pub fn build_menu(app: &AppHandle, enabled: bool) -> tauri::Result<Menu<Wry>> {
    let toggle_label = if enabled {
        "✓ 连发已启用"
    } else {
        "连发已禁用"
    };
    let toggle = MenuItem::with_id(app, "toggle", toggle_label, true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let open = MenuItem::with_id(app, "open", "打开面板", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    let profiles = list_profiles_brief(app);
    if profiles.is_empty() {
        return Menu::with_items(app, &[&toggle, &sep, &open, &quit]);
    }

    let active = active_profile_path(app);
    let items: Vec<CheckMenuItem<Wry>> = profiles
        .iter()
        .map(|(name, path)| {
            let label = if name == DEFAULT_PROFILE_NAME {
                "默认配置"
            } else {
                name.as_str()
            };
            let checked = active.as_deref().is_some_and(|a| Path::new(a) == path);
            CheckMenuItem::with_id(
                app,
                format!("{PROFILE_MENU_PREFIX}{}", path.to_string_lossy()),
                label,
                true,
                checked,
                None::<&str>,
            )
        })
        .collect::<Result<_, _>>()?;
    let item_refs: Vec<&dyn IsMenuItem<Wry>> =
        items.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect();
    let profiles_menu = Submenu::with_items(app, "切换配置", true, &item_refs)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    Menu::with_items(app, &[&toggle, &sep, &profiles_menu, &sep2, &open, &quit])
}

/// 重建托盘菜单（全局开关文案 + 配置清单与勾选态）。托盘尚未创建时（启动早期）no-op。
/// 配置清单或激活配置变化的命令（load / rename / delete / fork / import 等）都应调用。
pub fn refresh_menu(app: &AppHandle) {
    let Some(tray) = app.tray_by_id("main") else {
        return;
    };
    let Some(state) = app.try_state::<EngineState>() else {
        return;
    };
    let enabled = state.0.global_enabled.load(Ordering::SeqCst);
    if let Ok(menu) = build_menu(app, enabled) {
        let _ = tray.set_menu(Some(menu));
    }
}

pub fn setup_tray(app: &AppHandle, engine: Arc<BurstEngine>) -> tauri::Result<()> {
    let menu = build_menu(app, engine.global_enabled.load(Ordering::SeqCst))?;
    let engine_clone = engine.clone();

    TrayIconBuilder::with_id("main")
        .icon(
            app.default_window_icon()
                .expect("默认窗口图标必须在 tauri.conf.json 的 bundle.icon 中配置")
                .clone(),
        )
        .menu(&menu)
        .on_menu_event(move |app, event| {
            let id = event.id.as_ref();
            if let Some(path) = id.strip_prefix(PROFILE_MENU_PREFIX) {
                let path = PathBuf::from(path);
                match activate_profile_file(app, &engine_clone, &path) {
                    // 通知面板重新加载当前配置（面板复用 load_profile 流程刷新 UI）
                    Ok(_) => {
                        let _ =
                            app.emit("active-profile-changed", path.to_string_lossy().to_string());
                    }
                    Err(e) => warn!("托盘切换配置失败：{e}"),
                }
                // 无论成败都重建菜单：成功时更新勾选，失败时回滚 OS 自动打上的勾
                refresh_menu(app);
                return;
            }
            match id {
                "toggle" => {
                    let enabled = !engine_clone.global_enabled.load(Ordering::SeqCst);
                    engine_clone.set_global_enabled(enabled, true);
                    refresh_menu(app);
                    let _ = app.emit("global-enabled-changed", enabled);
                }
                "open" => {
                    crate::enter_panel_mode(app);
                }
                "quit" => {
                    engine_clone.shutdown();
                    app.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::DoubleClick { .. } = event {
                let app = tray.app_handle();
                crate::enter_panel_mode(app);
            }
        })
        .build(app)?;

    Ok(())
}
