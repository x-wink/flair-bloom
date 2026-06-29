//! 输入后端初始化：启动时先落到 SendInput，避免非管理员进程直接加载驱动后端。

#[cfg(windows)]
use tauri::Manager;
#[cfg(windows)]
use tauri_plugin_store::StoreExt;

pub fn init_input_backend(app: &tauri::AppHandle) {
    #[cfg(windows)]
    {
        use win_input::{init_backend, set_resources_dir, InputMode};

        if let Ok(dir) = app.path().resource_dir() {
            set_resources_dir(dir.join("resources"));
        }

        let cli_mode = parse_switch_mode_arg()
            .as_deref()
            .and_then(InputMode::from_str);
        init_backend(InputMode::SendInput);

        if let Some(mode) = cli_mode {
            if let Ok(store) = app.store(crate::STORE_PATH) {
                store.set("input_mode", serde_json::json!(mode.as_str()));
                let _ = store.save();
            }
        }
    }
    #[cfg(not(windows))]
    let _ = app;
}

#[cfg(windows)]
pub fn parse_switch_mode_arg() -> Option<String> {
    for arg in std::env::args() {
        if let Some(v) = arg.strip_prefix("--switch-mode=") {
            return Some(v.to_string());
        }
    }
    None
}

/// 提权重启场景：新提权实例由旧实例用 `--await-pid=<旧PID>` 拉起。必须在单实例插件
/// 初始化之前先等旧进程退出（释放单实例锁），否则新实例会被判定为重复实例而自杀，
/// 表现为「以管理员重启只退出不重启」。带 5 秒超时兜底；旧进程已退出（OpenProcess
/// 失败）则立即返回。
pub fn wait_for_predecessor_exit() {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{
            OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
        };

        let Some(pid) = std::env::args().find_map(|a| {
            a.strip_prefix("--await-pid=")
                .and_then(|v| v.parse::<u32>().ok())
        }) else {
            return;
        };

        // SAFETY: 参数合法；句柄打开失败返回 null
        let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        if handle.is_null() {
            return; // 旧进程已退出或无法打开，直接继续启动
        }
        // SAFETY: handle 由上面 OpenProcess 成功返回，等待后立即关闭
        unsafe {
            WaitForSingleObject(handle, 5000);
            CloseHandle(handle);
        }
    }
}
