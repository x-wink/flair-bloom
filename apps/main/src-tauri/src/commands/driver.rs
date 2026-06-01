//! 驱动管理：Interception / DD-HID 安装卸载 + 提权重启。
//!
//! 业务逻辑委托给 `win-driver` crate，本层仅负责 Tauri 桥接与状态广播。

use serde::Serialize;
#[allow(unused_imports)]
use tauri::{AppHandle, Manager, State};

use crate::commands::engine::EngineState;

/// DD-HID 安装结果。`pending_reboot=true` 表示 Windows PnP 报告驱动文件已更新
/// 并建议重启以确保完全生效（`ERROR_SUCCESS_REBOOT_REQUIRED`, 0xBC3）；
/// 实测驱动在重启前通常已可正常工作。
#[derive(Debug, Clone, Serialize)]
pub struct DdHidInstallOutcome {
    pub pending_reboot: bool,
}

/// 卸载结果。`pending_reboot=true` 表示驱动文件已标记为重启删除、卸载在逻辑上
/// 已完成，但物理文件要等下次开机才消失。
#[derive(Debug, Clone, Serialize)]
pub struct UninstallOutcome {
    pub message: String,
    pub pending_reboot: bool,
}

#[cfg(windows)]
const DD_HID_DISABLE_SCRIPT: &str = "disable-ddhid-driver.cmd";

#[cfg(windows)]
fn resource_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let raw = app
        .path()
        .resource_dir()
        .map_err(|e| format!("无法获取资源目录: {e}"))?
        .join("resources");
    Ok(win_driver::path_util::strip_verbatim(raw))
}

