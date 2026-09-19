//! 启动期按设置对齐两个开关：自动开全局、以管理员模式启动。

use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_store::StoreExt;
use tracing::{info, warn};

use crate::engine::BurstEngine;

/// settings.json 中「启动后自动开全局」开关的键名。缺省视为关闭。
pub const AUTO_ENABLE_ON_START_KEY: &str = "autoEnableOnStart";
/// settings.json 中「以管理员模式启动」的用户意图。真正生效的是注册表里的兼容性标志，
/// 这里只记意图，用来在它被抹掉后自愈。
pub const RUN_AS_ADMIN_KEY: &str = "runAsAdmin";

fn bool_setting(app: &AppHandle, key: &str) -> Option<bool> {
    app.store(crate::STORE_PATH)
        .ok()
        .and_then(|store| store.get(key).and_then(|v| v.as_bool()))
}

/// 读取「启动后自动开全局」开关。读不到 store 或未设置时视为关闭——自己动的键盘
/// 必须是用户明确要过的。
pub fn auto_enable_on_start(app: &AppHandle) -> bool {
    bool_setting(app, AUTO_ENABLE_ON_START_KEY).unwrap_or(false)
}

/// 按设置在启动时打开全局开关。
///
/// 两道闸：协议没同意不开——协议弹窗还挡在前面，引擎却已经在注入按键，那道门就形同虚设；
/// 开机自启时不开——登录即在后台连发，用户人可能都不在电脑前。后者在设置界面里是互斥的，
/// 这里是 settings.json 被手工改坏时的兜底。
pub fn apply_auto_enable_on_start(app: &AppHandle, engine: &BurstEngine, need_agreement: bool) {
    if !auto_enable_on_start(app) {
        return;
    }
    if need_agreement {
        info!("用户协议待同意，暂不自动打开全局开关");
        return;
    }
    if app.autolaunch().is_enabled().unwrap_or(false) {
        warn!("开机自启已启用，跳过「启动后自动开全局」");
        return;
    }
    info!("按设置在启动时打开全局开关");
    engine.set_global_enabled(true, true);
}

/// 把注册表里的管理员启动标志对齐到 settings.json 记下的意图。
///
/// 为什么需要：标志的值名是 exe 绝对路径，而 NSIS 更新会先跑一遍旧版卸载器、连带删掉它
/// （卸载钩子分不清「更新」和「真卸载」）；换安装目录同理。没有这一步，用户开了开关，
/// 更新一次就悄悄变回普通权限，而这个开关存在的意义正是免掉每次提权重启。
/// store 里没有记过意图就不碰注册表——那可能是用户自己在「属性」里设的。
pub fn apply_run_as_admin(app: &AppHandle) {
    let Some(want) = bool_setting(app, RUN_AS_ADMIN_KEY) else {
        return;
    };
    let exe = match crate::commands::app::current_exe_path() {
        Ok(exe) => exe,
        Err(e) => {
            warn!("无法对齐管理员启动标志: {e}");
            return;
        }
    };
    if win_sysinfo::run_as_admin::is_enabled(&exe) == want {
        return;
    }
    match win_sysinfo::run_as_admin::set_enabled(&exe, want) {
        Ok(()) => info!(
            "已按设置重建管理员启动标志：{}",
            if want { "开启" } else { "关闭" }
        ),
        Err(e) => warn!("重建管理员启动标志失败: {e}"),
    }
}
