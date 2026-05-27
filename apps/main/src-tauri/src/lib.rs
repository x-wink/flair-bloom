use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tauri::{Emitter, Manager};
use tauri_plugin_store::StoreExt;
use tracing::{error, info, warn};

mod commands;
mod engine;
mod tray;

use commands::{
    app::{
        agree_license, check_update, exit_app, needs_agreement, try_apply_pending_update,
        UpdateLock,
    },
    engine::{
        get_active_rules, get_global_enabled, get_input_mode, get_rules, install_dd_hid_driver,
        install_driver, is_dd_hid_driver_installed, is_driver_installed, is_elevated,
        relaunch_as_admin, set_global_enabled, set_input_mode, set_rules, uninstall_dd_hid_driver,
        uninstall_driver, EngineState,
    },
    log::{log_from_frontend, open_app_dir},
    profile::{
        delete_profile, fork_active_profile, get_active_profile_path, init_default_profile,
        list_profiles, load_profile, rename_profile, save_profile,
    },
    status::get_app_status,
};
use engine::{burst::start_listener, BurstEngine};

pub const APP_NAME: &str = "FlairBloom";
pub const APP_NAME_CN: &str = "气质花按键助手";
const AGREEMENT_VERSION: &str = "1.2";
const STORE_PATH: &str = "settings.json";
const APP_IDENTIFIER: &str = "fun.xwink.flairbloom";

pub fn log_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(base).join(APP_IDENTIFIER).join("logs")
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home)
            .join("Library/Logs")
            .join(APP_IDENTIFIER)
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        PathBuf::from(".").join("logs")
    }
}

fn cleanup_old_logs(dir: &PathBuf) {
    let cutoff = SystemTime::now() - Duration::from_secs(7 * 24 * 3600);
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with("crash-") {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        if modified < cutoff {
            let _ = std::fs::remove_file(&path);
        }
    }
}

pub fn run() {
    let dir = log_dir();
    std::fs::create_dir_all(&dir).ok();

    let file_appender = tracing_appender::rolling::daily(&dir, "flair-bloom");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    // Leak the guard so the background writer thread lives until process::exit.
    // Tauri's event loop calls process::exit() bypassing destructors; leaking
    // ensures the writer thread stays alive and the OS flushes the fd on exit.
    Box::leak(Box::new(guard));

    use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(fmt::layer().with_writer(std::io::stdout))
        .with(fmt::layer().with_writer(non_blocking).with_ansi(false))
        .init();

    // panic hook：写崩溃日志到同一目录
    let crash_dir = dir.clone();
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // panic hook 内绝不能再 panic，故此处必须容忍时钟异常：
        // 时钟早于 UNIX epoch 时退化为 crash-0.log（可能覆盖旧崩溃日志，
        // 但保住 hook 自身不再二次 panic 是更高优先级）。
        let ts = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let crash_file = crash_dir.join(format!("crash-{ts}.log"));
        let msg = format!(
            "{}\n\nBacktrace:\n{:?}",
            info,
            std::backtrace::Backtrace::force_capture()
        );
        let _ = std::fs::write(&crash_file, &msg);
        eprintln!("PANIC: {info}");
        prev_hook(info);
    }));

    cleanup_old_logs(&dir);

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
        .manage(UpdateLock(AtomicBool::new(false)))
        .invoke_handler(tauri::generate_handler![
            set_global_enabled,
            get_global_enabled,
            set_rules,
            get_rules,
            get_active_rules,
            get_input_mode,
            set_input_mode,
            is_driver_installed,
            install_driver,
            uninstall_driver,
            is_dd_hid_driver_installed,
            install_dd_hid_driver,
            uninstall_dd_hid_driver,
            is_elevated,
            relaunch_as_admin,
            save_profile,
            load_profile,
            list_profiles,
            init_default_profile,
            get_active_profile_path,
            rename_profile,
            delete_profile,
            fork_active_profile,
            needs_agreement,
            agree_license,
            check_update,
            exit_app,
            log_from_frontend,
            open_app_dir,
            get_app_status,
        ])
        .setup(move |app| {
            let need_agreement = check_agreement(app.handle());
            load_or_init_profile(app.handle(), &burst_engine);

            init_input_backend(app.handle());

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
    let active_path: Option<String> = app.store(STORE_PATH).ok().and_then(|store| {
        store
            .get(commands::profile::ACTIVE_PATH_KEY)
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
                if let Err(e2) = commands::profile::create_default_profile(app, engine) {
                    error!("回退默认配置也失败: {}", e2);
                }
            }
        },
        None => {
            if let Err(e) = commands::profile::create_default_profile(app, engine) {
                error!("初始化默认配置失败: {}", e);
            }
        }
    }
}