#[tauri::command]
pub fn is_driver_installed() -> bool {
    #[cfg(windows)]
    {
        win_input::interception::is_driver_installed()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[tauri::command]
pub async fn install_driver(app: AppHandle, state: State<'_, EngineState>) -> Result<(), String> {
    let engine = state.0.clone();

    #[cfg(windows)]
    {
        engine.pause_runtime();
        let res_dir = resource_dir(&app)?;
        let health = crate::commands::resource_integrity::check_resources(&res_dir);
        if !health.issues.is_empty() {
            let details = health
                .issues
                .iter()
                .map(crate::commands::resource_integrity::issue_label)
                .collect::<Vec<_>>()
                .join("；");
            return Err(format!(
                "驱动资源文件校验失败，拒绝安装以防提权执行被篡改文件：{details}"
            ));
        }
        let result = win_driver::interception::install(&res_dir).await;
        if let Err(ref e) = result {
            tracing::error!("Interception 驱动安装失败：{e}");
        }
        crate::commands::status::emit_status_changed(&app);
        result
    }
    #[cfg(not(windows))]
    {
        let _ = (app, state, engine);
        Err("仅 Windows 平台支持安装驱动".to_string())
    }
}

#[tauri::command]
pub async fn uninstall_driver(app: AppHandle, state: State<'_, EngineState>) -> Result<(), String> {
    let engine = state.0.clone();

    #[cfg(windows)]
    {
        use tauri_plugin_store::StoreExt;
        use win_input::{init_backend, InputMode};

        engine.pause_runtime();
        init_backend(InputMode::SendInput);
        if let Ok(store) = app.store(crate::STORE_PATH) {
            store.set("input_mode", serde_json::json!("sendinput"));
            let _ = store.save();
        }
        let res_dir = resource_dir(&app)?;
        let result = win_driver::interception::uninstall(&res_dir).await;
        if let Err(ref e) = result {
            tracing::error!("Interception 驱动卸载失败：{e}");
        }
        crate::commands::status::emit_status_changed(&app);
        result
    }
    #[cfg(not(windows))]
    {
        let _ = (app, state, engine);
        Err("仅 Windows 平台支持卸载驱动".to_string())
    }
}

#[tauri::command]
pub fn is_dd_hid_driver_installed() -> bool {
    win_driver::dd_hid::dd_hid_sys_installed()
}

#[tauri::command]
pub async fn install_dd_hid_driver(
    app: AppHandle,
    state: State<'_, EngineState>,
) -> Result<DdHidInstallOutcome, String> {
    let engine = state.0.clone();

    #[cfg(windows)]
    {
        engine.pause_runtime();
        let res_dir = resource_dir(&app)?;
        let health = crate::commands::resource_integrity::check_resources(&res_dir);
        if !health.issues.is_empty() {
            let details = health
                .issues
                .iter()
                .map(crate::commands::resource_integrity::issue_label)
                .collect::<Vec<_>>()
                .join("；");
            return Err(format!(
                "驱动资源文件校验失败，拒绝安装以防提权执行被篡改文件：{details}"
            ));
        }
        let install_result = win_driver::dd_hid::install(&res_dir).await;
        // pending_reboot=true 表示 ddc.exe 返回 0xBC3，驱动已装但 PnP 建议重启
        let pending_reboot = matches!(install_result, Ok(true));
        let exe_result: Result<(), String> = install_result.map(|_| ());
        let sys_installed = win_driver::dd_hid::dd_hid_sys_installed();
        let service_present = win_sysinfo::registry::service_key_present("ddhid63340");
        let judge = win_driver::judge::judge_install_result(
            sys_installed,
            service_present,
            exe_result.clone(),
        );
        if let Err(ref e) = judge {
            let exe_state = match &exe_result {
                Ok(()) => "ddc.exe 报告成功".to_string(),
                Err(msg) => format!("ddc.exe 失败: {msg}"),
            };
            tracing::error!(
                "DD-HID 驱动安装失败：{e}（{exe_state}，sys 落盘={sys_installed}，服务键={service_present}）"
            );
            return Err(e.clone());
        }
        crate::commands::status::emit_status_changed(&app);
        Ok(DdHidInstallOutcome { pending_reboot })
    }
    #[cfg(not(windows))]
    {
        let _ = (app, state, engine);
        Err("仅 Windows 平台支持安装 DD-HID 驱动".to_string())
    }
}

#[tauri::command]
pub async fn uninstall_dd_hid_driver(
    app: AppHandle,
    state: State<'_, EngineState>,
) -> Result<UninstallOutcome, String> {
    let engine = state.0.clone();

    #[cfg(windows)]
    {
        use tauri_plugin_store::StoreExt;
        use win_input::{init_backend, InputMode};

        engine.pause_runtime();
        init_backend(InputMode::SendInput);
        if let Ok(store) = app.store(crate::STORE_PATH) {
            store.set("input_mode", serde_json::json!("sendinput"));
            let _ = store.save();
        }
        let res_dir = resource_dir(&app)?;
        let (pending_reboot, exe_result) = win_driver::dd_hid::uninstall(&res_dir).await?;
        crate::commands::status::emit_status_changed(&app);

        if pending_reboot {
            return Ok(UninstallOutcome {
                message: "驱动卸载已发起，剩余清理需重启电脑由 PnP 完成。\n\
                    请重启电脑后再尝试安装驱动。"
                    .to_string(),
                pending_reboot: true,
            });
        }
        let sys_still_present = win_driver::dd_hid::dd_hid_sys_installed();
        match win_driver::judge::judge_uninstall_result(sys_still_present, exe_result.clone()) {
            Ok(()) => Ok(UninstallOutcome {
                message: "究极HID 驱动已卸载，建议重启电脑后再尝试重新安装。".to_string(),
                pending_reboot: false,
            }),
            Err(e) => {
                let exe_state = match &exe_result {
                    Ok(()) => "ddc.exe 报告成功".to_string(),
                    Err(msg) => format!("ddc.exe 失败: {msg}"),
                };
                tracing::error!(
                    "DD-HID 驱动卸载失败：{e}（{exe_state}，sys 仍存在={sys_still_present}）"
                );
                Err(e)
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (app, state, engine);
        Err("仅 Windows 平台支持卸载 DD-HID 驱动".to_string())
    }
}

#[tauri::command]
pub async fn disable_dd_hid_driver_service(
    app: AppHandle,
    state: State<'_, EngineState>,
) -> Result<UninstallOutcome, String> {
    let engine = state.0.clone();

    #[cfg(windows)]
    {
        use tauri_plugin_store::StoreExt;
        use win_input::{init_backend, InputMode};

        engine.pause_runtime();
        init_backend(InputMode::SendInput);
        if let Ok(store) = app.store(crate::STORE_PATH) {
            store.set("input_mode", serde_json::json!("sendinput"));
            let _ = store.save();
        }

        let res_dir = resource_dir(&app)?;
        let health = crate::commands::resource_integrity::check_one_resource(
            &res_dir,
            DD_HID_DISABLE_SCRIPT,
        );
        if !health.issues.is_empty() {
            let details = health
                .issues
                .iter()
                .map(crate::commands::resource_integrity::issue_label)
                .collect::<Vec<_>>()
                .join("；");
            return Err(format!("DD-HID 禁用脚本校验失败，拒绝提权执行：{details}"));
        }

        let script = res_dir.join(DD_HID_DISABLE_SCRIPT);
        let params = format!("/c call \"{}\" --online", script.display());
        let exit = win_driver::elevation::run_elevated_exe_capture(
            std::path::PathBuf::from("C:\\Windows\\System32\\cmd.exe"),
            Some(&params),
        )
        .await?;
        crate::commands::status::emit_status_changed(&app);

        match exit {
            0 => Ok(UninstallOutcome {
                message: "已禁用 DD-HID 驱动服务并切回通用模式。请立即重启电脑使更改生效。"
                    .to_string(),
                pending_reboot: true,
            }),
            2 => Ok(UninstallOutcome {
                message: "未发现 DD-HID 驱动服务键，系统可能已经清理完成。".to_string(),
                pending_reboot: false,
            }),
            n => Err(format!("DD-HID 禁用脚本执行失败，退出码 {n}")),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (app, state, engine);
        Err("仅 Windows 平台支持禁用 DD-HID 驱动".to_string())
    }
}

#[tauri::command]
pub fn is_elevated() -> bool {
    win_driver::elevation::is_process_elevated()
}

/// 以管理员身份重启自身，携带 `--switch-mode=<id>` 参数，然后退出当前进程。
#[tauri::command]
pub async fn relaunch_as_admin(
    app: AppHandle,
    state: State<'_, EngineState>,
    mode: String,
) -> Result<(), String> {
    let engine = state.0.clone();

    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Foundation::ERROR_CANCELLED;
        use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SHELLEXECUTEINFOW};

        let _ =
            win_input::InputMode::from_str(&mode).ok_or_else(|| format!("未知输入模式: {mode}"))?;

        let exe = std::env::current_exe().map_err(|e| format!("无法定位当前可执行文件: {e}"))?;
        let path_wide: Vec<u16> = exe
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let verb: Vec<u16> = "runas\0".encode_utf16().collect();
        let params: Vec<u16> = format!("--elevated --switch-mode={mode}\0")
            .encode_utf16()
            .collect();

        let result = tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
            // SAFETY: SHELLEXECUTEINFOW 是 POD，全 0 初始化合法
            let mut sei: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
            sei.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
            sei.lpVerb = verb.as_ptr();
            sei.lpFile = path_wide.as_ptr();
            sei.lpParameters = params.as_ptr();
            sei.nShow = 1;
            // SAFETY: 所有指针字段的 Vec 在闭包内存活，NUL 结尾宽串
            let ok = unsafe { ShellExecuteExW(&mut sei) };
            if ok == 0 {
                // SAFETY: GetLastError 无参
                let err = unsafe { windows_sys::Win32::Foundation::GetLastError() };
                return if err == ERROR_CANCELLED {
                    Err("已取消管理员授权".to_string())
                } else {
                    Err(format!("启动管理员实例失败 (Win32 错误码 {err})"))
                };
            }
            Ok(())
        })
        .await
        .map_err(|e| format!("任务异常: {e}"))?;

        result.as_ref().map_err(|e| e.clone())?;

        engine.shutdown();
        win_input::shutdown_backend();

        let app_clone = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            app_clone.exit(0);
        });
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (app, state, engine, mode);
        Err("仅 Windows 平台支持提权重启".to_string())
    }
}
