use std::sync::Arc;
use tauri::{Emitter, Manager};
use tauri_plugin_store::StoreExt;
use tracing::{error, info, warn};

mod commands;
mod engine;
mod tray;

use commands::{
    app::{agree_license, check_update, exit_app, needs_agreement, try_apply_pending_update},
    engine::{get_global_enabled, get_rules, set_global_enabled, set_rules, EngineState},
    profile::{
        get_active_profile_path, init_default_profile, list_profiles, load_profile,
        save_profile,
    },
};
use engine::{burst::start_listener, BurstEngine};

pub const APP_NAME: &str = "FlairBloom";
pub const APP_NAME_CN: &str = "气质花按键助手";
const AGREEMENT_VERSION: &str = "1.0";
const STORE_PATH: &str = "settings.json";

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let burst_engine = Arc::new(BurstEngine::new());
    let engine_for_listener = burst_engine.clone();
    let engine_for_tray = burst_engine.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {}))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(EngineState(burst_engine.clone()))
        .invoke_handler(tauri::generate_handler![
            set_global_enabled,
            get_global_enabled,
            set_rules,
            get_rules,
            save_profile,
            load_profile,
            list_profiles,
            init_default_profile,
            get_active_profile_path,
            needs_agreement,
            agree_license,
            check_update,
            exit_app,
        ])
        .setup(move |app| {
            let need_agreement = check_agreement(app.handle());
            load_or_init_profile(app.handle(), &burst_engine);

            tray::setup_tray(app.handle(), engine_for_tray)?;

            if let Some(panel) = app.get_webview_window("panel") {
                panel.show()?;
                if need_agreement {
                    let _ = panel.emit("show-agreement", AGREEMENT_VERSION);
                }
            }

            start_listener(engine_for_listener);

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                check_for_updates(handle).await;
            });

            info!("{} started", APP_NAME);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running FlairBloom");
}

fn check_agreement(app: &tauri::AppHandle) -> bool {
    match app.store(STORE_PATH) {
        Ok(store) => {
            let agreed = store
                .get("agreed")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let version = store
                .get("agreement_version")
                .and_then(|v| v.as_str().map(|s| s.to_string()));
            !agreed || version.as_deref() != Some(AGREEMENT_VERSION)
        }
        Err(e) => {
            warn!("无法读取协议状态: {}", e);
            true // 出错时展示协议，保证合规
        }
    }
}

fn load_or_init_profile(app: &tauri::AppHandle, engine: &Arc<BurstEngine>) {
    let active_path: Option<String> = app
        .store(STORE_PATH)
        .ok()
        .and_then(|store| {
            store
                .get("activeProfilePath")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
        });

    let profiles_dir = match app.path().app_data_dir() {
        Ok(d) => d.join("profiles"),
        Err(e) => {
            error!("无法获取应用数据目录: {}", e);
            return;
        }
    };

    match active_path {
        Some(path) => match load_profile_from_path(&path, &profiles_dir) {
            Ok(profile) => {
                engine.set_rules(profile.rules);
                info!("已加载配置: {}", path);
            }
            Err(e) => {
                warn!("加载配置失败 ({}): {}，回退到默认配置", path, e);
                if let Err(e2) = init_and_save_default(app, engine) {
                    error!("回退默认配置也失败: {}", e2);
                }
            }
        },
        None => {
            if let Err(e) = init_and_save_default(app, engine) {
                error!("初始化默认配置失败: {}", e);
            }
        }
    }
}

fn load_profile_from_path(path: &str, profiles_dir: &std::path::Path) -> Result<qzh_format::profile::Profile, String> {
    let file_name = std::path::Path::new(path)
        .file_name()
        .ok_or("无效文件路径")?
        .to_string_lossy();
    let safe_path = profiles_dir.join(file_name.as_ref());
    let data = std::fs::read(&safe_path).map_err(|e| format!("读取文件失败: {}", e))?;
    let header =
        qzh_format::header::FileHeader::from_bytes(&data).ok_or("文件格式无效")?;
    let aad = header.aad();
    let ciphertext = &data[qzh_format::header::FileHeader::SIZE..];
    let plaintext =
        crypto::aes::decrypt(ciphertext, &header.nonce, &aad).map_err(|e| e.to_string())?;

    let value: serde_json::Value =
        serde_json::from_slice(&plaintext).map_err(|e| format!("解析失败: {}", e))?;
    let version = value
        .get("schema_version")
        .and_then(|v| v.as_u64())
        .unwrap_or(qzh_format::profile::CURRENT_SCHEMA_VERSION as u64) as u32;

    let value = if version < qzh_format::profile::CURRENT_SCHEMA_VERSION {
        qzh_format::migrate::migrate_profile(value, version)
            .map_err(|e| format!("配置迁移失败: {}", e))?
    } else if version > qzh_format::profile::CURRENT_SCHEMA_VERSION {
        return Err(format!(
            "配置版本 {} 高于当前支持的版本 {}，请升级应用",
            version,
            qzh_format::profile::CURRENT_SCHEMA_VERSION
        ));
    } else {
        value
    };

    let profile: qzh_format::profile::Profile =
        serde_json::from_value(value).map_err(|e| format!("反序列化失败: {}", e))?;
    profile.validate().map_err(|e| e.to_string())?;
    Ok(profile)
}