fn load_profile_from_path(
    path: &str,
    profiles_dir: &std::path::Path,
) -> Result<qzh_format::profile::Profile, String> {
    let file_name = std::path::Path::new(path)
        .file_name()
        .ok_or("无效文件路径")?
        .to_string_lossy();
    let safe_path = profiles_dir.join(file_name.as_ref());
    let data = std::fs::read(&safe_path).map_err(|e| format!("读取文件失败: {e}"))?;
    let header = qzh_format::header::FileHeader::from_bytes(&data).ok_or("文件格式无效")?;
    let aad = header.aad();
    let ciphertext = &data[qzh_format::header::FileHeader::SIZE..];
    let plaintext =
        crypto::aes::decrypt(ciphertext, &header.nonce, &aad).map_err(|e| e.to_string())?;

    let value: serde_json::Value =
        serde_json::from_slice(&plaintext).map_err(|e| format!("解析失败: {e}"))?;
    let version = value
        .get("schema_version")
        .and_then(|v| v.as_u64())
        .unwrap_or(qzh_format::profile::CURRENT_SCHEMA_VERSION as u64) as u32;

    let value = if version < qzh_format::profile::CURRENT_SCHEMA_VERSION {
        qzh_format::migrate::migrate_profile(value, version)
            .map_err(|e| format!("配置迁移失败: {e}"))?
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
        serde_json::from_value(value).map_err(|e| format!("反序列化失败: {e}"))?;
    profile.validate().map_err(|e| e.to_string())?;
    Ok(profile)
}

async fn check_for_updates(app: tauri::AppHandle) {
    // 优先应用上次已下载的待安装包
    if try_apply_pending_update(&app).await {
        return; // 安装触发后应用会重启，无需继续
    }

    let lock = app.state::<UpdateLock>();
    let _guard = match lock.acquire() {
        Some(g) => g,
        None => return,
    };

    // 静默检查新版本并自动下载（不弹"已是最新版本"提示）
    if let Err(e) = do_silent_update(&app).await {
        warn!("silent update failed: {}", e);
    }
}

async fn do_silent_update(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;

    let updater = app.updater().map_err(|e| format!("{e}"))?;
    let update = match updater.check().await {
        Ok(Some(u)) => u,
        Ok(None) => {
            info!("app is up to date");
            return Ok(());
        }
        Err(e) => return Err(format!("update check failed: {e}")),
    };

    let version = update.version.clone();
    let notes = update.body.clone();
    info!("update available: {}", version);
    let _ = app.emit("update-downloading", &version);

    let bytes = update
        .download(|_, _| {}, || {})
        .await
        .map_err(|e| format!("download failed: {e}"))?;

    let dir = app
        .path()
        .app_local_data_dir()
        .map(|d| d.join("pending_update"))
        .map_err(|e| format!("can't get data dir: {e}"))?;

    std::fs::create_dir_all(&dir).map_err(|e| format!("{e}"))?;
    std::fs::write(dir.join("installer"), &bytes).map_err(|e| format!("{e}"))?;
    std::fs::write(dir.join("version"), &version).map_err(|e| format!("{e}"))?;

    let _ = app.emit(
        "update-ready",
        serde_json::json!({ "version": version, "notes": notes }),
    );
    Ok(())
}

fn init_input_backend(app: &tauri::AppHandle) {
    #[cfg(windows)]
    {
        use engine::input::{init_backend, set_resources_dir, InputMode};
        use tauri_plugin_store::StoreExt;

        // 先注册资源目录，DD 后端需要它定位 DLL
        if let Ok(dir) = app.path().resource_dir() {
            set_resources_dir(dir.join("resources"));
        }

        // 启动参数 --switch-mode=<id> 优先于 settings 中的持久值（提权重启场景）
        let cli_mode = parse_switch_mode_arg();

        let stored_mode: Option<String> = app.store(STORE_PATH).ok().and_then(|store| {
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

        // CLI 指定的模式需要持久化，下次启动直接生效
        if let Some(m) = cli_mode {
            if let Ok(store) = app.store(STORE_PATH) {
                store.set("input_mode", serde_json::json!(m));
                let _ = store.save();
            }
        }
    }
    #[cfg(not(windows))]
    let _ = app;
}

#[cfg(windows)]
fn parse_switch_mode_arg() -> Option<String> {
    for arg in std::env::args() {
        if let Some(v) = arg.strip_prefix("--switch-mode=") {
            return Some(v.to_string());
        }
    }
    None
}
