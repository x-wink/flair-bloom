use crate::engine::BurstEngine;
use std::sync::{atomic::Ordering, Arc};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, Runtime,
};

pub fn build_menu<R: Runtime>(app: &AppHandle<R>, enabled: bool) -> tauri::Result<Menu<R>> {
    let toggle_label = if enabled {
        "✓ 连发已启用"
    } else {
        "连发已禁用"
    };
    let toggle = MenuItem::with_id(app, "toggle", toggle_label, true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let open = MenuItem::with_id(app, "open", "打开面板", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    Menu::with_items(app, &[&toggle, &sep, &open, &quit])
}

pub fn setup_tray<R: Runtime>(app: &AppHandle<R>, engine: Arc<BurstEngine>) -> tauri::Result<()> {
    let menu = build_menu(app, engine.global_enabled.load(Ordering::SeqCst))?;
    let engine_clone = engine.clone();

    TrayIconBuilder::with_id("main")
        .icon(
            app.default_window_icon()
                .expect("默认窗口图标必须在 tauri.conf.json 的 bundle.icon 中配置")
                .clone(),
        )
        .menu(&menu)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "toggle" => {
                let enabled = !engine_clone.global_enabled.load(Ordering::SeqCst);
                engine_clone.global_enabled.store(enabled, Ordering::SeqCst);
                if let Ok(m) = build_menu(app, enabled) {
                    if let Some(tray) = app.tray_by_id("main") {
                        let _ = tray.set_menu(Some(m));
                    }
                }
                let _ = app.emit("global-enabled-changed", enabled);
            }
            "open" => {
                if let Some(panel) = app.get_webview_window("panel") {
                    let _ = panel.show();
                    let _ = panel.set_focus();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::DoubleClick { .. } = event {
                let app = tray.app_handle();
                if let Some(panel) = app.get_webview_window("panel") {
                    let _ = panel.show();
                    let _ = panel.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}
