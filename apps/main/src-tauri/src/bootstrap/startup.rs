//! 启动期按设置打开全局开关。

use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_store::StoreExt;
use tracing::{info, warn};

use crate::engine::BurstEngine;

/// settings.json 中「启动后自动开全局」开关的键名。缺省视为关闭。
pub const AUTO_ENABLE_ON_START_KEY: &str = "autoEnableOnStart";

/// 读取「启动后自动开全局」开关。读不到 store 或未设置时视为关闭——自己动的键盘
/// 必须是用户明确要过的。
pub fn auto_enable_on_start(app: &AppHandle) -> bool {
    app.store(crate::STORE_PATH)
        .ok()
        .and_then(|store| {
            store
                .get(AUTO_ENABLE_ON_START_KEY)
                .and_then(|v| v.as_bool())
        })
        .unwrap_or(false)
}

/// 按设置在启动时打开全局开关。
///
/// 开机自启时一律跳过：登录即在后台连发，用户人可能都不在电脑前，键盘却已经自己动了。
/// 两个开关在设置界面里互斥，这里是 settings.json 被手工改坏时的兜底。
pub fn apply_auto_enable_on_start(app: &AppHandle, engine: &BurstEngine) {
    if !auto_enable_on_start(app) {
        return;
    }
    if app.autolaunch().is_enabled().unwrap_or(false) {
        warn!("开机自启已启用，跳过「启动后自动开全局」");
        return;
    }
    info!("按设置在启动时打开全局开关");
    engine.set_global_enabled(true, true);
}
