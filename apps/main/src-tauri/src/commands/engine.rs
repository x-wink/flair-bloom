//! 规则 CRUD + 输入模式切换 + 按键捕获。驱动管理已迁至 [`super::driver`]。

use crate::engine::BurstEngine;
use qzh_profile::{BurstRule, Hotkeys, KeyId, MAX_RULES};
use std::sync::{atomic::Ordering, Arc};
#[allow(unused_imports)]
use tauri::{AppHandle, Emitter, Manager, State};

pub struct EngineState(pub Arc<BurstEngine>);

#[tauri::command]
pub fn set_global_enabled(app: AppHandle, state: State<EngineState>, enabled: bool) {
    state.0.global_enabled.store(enabled, Ordering::SeqCst);
    if !enabled {
        state.0.cancel_all_loops();
    }
    if let Some(tray) = app.tray_by_id("main") {
        if let Ok(menu) = crate::tray::build_menu(&app, enabled) {
            let _ = tray.set_menu(Some(menu));
        }
    }
}

/// 运行时更新全局热键（不写盘，写盘由 `save_profile` 负责）。
#[tauri::command]
pub fn set_global_hotkeys(state: State<EngineState>, hotkeys: Hotkeys) {
    state.0.set_hotkeys(hotkeys);
}

#[tauri::command]
pub fn get_global_enabled(state: State<EngineState>) -> bool {
    state.0.global_enabled.load(Ordering::SeqCst)
}

#[tauri::command]
pub fn set_rules(state: State<EngineState>, rules: Vec<BurstRule>) -> Result<(), String> {
    if rules.len() > MAX_RULES {
        return Err(format!("规则数量 {} 超过上限 {}", rules.len(), MAX_RULES));
    }
    for (i, rule) in rules.iter().enumerate() {
        if !(10..=10000).contains(&rule.interval_ms) {
            return Err(format!(
                "第 {} 条规则间隔 {}ms 超出范围 [10, 10000]",
                i + 1,
                rule.interval_ms
            ));
        }
    }

    #[cfg(windows)]
    {
        let mode = win_input::current_mode();
        if mode.requires_distinct_target_for_toggle() {
            for rule in rules.iter().filter(|r| r.enabled) {
                if !matches!(rule.mode, qzh_profile::profile::BurstMode::Toggle) {
                    continue;
                }
                if rule.target_key == rule.trigger_key {
                    return Err(format!(
                        "DDHID 模式下，切换连发规则「{}」的目标键不可与启动热键相同",
                        rule.id
                    ));
                }
                let stop = rule.stop_key.unwrap_or(rule.trigger_key);
                if rule.target_key == stop {
                    return Err(format!(
                        "DDHID 模式下，切换连发规则「{}」的目标键不可与停止热键相同",
                        rule.id
                    ));
                }
            }
        }
    }

    state.0.set_rules(rules);
    Ok(())
}

#[tauri::command]
pub fn get_rules(state: State<EngineState>) -> Vec<BurstRule> {
    state.0.get_rules()
}

#[tauri::command]
pub fn get_active_rules(state: State<EngineState>) -> Vec<String> {
    state.0.get_active_ids()
}

/// 面板聚焦时 WH_KEYBOARD_LL 不触发，前端将键盘事件中继到引擎统一处理。
/// 注意：WebView 默认行为必须由前端在 DOM 事件内同步 preventDefault；
/// 这里的返回值只表示引擎是否处理了按键，不能用于事后取消 F3 等浏览器快捷键。
#[tauri::command]
pub fn relay_key_event(state: State<EngineState>, key: KeyId, is_up: bool) -> bool {
    if is_up {
        state.0.on_key_release(key);
        false
    } else {
        state.0.on_key_press(key)
    }
}

#[tauri::command]
pub fn get_input_mode() -> String {
    #[cfg(windows)]
    {
        win_input::current_mode().as_str().to_string()
    }
    #[cfg(not(windows))]
    {
        "sendinput".to_string()
    }
}

#[tauri::command]
pub fn set_input_mode(
    app: AppHandle,
    state: State<EngineState>,
    mode: String,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        use tauri_plugin_store::StoreExt;
        use win_input::{init_backend, InputMode};

        let input_mode =
            InputMode::from_str(&mode).ok_or_else(|| format!("未知输入模式: {mode}"))?;

        if input_mode.requires_admin() && !win_driver::elevation::is_process_elevated() {
            return Err(format!(
                "{} 需要管理员权限，请先以管理员身份重启应用",
                input_mode_label(input_mode)
            ));
        }

        if input_mode.requires_distinct_target_for_toggle() {
            use qzh_profile::key_id::{KeyId, MouseButton};
            let rules = state.0.get_rules();
            for rule in rules.iter().filter(|r| r.enabled) {
                if matches!(
                    rule.target_key,
                    KeyId::Mouse(MouseButton::X1) | KeyId::Mouse(MouseButton::X2)
                ) {
                    return Err(format!(
                        "切换失败：规则「{}」的目标键是鼠标侧键，DDHID 模式不支持。请把目标键改为左/右/中键或键盘键。",
                        rule.id
                    ));
                }
                if !matches!(rule.mode, qzh_profile::profile::BurstMode::Toggle) {
                    continue;
                }
                if rule.target_key == rule.trigger_key {
                    return Err(format!(
                        "切换失败：切换连发规则「{}」的目标键与启动热键相同。\nDDHID 模式下，切换连发的目标键不可与启动/停止热键相同。请修改后再切换。",
                        rule.id
                    ));
                }
                let stop = rule.stop_key.unwrap_or(rule.trigger_key);
                if rule.target_key == stop {
                    return Err(format!(
                        "切换失败：切换连发规则「{}」的目标键与停止热键相同。\nDDHID 模式下，切换连发的目标键不可与启动/停止热键相同。请修改后再切换。",
                        rule.id
                    ));
                }
            }
        }

        init_backend(input_mode);

        if let Ok(store) = app.store(crate::STORE_PATH) {
            store.set("input_mode", serde_json::json!(input_mode.as_str()));
            let _ = store.save();
        }
        crate::commands::status::emit_status_changed(&app);
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (app, state, mode);
        Err("仅 Windows 平台支持切换输入模式".to_string())
    }
}

#[cfg(windows)]
fn input_mode_label(mode: win_input::InputMode) -> &'static str {
    match mode {
        win_input::InputMode::SendInput => "通用模式",
        win_input::InputMode::Interception => "游戏模式",
        win_input::InputMode::DdSimple => "DD驱动",
        win_input::InputMode::DdHid => "DDHID",
    }
}
