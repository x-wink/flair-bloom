//! 会话级稳定状态汇总。
//!
//! UI 启动时拉一次 [`get_app_status`]，之后只通过 `app-status-changed` 事件刷新。
//! 高频/事件驱动的状态（规则、激活规则 ID、global_enabled）仍走独立命令，
//! 避免逼前端轮询整个状态。

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

pub const STATUS_CHANGED_EVENT: &str = "app-status-changed";

#[derive(Debug, Clone, Serialize)]
pub struct AppStatus {
    pub elevated: bool,
    pub interception_installed: bool,
    pub dd_hid_installed: bool,
    pub input_mode: String,
    pub platform: &'static str,
    pub os_family: &'static str,
    pub os_version: String,
    pub webview_version: String,
    pub arch: String,
    pub locale: String,
    pub install_path: String,
    pub log_dir: String,
    pub app_data_dir: String,
    pub autostart_enabled: bool,
    pub resources_ok: bool,
    pub missing_resources: Vec<String>,
}

impl AppStatus {
    pub fn collect(app: &AppHandle) -> Self {
        use crate::commands::sysinfo;
        let (resources_ok, missing_resources) = collect_resource_health(app);
        Self {
            elevated: collect_elevated(),
            interception_installed: collect_interception_installed(),
            dd_hid_installed: collect_dd_hid_installed(),
            input_mode: collect_input_mode(),
            platform: std::env::consts::OS,
            os_family: std::env::consts::FAMILY,
            os_version: sysinfo::os_version(),
            webview_version: sysinfo::webview2_version(),
            arch: sysinfo::host_arch(),
            locale: sysinfo::user_locale(),
            install_path: sysinfo::install_path(),
            log_dir: crate::log_dir().to_string_lossy().into_owned(),
            app_data_dir: app
                .path()
                .app_data_dir()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            autostart_enabled: collect_autostart_enabled(app),
            resources_ok,
            missing_resources,
        }
    }
}

#[tauri::command]
pub fn get_app_status(app: AppHandle) -> AppStatus {
    AppStatus::collect(&app)
}

/// 由命令侧在状态变更后调用：重新采集并向所有窗口广播 `app-status-changed`。
// 非 Windows 编译路径下没有任何调用方（驱动相关命令全部在 cfg(windows) 内），
// 但函数体本身跨平台，留它在这里方便后续接入非 Windows 的状态变化点。
#[cfg_attr(not(windows), allow(dead_code))]
pub fn emit_status_changed(app: &AppHandle) {
    let status = AppStatus::collect(app);
    if let Err(e) = app.emit(STATUS_CHANGED_EVENT, &status) {
        tracing::warn!("emit {} 失败: {}", STATUS_CHANGED_EVENT, e);
    }
}

#[cfg(windows)]
fn collect_elevated() -> bool {
    crate::commands::engine::is_process_elevated()
}

#[cfg(not(windows))]
fn collect_elevated() -> bool {
    false
}

#[cfg(windows)]
fn collect_interception_installed() -> bool {
    crate::engine::interception::is_driver_installed()
}

#[cfg(not(windows))]
fn collect_interception_installed() -> bool {
    false
}

#[cfg(windows)]
fn collect_dd_hid_installed() -> bool {
    crate::commands::engine::dd_hid_sys_installed()
}

#[cfg(not(windows))]
fn collect_dd_hid_installed() -> bool {
    false
}

#[cfg(windows)]
fn collect_input_mode() -> String {
    crate::engine::input::current_mode().as_str().to_string()
}

#[cfg(not(windows))]
fn collect_input_mode() -> String {
    "sendinput".to_string()
}

fn collect_autostart_enabled(app: &AppHandle) -> bool {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().unwrap_or(false)
}

/// 资源完整性自检：检查驱动安装器是否齐全。
///
/// Windows 安装包会把这些 exe 落到 `<resource_dir>/resources/`，杀软误删或解压不全
/// 时会让"安装游戏模式驱动 / 究极HID"按下去就报错。把缺失项列出来给反馈链路用，
/// 比让用户自己翻日志强得多。其它平台没有这些资源，恒返回 OK。
fn collect_resource_health(app: &AppHandle) -> (bool, Vec<String>) {
    #[cfg(windows)]
    {
        let resources = match app.path().resource_dir() {
            Ok(d) => d.join("resources"),
            Err(_) => return (false, vec!["<resource_dir 不可达>".to_string()]),
        };
        let mut missing = Vec::new();
        let expected = [
            ("install-interception.exe", "install-interception.exe"),
            ("ddhid-driver/ddc.exe", "ddhid-driver/ddc.exe"),
        ];
        for (rel, label) in expected {
            if !resources.join(rel).exists() {
                missing.push(label.to_string());
            }
        }
        (missing.is_empty(), missing)
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        (true, Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_name_is_stable() {
        // 前端 listen 的事件名硬编码在 PanelApp.tsx，名字一旦改动两端必须同步。
        // 这里钉死字符串，避免后续 typo 把广播打到一个没人监听的频道上。
        assert_eq!(STATUS_CHANGED_EVENT, "app-status-changed");
    }

    fn sample_status() -> AppStatus {
        AppStatus {
            elevated: true,
            interception_installed: true,
            dd_hid_installed: false,
            input_mode: "dd_hid".to_string(),
            platform: "windows",
            os_family: "windows",
            os_version: "Windows 11 23H2 (Build 22631.4317)".to_string(),
            webview_version: "120.0.0.0".to_string(),
            arch: "x64".to_string(),
            locale: "zh-CN".to_string(),
            install_path: r"C:\Program Files\FlairBloom".to_string(),
            log_dir: r"C:\Users\me\AppData\Local\fun.xwink.flairbloom\logs".to_string(),
            app_data_dir: r"C:\Users\me\AppData\Roaming\fun.xwink.flairbloom".to_string(),
            autostart_enabled: false,
            resources_ok: true,
            missing_resources: Vec::new(),
        }
    }

    #[test]
    fn serialize_uses_snake_case_for_frontend_consumption() {
        // 前端 AppStatus 字段是 snake_case，这里钉住序列化的实际 key
        let json = serde_json::to_value(sample_status()).unwrap();
        let obj = json.as_object().unwrap();
        for key in [
            "elevated",
            "interception_installed",
            "dd_hid_installed",
            "input_mode",
            "platform",
            "os_family",
            "os_version",
            "webview_version",
            "arch",
            "locale",
            "install_path",
            "log_dir",
            "app_data_dir",
            "autostart_enabled",
            "resources_ok",
            "missing_resources",
        ] {
            assert!(obj.contains_key(key), "缺少键 {key}");
        }
        assert_eq!(obj["input_mode"], "dd_hid");
        assert_eq!(obj["arch"], "x64");
        assert_eq!(obj["locale"], "zh-CN");
        assert!(obj["missing_resources"].is_array());
    }
}
