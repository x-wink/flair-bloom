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

    // DD-HID 模式仅 Toggle 模式要求 target_key 与 trigger_key/stop_key 不同；
    // Hold 模式靠 input.rs 的注入事件队列识别 sim 事件，允许 trigger == target
    #[cfg(windows)]
    {
        let mode = crate::engine::input::current_mode();
        if mode.requires_distinct_target_for_toggle() {
            for rule in rules
                .iter()
                .filter(|r| r.enabled && matches!(r.mode, qzh_format::profile::BurstMode::Toggle))
            {
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
pub fn get_input_mode() -> String {
    #[cfg(windows)]
    {
        crate::engine::input::current_mode().as_str().to_string()
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
        use crate::engine::input::{init_backend, InputMode};
        use tauri_plugin_store::StoreExt;

        let input_mode =
            InputMode::from_str(&mode).ok_or_else(|| format!("未知输入模式: {}", mode))?;

        // 切到 DD-HID 时要求当前已是管理员，否则前端应先发起提权重启
        if input_mode.requires_admin() && !is_process_elevated() {
            return Err("究极HID 模式需要管理员权限，请先以管理员身份重启应用".to_string());
        }

        // 切到 DD-HID 前用新规则约束做静态校验：仅 Toggle 模式要求 target 与 trigger/stop 互异
        if input_mode.requires_distinct_target_for_toggle() {
            let rules = state.0.get_rules();
            for rule in rules
                .iter()
                .filter(|r| r.enabled && matches!(r.mode, qzh_format::profile::BurstMode::Toggle))
            {
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
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (app, state, mode);
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
async fn run_elevated_exe(
    app: AppHandle,
    file_path: std::path::PathBuf,
    params: Option<&str>,
) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, WaitForSingleObject, INFINITE,
    };
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };

    let _ = app;
    if !file_path.exists() {
        return Err(format!("可执行文件不存在: {}", file_path.display()));
    }

    let path_wide: Vec<u16> = file_path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let verb: Vec<u16> = "runas\0".encode_utf16().collect();
    let params_wide: Vec<u16> = match params {
        Some(p) => format!("{}\0", p).encode_utf16().collect(),
        None => vec![0u16],
    };
    let working_dir: Vec<u16> = file_path
        .parent()
        .ok_or_else(|| "无法获取所在目录".to_string())?
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        // SAFETY: SHELLEXECUTEINFOW 是 POD,全 0 初始化合法,后续逐字段填写
        let mut sei: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
        sei.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        sei.fMask = SEE_MASK_NOCLOSEPROCESS;
        sei.lpVerb = verb.as_ptr();
        sei.lpFile = path_wide.as_ptr();
        sei.lpParameters = params_wide.as_ptr();
        sei.lpDirectory = working_dir.as_ptr();
        sei.nShow = 1;

        // SAFETY: 所有指针字段所指向的 Vec 在闭包结束前都存活,且都是 NUL 结尾的宽串
        let ok = unsafe { ShellExecuteExW(&mut sei) };
        if ok == 0 {
            // SAFETY: GetLastError 无参,任意线程任意时刻调用安全
            let err = unsafe { windows_sys::Win32::Foundation::GetLastError() };
            return if err == ERROR_CANCELLED {
                Err("已取消管理员授权".to_string())
            } else {
                Err(format!("启动程序失败 (Win32 错误码 {})", err))
            };
        }

        if sei.hProcess.is_null() {
            return Err("无法获取进程句柄".to_string());
        }

        // SAFETY: hProcess 是上面 ShellExecuteExW 在 SEE_MASK_NOCLOSEPROCESS
        // 模式下返回的有效进程句柄,INFINITE 是合法等待时长
        let wait = unsafe { WaitForSingleObject(sei.hProcess, INFINITE) };
        if wait != WAIT_OBJECT_0 {
            // SAFETY: hProcess 仍是上面返回的有效句柄
            unsafe { CloseHandle(sei.hProcess) };
            return Err("等待程序结束时出错".to_string());
        }

        let mut exit_code: u32 = 0;
        // SAFETY: hProcess 仍有效;exit_code 是栈上 u32,&mut 在调用期间有效
        let got = unsafe { GetExitCodeProcess(sei.hProcess, &mut exit_code) };
        // SAFETY: hProcess 是上面返回的有效句柄,函数返回前 hProcess 不再被读
        unsafe { CloseHandle(sei.hProcess) };

        if got == 0 {
            return Err("无法读取退出码".to_string());
        }

        if exit_code == 0 {
            Ok(())
        } else {
            Err(format!("程序返回错误码 {}", exit_code))
        }
    })
    .await
    .map_err(|e| format!("任务异常: {}", e))?
}

#[cfg(windows)]
fn resource_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let raw = app
        .path()
        .resource_dir()
        .map_err(|e| format!("无法获取资源目录: {}", e))?
        .join("resources");
    // Tauri 的 resource_dir 在 Windows 上返回 \\?\<drive>:\... 形式的 verbatim 路径。
    // 直接用作 ShellExecuteEx 的 lpDirectory 时，被启动进程（如 ddc.exe）再 spawn cmd
    // 会因「UNC 路径不受支持」回退到 C:\Windows\，导致 INF/SYS 找不到、驱动安装失败。
    Ok(strip_verbatim(raw))
}

/// 去掉 Windows verbatim 路径前缀（`\\?\<drive>:\...` → `<drive>:\...`），
/// 保留真正的 UNC 路径（`\\?\UNC\server\share\...` → `\\server\share\...`）。
#[cfg(windows)]
fn strip_verbatim(path: std::path::PathBuf) -> std::path::PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        return std::path::PathBuf::from(format!(r"\\{}", rest));
    }
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        // 仅当剥离后形如 "<letter>:\..." 时认为是普通本地路径
        let bytes = rest.as_bytes();
        if bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes[2] == b'\\' || bytes[2] == b'/')
        {
            return std::path::PathBuf::from(rest);
        }
    }
    path
}

