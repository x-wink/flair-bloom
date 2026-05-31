//! 规则 CRUD + 输入模式切换 + 按键捕获。驱动管理已迁至 [`super::driver`]。

use crate::engine::BurstEngine;
use qzh_profile::{BurstRule, Hotkeys, KeyId, MAX_RULES};
use std::sync::{atomic::Ordering, Arc};
#[allow(unused_imports)]
use tauri::{AppHandle, Emitter, Manager, State};
#[cfg(windows)]
use tauri_plugin_store::StoreExt;

pub struct EngineState(pub Arc<BurstEngine>);

#[derive(serde::Serialize)]
pub struct EngineMetricsDto {
    pub active_rules: usize,
    pub injected_events: u64,
    pub injection_rate_per_sec: f64,
    pub scheduler_steps: u64,
    pub skipped_pulses: u64,
    pub stop_commands: u64,
    pub delay_sample_count: usize,
    pub delay_p50_us: u64,
    pub delay_p95_us: u64,
    pub delay_p99_us: u64,
    pub delay_max_us: u64,
    pub hook_sample_count: usize,
    pub hook_p50_us: u64,
    pub hook_p95_us: u64,
    pub hook_p99_us: u64,
    pub hook_max_us: u64,
    pub stop_response_sample_count: usize,
    pub stop_response_p50_us: u64,
    pub stop_response_p95_us: u64,
    pub stop_response_p99_us: u64,
    pub stop_response_max_us: u64,
}

impl From<burst_engine::EngineMetricsSnapshot> for EngineMetricsDto {
    fn from(value: burst_engine::EngineMetricsSnapshot) -> Self {
        Self {
            active_rules: value.active_rules,
            injected_events: value.injected_events,
            injection_rate_per_sec: value.injection_rate_per_sec,
            scheduler_steps: value.scheduler_steps,
            skipped_pulses: value.skipped_pulses,
            stop_commands: value.stop_commands,
            delay_sample_count: value.delay_sample_count,
            delay_p50_us: value.delay_p50_us,
            delay_p95_us: value.delay_p95_us,
            delay_p99_us: value.delay_p99_us,
            delay_max_us: value.delay_max_us,
            hook_sample_count: value.hook_sample_count,
            hook_p50_us: value.hook_p50_us,
            hook_p95_us: value.hook_p95_us,
            hook_p99_us: value.hook_p99_us,
            hook_max_us: value.hook_max_us,
            stop_response_sample_count: value.stop_response_sample_count,
            stop_response_p50_us: value.stop_response_p50_us,
            stop_response_p95_us: value.stop_response_p95_us,
            stop_response_p99_us: value.stop_response_p99_us,
            stop_response_max_us: value.stop_response_max_us,
        }
    }
}

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
        use qzh_profile::key_id::{KeyId, MouseButton};

        let mode = win_input::current_mode();
        if mode.requires_distinct_target_for_toggle() {
            for rule in rules.iter().filter(|r| r.enabled) {
                if matches!(
                    rule.target_key,
                    KeyId::Mouse(MouseButton::X1) | KeyId::Mouse(MouseButton::X2)
                ) {
                    return Err(format!(
                        "究极HID 模式不支持鼠标侧键作为目标键，请把规则「{}」的目标键换成左/右/中键或键盘键",
                        rule.id
                    ));
                }
                if !matches!(rule.mode, qzh_profile::profile::BurstMode::Toggle) {
                    continue;
                }
                if rule.target_key == rule.trigger_key {
                    return Err(format!(
                        "究极HID 模式下，切换连发规则「{}」的目标键不可与启动热键相同",
                        rule.id
                    ));
                }
                let stop = rule.stop_key.unwrap_or(rule.trigger_key);
                if rule.target_key == stop {
                    return Err(format!(
                        "究极HID 模式下，切换连发规则「{}」的目标键不可与停止热键相同",
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

#[tauri::command]
pub fn get_engine_metrics(state: State<EngineState>) -> EngineMetricsDto {
    state.0.metrics_snapshot().into()
}

/// 面板聚焦时 WH_KEYBOARD_LL 不触发，前端将键盘事件中继到引擎统一处理。
/// 注意：WebView 默认行为必须由前端在 DOM 事件内同步 preventDefault；
/// 这里的返回值只表示引擎是否处理了按键，不能用于事后取消 F3 等浏览器快捷键。
///
/// # ⚠️ 致命陷阱：注入按键必须在此处过滤
///
/// WebView2 聚焦时 WH_KEYBOARD_LL 被 Chromium hook 优先截断，所有按键事件——
/// 包括调度器通过驱动注入的**模拟**按键——都会产生 DOM keydown/keyup 并中继到此。
///
/// 后果：对于 trigger == target 的 Toggle 规则，每次注入脉冲都触发一次开关翻转，
/// 形成高频循环（10ms 间隔），用户任何停止操作均无效，只能卸载驱动。
/// 即使关闭全局开关，命令积压也会导致 StopAll 超时，驱动侧按键永久卡住。
///
/// 规则：**所有向驱动发送注入的代码路径必须调用 `record_relay_injection`**，
/// 此处的 `try_consume_relay_injection` 才能正确过滤，物理按键不受影响。
#[tauri::command]
pub fn relay_key_event(state: State<EngineState>, key: KeyId, is_up: bool) -> bool {
    #[cfg(windows)]
    if win_input::try_consume_relay_injection(key, is_up) {
        return false;
    }
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
        use win_input::{init_backend, InputMode};

        let input_mode =
            InputMode::from_str(&mode).ok_or_else(|| format!("未知输入模式: {mode}"))?;

        if input_mode.requires_admin() && !win_driver::elevation::is_process_elevated() {
            return Err("究极HID 模式需要管理员权限，请先以管理员身份重启应用".to_string());
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
                        "切换失败：规则「{}」的目标键是鼠标侧键，究极HID 模式不支持。请把目标键改为左/右/中键或键盘键。",
                        rule.id
                    ));
                }
                if !matches!(rule.mode, qzh_profile::profile::BurstMode::Toggle) {
                    continue;
                }
                if rule.target_key == rule.trigger_key {
                    return Err(format!(
                        "切换失败：切换连发规则「{}」的目标键与启动热键相同。\n究极HID 模式下，切换连发的目标键不可与启动/停止热键相同。请修改后再切换。",
                        rule.id
                    ));
                }
                let stop = rule.stop_key.unwrap_or(rule.trigger_key);
                if rule.target_key == stop {
                    return Err(format!(
                        "切换失败：切换连发规则「{}」的目标键与停止热键相同。\n究极HID 模式下，切换连发的目标键不可与启动/停止热键相同。请修改后再切换。",
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
