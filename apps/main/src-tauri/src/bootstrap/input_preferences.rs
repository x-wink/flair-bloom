#[cfg(any(windows, test))]
const ONBOARDING_VERSION_KEY: &str = "inputModeOnboardingVersion";

#[cfg(windows)]
pub fn initialize(app: &tauri::AppHandle) {
    use tauri_plugin_autostart::ManagerExt;
    use tauri_plugin_store::StoreExt;

    let result = (|| -> Result<(), String> {
        let store = app.store(crate::STORE_PATH).map_err(|e| e.to_string())?;
        let mut settings = store.entries().into_iter().collect();
        let autostart = app.autolaunch().is_enabled().map_err(|e| e.to_string())?;
        if migrate(&mut settings, autostart) {
            for key in [
                "input_mode",
                super::startup::RUN_AS_ADMIN_KEY,
                ONBOARDING_VERSION_KEY,
            ] {
                if let Some(value) = settings.get(key) {
                    store.set(key, value.clone());
                }
            }
            store.save().map_err(|e| e.to_string())?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        tracing::warn!("初始化输入模式偏好失败: {error}");
    }
}

#[cfg(any(windows, test))]
fn migrate(settings: &mut serde_json::Map<String, serde_json::Value>, autostart: bool) -> bool {
    use serde_json::json;

    if settings
        .get(ONBOARDING_VERSION_KEY)
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
        >= 1
    {
        return false;
    }
    let mode = settings.get("input_mode").and_then(|v| v.as_str());
    let use_default_mode = matches!(mode, None | Some("sendinput"));
    if use_default_mode {
        settings.insert("input_mode".into(), json!("ddsimple"));
    }
    // 已明确选择的启动偏好与开机自启保持原样，避免 Windows 静默拦下自启。
    if use_default_mode && !autostart && !settings.contains_key(super::startup::RUN_AS_ADMIN_KEY) {
        settings.insert(super::startup::RUN_AS_ADMIN_KEY.into(), json!(true));
    }
    settings.insert(ONBOARDING_VERSION_KEY.into(), json!(1));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn new_install_uses_internet_cafe_mode_and_admin_startup() {
        let mut settings = serde_json::Map::new();
        assert!(migrate(&mut settings, false));
        assert_eq!(settings["input_mode"], "ddsimple");
        assert_eq!(settings["runAsAdmin"], true);
    }

    #[test]
    fn upgrade_migrates_lightweight_mode_and_preserves_other_settings() {
        let mut settings = json!({"input_mode": "sendinput", "runAsAdmin": false, "theme": "dark"})
            .as_object()
            .unwrap()
            .clone();
        migrate(&mut settings, false);
        assert_eq!(
            settings,
            json!({"input_mode": "ddsimple", "runAsAdmin": false,
            "theme": "dark", "inputModeOnboardingVersion": 1})
            .as_object()
            .unwrap()
            .clone()
        );
    }

    #[test]
    fn upgrade_preserves_existing_game_driver_and_removed_modes() {
        for mode in ["interception", "ddsimple", "hidstick", "dd_hid"] {
            let mut settings = json!({"input_mode": mode}).as_object().unwrap().clone();
            migrate(&mut settings, false);
            assert_eq!(settings["input_mode"], mode);
            assert!(!settings.contains_key("runAsAdmin"));
        }
    }

    #[test]
    fn migration_does_not_repeat_after_user_selects_lightweight_mode() {
        let mut settings = serde_json::Map::new();
        migrate(&mut settings, false);
        settings.insert("input_mode".into(), json!("sendinput"));
        let snapshot = settings.clone();
        assert!(!migrate(&mut settings, false));
        assert_eq!(settings, snapshot);
    }

    #[test]
    fn startup_preferences_are_preserved_for_autostart_and_explicit_opt_out() {
        for initial in [json!({}), json!({"runAsAdmin": false})] {
            let mut settings = initial.as_object().unwrap().clone();
            let autostart = settings.is_empty();
            let preference = settings.get("runAsAdmin").cloned();
            migrate(&mut settings, autostart);
            assert_eq!(settings.get("runAsAdmin"), preference.as_ref());
        }
    }

    #[test]
    fn upgrade_applies_admin_default_when_no_startup_preference_was_saved() {
        let mut settings =
            json!({"input_mode": "sendinput", "sequenceActiveProfilePath": "old.qzh"})
                .as_object()
                .unwrap()
                .clone();
        migrate(&mut settings, false);
        assert_eq!(settings["input_mode"], "ddsimple");
        assert_eq!(settings["runAsAdmin"], true);
    }
}