#[tauri::command]
pub async fn install_driver(app: AppHandle) -> Result<(), String> {
    #[cfg(windows)]
    {
        let exe = resource_dir(&app)?.join("install-interception.exe");
        run_elevated_exe(app, exe, Some("/install")).await
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
        use crate::engine::input::{init_backend, InputMode};
        use tauri_plugin_store::StoreExt;
        init_backend(InputMode::SendInput);
        if let Ok(store) = app.store(crate::STORE_PATH) {
            store.set("input_mode", serde_json::json!("sendinput"));
            let _ = store.save();
        }

        let exe = resource_dir(&app)?.join("install-interception.exe");
        run_elevated_exe(app, exe, Some("/uninstall")).await
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err("仅 Windows 平台支持卸载驱动".to_string())
    }
}

// ===== DD-HID 驱动管理 =====

/// HID 驱动是否已安装：检测 system32\drivers\ddhid63340.sys 是否存在
#[tauri::command]
pub fn is_dd_hid_driver_installed() -> bool {
    #[cfg(windows)]
    {
        let sysroot = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
        let drv_path = std::path::Path::new(&sysroot)
            .join("System32")
            .join("drivers")
            .join("ddhid63340.sys");
        drv_path.exists()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[tauri::command]
pub async fn install_dd_hid_driver(app: AppHandle) -> Result<(), String> {
    #[cfg(windows)]
    {
        let exe = resource_dir(&app)?.join("ddhid-driver").join("ddc.exe");
        run_elevated_exe(app, exe, None).await
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err("仅 Windows 平台支持安装 DD-HID 驱动".to_string())
    }
}

#[tauri::command]
pub async fn uninstall_dd_hid_driver(app: AppHandle) -> Result<(), String> {
    #[cfg(windows)]
    {
        use crate::engine::input::{init_backend, InputMode};
        use tauri_plugin_store::StoreExt;
        // 卸载前先切回 SendInput，释放 DLL
        init_backend(InputMode::SendInput);
        if let Ok(store) = app.store(crate::STORE_PATH) {
            store.set("input_mode", serde_json::json!("sendinput"));
            let _ = store.save();
        }

        let exe = resource_dir(&app)?.join("ddhid-driver").join("ddc.exe");
        run_elevated_exe(app, exe, Some("-u")).await
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err("仅 Windows 平台支持卸载 DD-HID 驱动".to_string())
    }
}

// ===== 提权重启 =====

#[cfg(windows)]
fn is_process_elevated() -> bool {
    use std::mem;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    const TOKEN_QUERY: u32 = 0x0008;
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: GetCurrentProcess 返回伪句柄无需释放;OpenProcessToken 写入 token 出参
    let ok = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) };
    if ok == 0 {
        return false;
    }
    // SAFETY: TOKEN_ELEVATION 是 POD,全 0 初始化合法
    let mut elev: TOKEN_ELEVATION = unsafe { mem::zeroed() };
    let mut ret_len: u32 = 0;
    // SAFETY: token 来自上面 OpenProcessToken 成功调用;elev 是栈上 POD,
    // 大小由 size_of 提供;ret_len 是栈上 u32 出参
    let got = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            &mut elev as *mut _ as *mut _,
            mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret_len,
        )
    };
    // SAFETY: token 是上面 OpenProcessToken 返回的有效句柄
    unsafe { CloseHandle(token) };
    got != 0 && elev.TokenIsElevated != 0
}