fn init_and_save_default(app: &tauri::AppHandle, engine: &Arc<BurstEngine>) -> Result<(), String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let profile = qzh_format::profile::Profile {
        schema_version: qzh_format::profile::CURRENT_SCHEMA_VERSION,
        meta: qzh_format::profile::ProfileMeta {
            name: "默认配置".to_string(),
            created_at: now,
            updated_at: now,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
        },
        rules: vec![
            qzh_format::profile::BurstRule {
                id: "default-hold".to_string(),
                enabled: false,
                trigger_key: 0x51,
                target_key: 0x51,
                mode: qzh_format::profile::BurstMode::Hold,
                stop_key: None,
                interval_ms: 10,
            },
            qzh_format::profile::BurstRule {
                id: "default-toggle".to_string(),
                enabled: false,
                trigger_key: 0x46,
                target_key: 0x46,
                mode: qzh_format::profile::BurstMode::Toggle,
                stop_key: None,
                interval_ms: 10,
            },
        ],
        hotkeys: qzh_format::profile::Hotkeys::default(),
        advanced: qzh_format::profile::Advanced::default(),
    };

    let dir = match app.path().app_data_dir() {
        Ok(d) => d.join("profiles"),
        Err(e) => {
            return Err(format!("无法获取应用数据目录: {}", e));
        }
    };

    if let Err(e) = std::fs::create_dir_all(&dir) {
        return Err(format!("无法创建配置目录: {}", e));
    }

    let json = match serde_json::to_vec(&profile) {
        Ok(j) => j,
        Err(e) => {
            return Err(format!("序列化失败: {}", e));
        }
    };

    let aad = qzh_format::header::FileHeader::new([0u8; 12]).aad();
    let (ciphertext, nonce) = match crypto::aes::encrypt(&json, &aad) {
        Ok(c) => c,
        Err(e) => {
            return Err(format!("加密失败: {}", e));
        }
    };

    let header = qzh_format::header::FileHeader::new(nonce);
    let file_path = dir.join("默认配置.qzh");
    let mut data = header.to_bytes();
    data.extend_from_slice(&ciphertext);

    if let Err(e) = std::fs::write(&file_path, &data) {
        return Err(format!("写入配置失败: {}", e));
    }

    let path_str = file_path.to_string_lossy().to_string();

    if let Ok(store) = app.store(STORE_PATH) {
        store.set("activeProfilePath", serde_json::json!(path_str));
        if let Err(e) = store.save() {
            warn!("保存存储失败: {}", e);
        }
    }

    engine.set_rules(profile.rules);
    info!("默认配置已创建: {}", path_str);
    Ok(())
}

async fn check_for_updates(app: tauri::AppHandle) {
    use tauri_plugin_updater::UpdaterExt;

    // 优先应用上次已下载的待安装包
    if try_apply_pending_update(&app).await {
        return; // 安装触发后应用会重启，无需继续
    }

    // 静默检查新版本并自动下载（不弹"已是最新版本"提示）
    let updater = match app.updater() {
        Ok(u) => u,
        Err(e) => {
            warn!("updater not available: {}", e);
            return;
        }
    };
    let update = match updater.check().await {
        Ok(Some(u)) => u,
        Ok(None) => {
            info!("app is up to date");
            return;
        }
        Err(e) => {
            warn!("update check failed: {}", e);
            return;
        }
    };

    let version = update.version.clone();
    let notes = update.body.clone();
    info!("update available: {}", version);
    let _ = app.emit("update-downloading", &version);

    let bytes = match update.download(|_, _| {}, || {}).await {
        Ok(b) => b,
        Err(e) => {
            warn!("update download failed: {}", e);
            return;
        }
    };

    let dir = match app
        .path()
        .app_local_data_dir()
        .map(|d| d.join("pending_update"))
    {
        Ok(d) => d,
        Err(e) => {
            warn!("can't get data dir: {}", e);
            return;
        }
    };

    if std::fs::create_dir_all(&dir).is_ok()
        && std::fs::write(dir.join("installer"), &bytes).is_ok()
        && std::fs::write(dir.join("version"), &version).is_ok()
    {
        let _ = app.emit(
            "update-ready",
            serde_json::json!({ "version": version, "notes": notes }),
        );
    }
}
