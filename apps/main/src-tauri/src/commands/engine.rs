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

#[cfg(windows)]
async fn run_interception_installer(app: AppHandle, action: &'static str) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, WaitForSingleObject, INFINITE,
    };
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };

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
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let verb: Vec<u16> = "runas\0".encode_utf16().collect();
    let params: Vec<u16> = format!("{}\0", action).encode_utf16().collect();
    let working_dir: Vec<u16> = resource_path
        .parent()
        .ok_or_else(|| "无法获取安装程序所在目录".to_string())?
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let mut sei: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
        sei.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        sei.fMask = SEE_MASK_NOCLOSEPROCESS;
        sei.lpVerb = verb.as_ptr();
        sei.lpFile = path_wide.as_ptr();
        sei.lpParameters = params.as_ptr();
        sei.lpDirectory = working_dir.as_ptr();
        sei.nShow = 1; // SW_SHOWNORMAL

        let ok = unsafe { ShellExecuteExW(&mut sei) };
        if ok == 0 {
            let err = unsafe { windows_sys::Win32::Foundation::GetLastError() };
            return if err == ERROR_CANCELLED {
                Err("已取消管理员授权".to_string())
            } else {
                Err(format!("启动安装程序失败 (Win32 错误码 {})", err))
            };
        }

        if sei.hProcess.is_null() {
            return Err("无法获取安装程序进程句柄".to_string());
        }

        let wait = unsafe { WaitForSingleObject(sei.hProcess, INFINITE) };
        if wait != WAIT_OBJECT_0 {
            unsafe { CloseHandle(sei.hProcess) };
            return Err("等待安装程序结束时出错".to_string());
        }

        let mut exit_code: u32 = 0;
        let got = unsafe { GetExitCodeProcess(sei.hProcess, &mut exit_code) };
        unsafe { CloseHandle(sei.hProcess) };

        if got == 0 {
            return Err("无法读取安装程序退出码".to_string());
        }

        if exit_code == 0 {
            Ok(())
        } else {
            Err(format!("安装程序返回错误码 {}", exit_code))
        }
    })
    .await
    .map_err(|e| format!("安装任务异常: {}", e))?
}

#[tauri::command]
pub async fn install_driver(app: AppHandle) -> Result<(), String> {
    #[cfg(windows)]
    {
        run_interception_installer(app, "/install").await
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err("仅 Windows 平台支持安装驱动".to_string())
    }
}

#[tauri::command]
pub async fn uninstall_driver(app: AppHandle) -> Result<(), String> {
    #[cfg(windows)]
    {
        // 卸载前先切回 SendInput，避免句柄被 Drop 的时机问题
        use crate::engine::input::{init_backend, InputMode};
        use tauri_plugin_store::StoreExt;
        init_backend(InputMode::SendInput);
        if let Ok(store) = app.store(crate::STORE_PATH) {
            store.set("input_mode", serde_json::json!("sendinput"));
            let _ = store.save();
        }

        run_interception_installer(app, "/uninstall").await
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err("仅 Windows 平台支持卸载驱动".to_string())
    }
}