#[tauri::command]
pub fn is_elevated() -> bool {
    #[cfg(windows)]
    {
        is_process_elevated()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// 以管理员身份重启自身。当前进程会通过 ShellExecuteEx 启动新实例（带 runas verb），
/// 然后 `app.exit(0)` 触发本进程退出。新进程读取 `--switch-mode=<id>` 自动设定模式。
#[tauri::command]
pub async fn relaunch_as_admin(app: AppHandle, mode: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Foundation::ERROR_CANCELLED;
        use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SHELLEXECUTEINFOW};

        // 校验目标模式合法
        let _ = crate::engine::input::InputMode::from_str(&mode)
            .ok_or_else(|| format!("未知输入模式: {}", mode))?;

        let exe = std::env::current_exe().map_err(|e| format!("无法定位当前可执行文件: {}", e))?;
        let path_wide: Vec<u16> = exe
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let verb: Vec<u16> = "runas\0".encode_utf16().collect();
        let params: Vec<u16> = format!("--elevated --switch-mode={}\0", mode)
            .encode_utf16()
            .collect();

        let result = tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
            // SAFETY: SHELLEXECUTEINFOW 是 POD,全 0 初始化合法,后续逐字段填写
            let mut sei: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
            sei.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
            sei.lpVerb = verb.as_ptr();
            sei.lpFile = path_wide.as_ptr();
            sei.lpParameters = params.as_ptr();
            sei.nShow = 1;

            // SAFETY: verb / path_wide / params 都是 NUL 结尾的宽串,Vec 在闭包内存活
            let ok = unsafe { ShellExecuteExW(&mut sei) };
            if ok == 0 {
                // SAFETY: GetLastError 无参,任意线程任意时刻调用安全
                let err = unsafe { windows_sys::Win32::Foundation::GetLastError() };
                return if err == ERROR_CANCELLED {
                    Err("已取消管理员授权".to_string())
                } else {
                    Err(format!("启动管理员实例失败 (Win32 错误码 {})", err))
                };
            }
            Ok(())
        })
        .await
        .map_err(|e| format!("任务异常: {}", e))?;

        result.as_ref().map_err(|e| e.clone())?;

        // 新实例已启动（不等其退出）。让前端有时间收到响应，再触发本进程退出
        let app_clone = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            app_clone.exit(0);
        });
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (app, mode);
        Err("仅 Windows 平台支持提权重启".to_string())
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn strip_verbatim_removes_drive_prefix() {
        let p = PathBuf::from(r"\\?\C:\Windows\System32");
        assert_eq!(strip_verbatim(p), PathBuf::from(r"C:\Windows\System32"));
    }

    #[test]
    fn strip_verbatim_handles_lowercase_drive() {
        let p = PathBuf::from(r"\\?\d:\foo\bar");
        assert_eq!(strip_verbatim(p), PathBuf::from(r"d:\foo\bar"));
    }

    #[test]
    fn strip_verbatim_converts_unc_back_to_double_slash() {
        let p = PathBuf::from(r"\\?\UNC\server\share\dir");
        assert_eq!(strip_verbatim(p), PathBuf::from(r"\\server\share\dir"));
    }

    #[test]
    fn strip_verbatim_keeps_non_verbatim_unchanged() {
        let p = PathBuf::from(r"C:\Users\me");
        assert_eq!(strip_verbatim(p.clone()), p);
    }

    #[test]
    fn strip_verbatim_keeps_unrecognized_verbatim_form() {
        // \\?\Volume{GUID}\... 类形式不是 drive,也不是 UNC,应保持原样
        let p = PathBuf::from(r"\\?\Volume{12345}\foo");
        assert_eq!(strip_verbatim(p.clone()), p);
    }

    #[test]
    fn strip_verbatim_handles_normal_unc() {
        // \\server\share 已经是 UNC,无 verbatim 前缀,保持不变
        let p = PathBuf::from(r"\\server\share\file");
        assert_eq!(strip_verbatim(p.clone()), p);
    }
}
