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
