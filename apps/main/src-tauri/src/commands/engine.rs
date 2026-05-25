use crate::engine::BurstEngine;
use qzh_format::profile::{BurstRule, MAX_RULES};
use std::sync::{atomic::Ordering, Arc};
#[allow(unused_imports)]
use tauri::{AppHandle, Manager, State};

pub struct EngineState(pub Arc<BurstEngine>);

#[tauri::command]
pub fn set_global_enabled(app: AppHandle, state: State<EngineState>, enabled: bool) {
    state.0.global_enabled.store(enabled, Ordering::SeqCst);
    if let Some(tray) = app.tray_by_id("main") {
        if let Ok(menu) = crate::tray::build_menu(&app, enabled) {
            let _ = tray.set_menu(Some(menu));
        }
    }
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
    state.0.set_rules(rules);
    Ok(())
}

#[tauri::command]
pub fn get_rules(state: State<EngineState>) -> Vec<BurstRule> {
    state.0.get_rules()
}

#[tauri::command]
pub fn get_input_mode() -> String {
    #[cfg(windows)]
    {
        let mode = crate::engine::input::current_mode();
        match mode {
            crate::engine::input::InputMode::SendInput => "sendinput".to_string(),
            crate::engine::input::InputMode::Interception => "interception".to_string(),
        }
    }
    #[cfg(not(windows))]
    {
        "sendinput".to_string()
    }
}

#[tauri::command]
pub fn set_input_mode(app: AppHandle, mode: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        use crate::engine::input::{init_backend, InputMode};
        use tauri_plugin_store::StoreExt;

        let input_mode = match mode.as_str() {
            "interception" => InputMode::Interception,
            "sendinput" => InputMode::SendInput,
            _ => return Err(format!("未知输入模式: {}", mode)),
        };
        init_backend(input_mode);

        if let Ok(store) = app.store(crate::STORE_PATH) {
            store.set("input_mode", serde_json::json!(mode));
            let _ = store.save();
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (app, mode);
        Err("仅 Windows 平台支持切换输入模式".to_string())
    }
}

#[tauri::command]
pub fn is_driver_installed() -> bool {
    #[cfg(windows)]
    {
        crate::engine::interception::is_driver_installed()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[tauri::command]
pub async fn install_driver(app: AppHandle) -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;

        let resource_path = app
            .path()
            .resource_dir()
            .map_err(|e| format!("无法获取资源目录: {}", e))?
            .join("resources")
            .join("install-interception.exe");

        if !resource_path.exists() {
            return Err("安装程序不存在，请重新安装应用".to_string());
        }

        let path_wide: Vec<u16> = resource_path
            .to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let verb: Vec<u16> = "runas\0".encode_utf16().collect();
        let params: Vec<u16> = "/install\0".encode_utf16().collect();

        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                path_wide.as_ptr(),
                params.as_ptr(),
                std::ptr::null(),
                1, // SW_SHOWNORMAL
            )
        };

        // ShellExecuteW 返回值 > 32 表示成功
        if result as isize > 32 {
            Ok(())
        } else {
            Err("驱动安装启动失败，请确认已授予管理员权限".to_string())
        }
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err("仅 Windows 平台支持安装驱动".to_string())
    }
}
