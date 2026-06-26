//! 规则 CRUD + 输入模式切换 + 按键捕获。驱动管理已迁至 [`super::driver`]。

use crate::engine::BurstEngine;
use qzh_profile::{BurstRule, Hotkeys, KeyId, MAX_INTERVAL_MS, MAX_RULES, MIN_INTERVAL_MS};
use serde::Serialize;
use std::sync::{atomic::Ordering, Arc};
#[allow(unused_imports)]
use tauri::{AppHandle, Emitter, Manager, State};
use win_input::try_consume_relay_injection;

pub struct EngineState(pub Arc<BurstEngine>);

#[derive(Debug, Clone, Copy, Serialize)]
pub struct RelayKeyResult {
    pub accepted_physical: bool,
    pub handled: bool,
}

#[tauri::command]
pub fn set_global_enabled(app: AppHandle, state: State<EngineState>, enabled: bool) {
    state.0.set_global_enabled(enabled, true);
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
        if !(MIN_INTERVAL_MS..=MAX_INTERVAL_MS).contains(&rule.interval_ms) {
            return Err(format!(
                "第 {} 条规则间隔 {}ms 超出范围 [{}, {}]",
                i + 1,
                rule.interval_ms,
                MIN_INTERVAL_MS,
                MAX_INTERVAL_MS
            ));
        }
    }

    #[cfg(windows)]
    check_rules_for_mode(&rules, win_input::current_mode())?;

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
/// 这里的 handled 只表示引擎是否处理了按键，不能用于事后取消 F3 等浏览器快捷键。
#[tauri::command]
pub fn relay_key_event(state: State<EngineState>, key: KeyId, is_up: bool) -> RelayKeyResult {
    if try_consume_relay_injection(key, is_up) {
        return RelayKeyResult {
            accepted_physical: false,
            handled: false,
        };
    }

    let result = if is_up {
        state.0.on_key_release_event(key)
    } else {
        state.0.on_key_press_event(key)
    };
    RelayKeyResult {
        accepted_physical: result.accepted_physical,
        handled: result.handled,
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
        use win_input::InputMode;

        let input_mode =
            InputMode::from_str(&mode).ok_or_else(|| format!("未知输入模式: {mode}"))?;

        if input_mode.requires_admin() && !win_driver::elevation::is_process_elevated() {
            return Err(format!(
                "{} 需要管理员权限，请先以管理员身份重启应用",
                input_mode_label(input_mode)
            ));
        }

        check_rules_for_mode(&state.0.get_rules(), input_mode)?;

        switch_input_backend(&app, &state.0, input_mode);

        crate::commands::status::emit_status_changed(&app);
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (app, state, mode);
        Err("仅 Windows 平台支持切换输入模式".to_string())
    }
}

/// 安全切换输入后端并持久化所选模式。切换期间通过 [`BurstEngine::begin_backend_switch`] 置
/// 「切换中」标志、停连发并经旧后端阻塞释放所有已按下的目标键，使窗口内到来的物理触发不会启动
/// 规则——杜绝目标键 down 走旧后端、up 走新后端的错配卡键。不改全局开关，用户在切换期间按下的
/// 停止热键得以保留。`set_input_mode` 与驱动卸载 / 修复共用此入口，避免各处切后端逻辑漂移。
#[cfg(windows)]
pub(crate) fn switch_input_backend(
    app: &AppHandle,
    engine: &BurstEngine,
    mode: win_input::InputMode,
) {
    use tauri_plugin_store::StoreExt;
    engine.begin_backend_switch();
    win_input::init_backend(mode);
    engine.end_backend_switch();
    if let Ok(store) = app.store(crate::STORE_PATH) {
        store.set("input_mode", serde_json::json!(mode.as_str()));
        let _ = store.save();
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

/// 校验规则集是否满足目标输入模式的约束：DD 系列要求 Toggle 目标键区别于启动/停止键；
/// DD-HID 还禁止鼠标侧键（X1/X2）作目标。`set_rules`（编辑规则保存）与 `set_input_mode`
/// （切换模式）共用同一校验，保证两入口约束一致。
#[cfg(windows)]
fn check_rules_for_mode(rules: &[BurstRule], mode: win_input::InputMode) -> Result<(), String> {
    use qzh_profile::key_id::{KeyId, MouseButton};
    use qzh_profile::profile::BurstMode;

    let label = input_mode_label(mode);
    let forbids_side_button = mode.forbids_side_button_target();
    let distinct_target = mode.requires_distinct_target_for_toggle();
    for rule in rules.iter().filter(|r| r.enabled) {
        if forbids_side_button
            && matches!(
                rule.target_key,
                KeyId::Mouse(MouseButton::X1) | KeyId::Mouse(MouseButton::X2)
            )
        {
            return Err(format!(
                "规则「{}」的目标键是鼠标侧键，{} 模式不支持。请把目标键改为左/右/中键或键盘键。",
                rule.id, label
            ));
        }
        if !distinct_target || !matches!(rule.mode, BurstMode::Toggle) {
            continue;
        }
        if rule.target_key == rule.trigger_key {
            return Err(format!(
                "{} 模式下，切换连发规则「{}」的目标键不可与启动热键相同。请修改后再使用。",
                label, rule.id
            ));
        }
        let stop = rule.stop_key.unwrap_or(rule.trigger_key);
        if rule.target_key == stop {
            return Err(format!(
                "{} 模式下，切换连发规则「{}」的目标键不可与停止热键相同。请修改后再使用。",
                label, rule.id
            ));
        }
    }
    Ok(())
}
