//! 输入后端初始化：从 settings.json 读取 input_mode，或优先使用 CLI 参数。

#[cfg(windows)]
use tauri::Manager;
use tauri_plugin_store::StoreExt;

pub fn init_input_backend(app: &tauri::AppHandle) {
    #[cfg(windows)]
    {
        use win_input::{init_backend, set_resources_dir, InputMode};

        if let Ok(dir) = app.path().resource_dir() {
            set_resources_dir(dir.join("resources"));
        }

        let cli_mode = parse_switch_mode_arg();

        let stored_mode: Option<String> = app.store(crate::STORE_PATH).ok().and_then(|store| {
            store
                .get("input_mode")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
        });

        let mode_str = cli_mode.clone().or(stored_mode);
        let mode = mode_str
            .as_deref()
            .and_then(InputMode::from_str)
            .unwrap_or_default();
        init_backend(mode);

        if let Some(m) = cli_mode {
            if let Ok(store) = app.store(crate::STORE_PATH) {
                store.set("input_mode", serde_json::json!(m));
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
