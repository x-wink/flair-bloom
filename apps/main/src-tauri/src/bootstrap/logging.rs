//! tracing 初始化 + panic hook + 旧日志清理。

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crate::APP_IDENTIFIER;

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

pub fn cleanup_old_logs(dir: &PathBuf) {
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

/// 初始化 tracing（stdout + rolling file）+ panic hook。
/// 返回的 `_guard` 必须保持存活直到进程退出（防止 writer 线程提前退出）。
pub fn init(dir: &PathBuf) {
    let file_appender = tracing_appender::rolling::daily(dir, "flair-bloom");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    // Leak the guard so the background writer thread lives until process::exit.
    Box::leak(Box::new(guard));

    use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(fmt::layer().with_writer(std::io::stdout))
        .with(fmt::layer().with_writer(non_blocking).with_ansi(false))
        .init();

    let crash_dir = dir.clone();
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
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
}
