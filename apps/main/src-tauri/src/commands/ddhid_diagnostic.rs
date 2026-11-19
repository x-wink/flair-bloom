//! Unified driver and display diagnostic report export.

#[allow(unused_imports)]
use crate::commands::repair::{DiagnosticItem, ItemStatus, RepairReport, Severity};
#[allow(unused_imports)]
use std::fmt::Write as _;
use tauri::AppHandle;
#[allow(unused_imports)]
use tauri::Manager;

#[tauri::command]
pub async fn export_diagnostic_bundle(
    app: AppHandle,
    environment: serde_json::Value,
    progress: tauri::ipc::Channel<serde_json::Value>,
) -> Result<String, String> {
    #[cfg(windows)]
    {
        static BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        struct ExportGuard;
        impl Drop for ExportGuard {
            fn drop(&mut self) {
                BUSY.store(false, std::sync::atomic::Ordering::Release);
            }
        }
        if BUSY.swap(true, std::sync::atomic::Ordering::AcqRel) {
            return Err("诊断包正在导出".into());
        }
        let guard = ExportGuard;
        let diagnosis = crate::commands::repair::diagnose_environment(app.clone()).await;
        tauri::async_runtime::spawn_blocking(move || {
            let _guard = guard;
            bundle::export(app, environment, progress, diagnosis)
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    {
        let _ = (app, environment, progress);
        Err("完整诊断包需要 Windows".into())
    }
}

#[cfg(windows)]
fn append_header(
    out: &mut String,
    out_file: &std::path::Path,
    resources_root: Option<&std::path::Path>,
) {
    let _ = writeln!(out, "FlairBloom environment diagnostic report");
    let _ = writeln!(
        out,
        "generated_at_slug: {}",
        chrono::Local::now().to_rfc3339()
    );
    let _ = writeln!(out, "app_name: {}", crate::APP_NAME);
    let _ = writeln!(out, "app_version: {}", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(out, "output_file: {}", out_file.display());
    let _ = writeln!(
        out,
        "resource_dir: {}",
        resources_root
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<unavailable>".to_string())
    );
}

#[cfg(windows)]
fn append_app_diagnosis(out: &mut String, report: &RepairReport) {
    push_section(out, "App diagnosis summary");
    let _ = writeln!(out, "scan_time: {}", report.timestamp);
    if report.items.is_empty() {
        let _ = writeln!(out, "<no diagnosis items>");
        return;
    }
    for item in &report.items {
        append_diagnosis_item(out, item);
    }
}

#[cfg(windows)]
fn append_diagnosis_item(out: &mut String, item: &DiagnosticItem) {
    let _ = writeln!(out);
    let _ = writeln!(out, "[{}] {}", item.id, item.label);
    let _ = writeln!(out, "category: {}", item.category);
    let _ = writeln!(out, "severity: {}", severity_label(item.severity));
    let _ = writeln!(out, "status: {}", status_label(item.status));
    let _ = writeln!(out, "detail: {}", item.detail);
    let _ = writeln!(
        out,
        "recommended_action: {}",
        item.recommended_action.as_deref().unwrap_or("<none>")
    );
}

#[cfg(windows)]
fn append_resource_files(out: &mut String, resources_root: &std::path::Path) {
    use crate::commands::resource_integrity::{sha256_file_hex, EXPECTED_RESOURCES};

    push_section(out, "Packaged resource files");
    let _ = writeln!(out, "resource_dir: {}", resources_root.display());
    for expected in EXPECTED_RESOURCES {
        let path = resources_root.join(expected.rel);
        let _ = writeln!(out);
        let _ = writeln!(out, "-- {}", path.display());
        let _ = writeln!(out, "label: {}", expected.label);
        let _ = writeln!(out, "expected_size: {}", expected.size);
        let _ = writeln!(out, "expected_sha256: {}", expected.sha256);
        match std::fs::metadata(&path) {
            Ok(meta) => {
                let _ = writeln!(out, "exists: true");
                let _ = writeln!(out, "actual_size: {}", meta.len());
                let _ = writeln!(out, "size_match: {}", meta.len() == expected.size);
                match sha256_file_hex(&path) {
                    Ok(hash) => {
                        let _ = writeln!(out, "actual_sha256: {hash}");
                        let _ = writeln!(out, "sha256_match: {}", hash == expected.sha256);
                    }
                    Err(e) => {
                        let _ = writeln!(out, "hash_error: {e}");
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let _ = writeln!(out, "exists: false");
            }
            Err(e) => {
                let _ = writeln!(out, "metadata_error: {e}");
            }
        }
    }
}

#[cfg(windows)]
fn push_section(out: &mut String, name: &str) {
    let _ = writeln!(out);
    let _ = writeln!(out, "==== {name} ====");
}

#[cfg(windows)]
fn severity_label(severity: Severity) -> &'static str {
    match severity {
        Severity::Info => "info",
        Severity::Warn => "warn",
        Severity::Error => "error",
    }
}

#[cfg(windows)]
fn status_label(status: ItemStatus) -> &'static str {
    match status {
        ItemStatus::Ok => "ok",
        ItemStatus::Orphan => "orphan",
        ItemStatus::Missing => "missing",
        ItemStatus::Corrupted => "corrupted",
        ItemStatus::Unknown => "unknown",
    }
}

#[cfg(windows)]
mod bundle {
    use super::*;
    use crate::commands::engine::EngineState;
    use chrono::{DateTime, Local, NaiveDate, Utc};
    use std::{
        fs,
        io::{self, BufRead, BufReader, Write},
        os::windows::process::CommandExt,
        path::{Path, PathBuf},
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    use tauri::ipc::Channel;
    use zip::{write::SimpleFileOptions, ZipWriter};

    fn verified_resource(path: &Path, expected: &[u8]) -> Result<fs::File, String> {
        use std::{io::Read, os::windows::fs::OpenOptionsExt};
        use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;
        // 校验和进程运行期间持有同一只读句柄，避免执行被替换或改写的资源。
        let mut file = fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(path)
            .map_err(|e| format!("无法读取 {}：{e}", path.display()))?;
        let mut content = Vec::new();
        (&mut file)
            .take(expected.len() as u64 + 1)
            .read_to_end(&mut content)
            .map_err(|e| e.to_string())?;
        if content != expected {
            return Err(format!("诊断资源校验失败：{}", path.display()));
        }
        Ok(file)
    }

    fn run(command: &mut Command, timeout: u64) -> Result<(), String> {
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|e| e.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(timeout);
        loop {
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                return if status.success() {
                    Ok(())
                } else {
                    Err(format!("退出状态 {status}"))
                };
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("超过 {timeout} 秒，保留已采集的证据"));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn stage(report: &mut String, title: &str, result: Result<(), String>) {
        let status = result.err().unwrap_or_else(|| "完成".into());
        let _ = writeln!(report, "{title}：{status}");
    }

    fn include_text(report: &mut String, path: &Path, title: &str) {
        push_section(report, title);
        match fs::read_to_string(path) {
            Ok(s) => report.push_str(s.trim_start_matches('\u{feff}')),
            Err(e) => {
                let _ = writeln!(report, "采集文件不可用：{e}");
            }
        }
        report.push('\n');
    }

    fn collect_panel_received(
        app: &AppHandle,
        path: &Path,
        progress: &Channel<serde_json::Value>,
        report: &mut String,
    ) {
        use serde::Deserialize;
        use tauri::Listener;
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Counts {
            name: String,
            attempts: u32,
            key_hits: u32,
            wheel_hits: u32,
            down: u32,
            up: u32,
            character: u32,
            wheel: u32,
            blur: u32,
        }
        push_section(report, "气质花前台面板实际送达（每段三次）");
        let Ok(content) = fs::read_to_string(path.with_extension("samples")) else {
            report.push_str("没有完整采样区间，无法判断送达。\n");
            return;
        };
        let windows: Vec<_> = content
            .lines()
            .filter_map(|line| {
                let mut fields = line.trim_start_matches('\u{feff}').split('\t');
                let name = fields.next()?;
                let start = fields.next()?.parse::<i64>().ok()?;
                let end = fields.next()?.parse::<i64>().ok()?;
                Some(serde_json::json!({ "name": name, "start":start, "end":end }))
            })
            .collect();
        let event = format!("diagnostic-observations-{}", uuid::Uuid::new_v4().simple());
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let listener = app.listen(event.clone(), move |e| {
            let _ = tx.try_send(e.payload().to_owned());
        });
        let sent = progress
            .send(serde_json::json!({"reply": event,"samples":windows}))
            .is_ok();
        let rows = sent
            .then(|| rx.recv_timeout(Duration::from_secs(3)).ok())
            .flatten()
            .and_then(|payload| serde_json::from_str::<Vec<Counts>>(&payload).ok());
        app.unlisten(listener);
        let Some(rows) = rows else {
            report.push_str("面板计数未返回，不能判断送达。\n");
            return;
        };
        let mut control_key = false;
        let mut control_wheel = false;
        for row in rows {
            let keys = row.key_hits == 3;
            let wheels = row.wheel_hits == 3;
            let verdict = match row.name.as_str() {
                _ if row.attempts != 3 => "采样未完成，结果无效",
                _ if row.blur > 0 => "期间失焦，结果无效",
                "轻量按键" => {
                    control_key = keys;
                    if keys {
                        "对照通过"
                    } else {
                        "对照未通过"
                    }
                }
                "轻量滚轮" => {
                    control_wheel = wheels;
                    if wheels {
                        "对照通过"
                    } else {
                        "对照未通过"
                    }
                }
                "网吧按键" if keys => "已送达",
                "网吧滚轮" if wheels => "已送达",
                "网吧按键" if control_key => "未完整送达",
                "网吧滚轮" if control_wheel => "未完整送达",
                _ => "对应轻量对照未通过，无法判定",
            };
            let counts = if row.name.ends_with("滚轮") {
                format!(
                    "收到 {} 个滚轮事件，命中 {}/{} 次发送",
                    row.wheel, row.wheel_hits, row.attempts
                )
            } else {
                format!(
                    "按下/抬起 {}/{}，字符 1 共 {} 个，命中 {}/{} 次发送",
                    row.down, row.up, row.character, row.key_hits, row.attempts
                )
            };
            let _ = writeln!(report, "{}：{}。{}", row.name, counts, verdict);
        }
    }

    fn delivery(
        app: &AppHandle,
        tool: &Path,
        dir: &Path,
        sdk: &Path,
        report: &mut String,
        progress: &Channel<serde_json::Value>,
    ) {
        use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        let engine = app.state::<EngineState>();
        let checked = engine.0.with_input_diagnosis(|| {
            let _ = progress.send("正在验证游戏模式键盘槽".into());
            push_section(report, "游戏模式逐槽 F24 送达");
            match win_input::interception::probe_keyboard_slots() {
                Some(probe) => {
                    let _ = writeln!(
                        report,
                        "首个通过槽：{:?}\n{}",
                        probe.selected(),
                        serde_json::to_string_pretty(&probe).unwrap_or_default()
                    );
                }
                None => report.push_str("Interception 未安装或通道被占用。\n"),
            }
            let panel = app.get_webview_window("panel").and_then(|w| w.hwnd().ok());
            let Some(panel) = panel else {
                return Err("部分完成，网吧模式测试跳过：无法取得面板窗口。".into());
            };
            if panel.0 != unsafe { GetForegroundWindow() } {
                return Err("部分完成，网吧模式测试跳过：面板已失去前台。".into());
            }
            let _ = progress.send("正在验证网吧模式按键和滚轮".into());
            let was_dd = win_input::current_mode() == win_input::InputMode::DdSimple;
            if was_dd {
                win_input::init_backend(win_input::InputMode::SendInput);
            }
            let path = dir.join("delivery.txt");
            let result = run(
                Command::new(tool)
                    .arg("delivery")
                    .arg(&path)
                    .arg(sdk)
                    .arg((panel.0 as usize).to_string()),
                40,
            );
            if was_dd {
                win_input::init_backend(win_input::InputMode::DdSimple);
                if win_input::current_mode() != win_input::InputMode::DdSimple {
                    report.push_str("网吧模式未能恢复，当前已回退轻量模式。\n");
                    crate::commands::status::emit_status_changed(app);
                }
            }
            include_text(report, &path, "网吧模式键鼠送达与原始设备");
            collect_panel_received(app, &path, progress, report);
            result.map_err(|e| format!("网吧模式采样未完成：{e}"))
        });
        stage(report, "主动检测", checked.and_then(|result| result));
    }

    pub(super) fn export(
        app: AppHandle,
        environment: serde_json::Value,
        progress: Channel<serde_json::Value>,
        diagnosis: Result<RepairReport, String>,
    ) -> Result<String, String> {
        let start = Local::now();
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let desktop = app
            .path()
            .desktop_dir()
            .map_err(|e| format!("无法定位桌面：{e}"))?;
        fs::create_dir_all(&desktop).map_err(|e| e.to_string())?;
        let desktop = win_driver::path_util::strip_verbatim(desktop);
        let out = desktop.join(format!(
            "气质花诊断-{}-{}.zip",
            start.format("%Y%m%d-%H%M%S"),
            &uuid::Uuid::new_v4().simple().to_string()[..8]
        ));
        let resources = win_driver::path_util::strip_verbatim(
            app.path()
                .resource_dir()
                .map_err(|e| e.to_string())?
                .join("resources"),
        );
        let mut report = String::new();
        append_header(&mut report, &out, Some(&resources));
        let _ = writeln!(report, "本机开始时间：{start}");
        push_section(&mut report, "系统环境（含界面实测）");
        report.push_str(&serde_json::to_string_pretty(&environment).map_err(|e| e.to_string())?);
        match diagnosis {
            Ok(result) => append_app_diagnosis(&mut report, &result),
            Err(e) => stage(&mut report, "应用轻量诊断", Err(e)),
        }
        let tool = resources.join("input-diagnostics.exe");
        let ready = verified_resource(
            &tool,
            include_bytes!(concat!(env!("OUT_DIR"), "/input-diagnostics.exe")),
        );
        match &ready {
            Ok(_) => delivery(
                &app,
                &tool,
                dir.path(),
                &resources.join("dd63330.dll"),
                &mut report,
                &progress,
            ),
            Err(e) => stage(&mut report, "主动检测程序", Err(e.clone())),
        }
        let _ = progress.send("正在采集设备栈、驱动和系统事件".into());
        append_resource_files(&mut report, &resources);
        let script = resources.join("diagnose-environment.ps1");
        let evidence = dir.path().join("environment.txt");
        let result = verified_resource(
            &script,
            include_bytes!("../../../../../scripts/diagnose-environment.ps1"),
        )
        .and_then(|_script| {
            let windows = std::env::var_os("SystemRoot").ok_or("无法定位 Windows 目录")?;
            run(
                Command::new(
                    PathBuf::from(windows).join(r"System32\WindowsPowerShell\v1.0\powershell.exe"),
                )
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                ])
                .arg(&script)
                .arg("-DriverDir")
                .arg(resources.join("ddhid-driver"))
                .arg("-OutFile")
                .arg(&evidence)
                .arg("-EvidenceOnly"),
                150,
            )
        });
        stage(&mut report, "Windows 设备采集", result);
        include_text(&mut report, &evidence, "Windows 设备与驱动证据");
        let events = dir.path().join("events.txt");
        let result = ready.and_then(|_tool| {
            run(
                Command::new(&tool)
                    .arg("events")
                    .arg(&events)
                    .arg(start.date_naive().to_string()),
                120,
            )
        });
        stage(&mut report, "系统事件与 WER", result);
        include_text(&mut report, &events, "当天的 Windows 错误报告");
        let _ = progress.send("正在整理当天日志并生成桌面 ZIP".into());
        let temp = tempfile::NamedTempFile::new_in(&desktop).map_err(|e| e.to_string())?;
        {
            let mut zip = ZipWriter::new(temp.as_file());
            add_logs(&mut zip, &crate::log_dir(), start.date_naive(), &mut report)
                .map_err(|e| e.to_string())?;
            zip.start_file("完整诊断报告.txt", SimpleFileOptions::default())
                .map_err(|e| e.to_string())?;
            zip.write_all(b"\xef\xbb\xbf")
                .and_then(|()| zip.write_all(report.as_bytes()))
                .map_err(|e| e.to_string())?;
            zip.finish().map_err(|e| e.to_string())?;
        }
        temp.persist_noclobber(&out)
            .map_err(|e| format!("无法保存诊断包：{e}"))?;
        Ok(out.display().to_string())
    }

    fn add_logs<W: Write + io::Seek>(
        zip: &mut ZipWriter<W>,
        dir: &Path,
        day: NaiveDate,
        report: &mut String,
    ) -> io::Result<()> {
        push_section(report, "当天应用日志（本机日期）");
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) => {
                let _ = writeln!(report, "日志目录不可读：{e}");
                return Ok(());
            }
        };
        let mut count = 0;
        for entry in entries {
            let entry = match entry {
                Ok(v) => v,
                Err(e) => {
                    let _ = writeln!(report, "读取日志条目失败：{e}");
                    continue;
                }
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            let daily = name
                .strip_prefix("flair-bloom.")
                .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
            let crash = name
                .strip_prefix("crash-")
                .and_then(|s| s.strip_suffix(".log"))
                .and_then(|s| s.parse::<i64>().ok())
                .and_then(|s| DateTime::<Utc>::from_timestamp(s, 0))
                .is_some_and(|t| t.with_timezone(&Local).date_naive() == day);
            if !crash && daily.is_none_or(|d| (d - day).num_days().abs() > 1) {
                continue;
            }
            let file = match fs::File::open(entry.path()) {
                Ok(f) => f,
                Err(e) => {
                    let _ = writeln!(report, "{name}：{e}");
                    continue;
                }
            };
            let mut inside = crash;
            let mut opened = false;
            for line in BufReader::new(file).split(b'\n') {
                let line = match line {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = writeln!(report, "{name} 未读完：{e}");
                        break;
                    }
                };
                if !crash {
                    if let Some(t) = line
                        .split(|b| b.is_ascii_whitespace())
                        .next()
                        .and_then(|v| std::str::from_utf8(v).ok())
                        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                    {
                        inside = t.with_timezone(&Local).date_naive() == day;
                    }
                }
                if inside {
                    if !opened {
                        zip.start_file(format!("日志/{name}"), SimpleFileOptions::default())?;
                        opened = true;
                        count += 1;
                    }
                    zip.write_all(&line)?;
                    zip.write_all(b"\n")?;
                }
            }
            if opened {
                let _ = writeln!(report, "日志/{name}");
            }
        }
        let _ = writeln!(report, "共 {count} 个日志文件。");
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use chrono::{Days, TimeZone};
        use std::io::{Cursor, Read};

        #[test]
        fn diagnostic_resource_must_match_and_stay_read_only_during_use() {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("sample");
            fs::write(&path, b"original").unwrap();
            assert!(verified_resource(&path, b"other").is_err());
            let guard = verified_resource(&path, b"original").unwrap();
            assert!(fs::write(&path, b"modified").is_err());
            drop(guard);
            fs::write(&path, b"modified").unwrap();
            assert!(verified_resource(&path, b"original").is_err());
        }

        #[test]
        fn bundle_logs_use_local_event_dates_and_keep_multiline_errors() {
            let day = NaiveDate::from_ymd_opt(2026, 11, 19).unwrap();
            let local = Local
                .from_local_datetime(&day.and_hms_opt(12, 0, 0).unwrap())
                .single()
                .unwrap();
            let today = local.to_utc();
            let prior = today.checked_sub_days(Days::new(1)).unwrap();
            let temp = tempfile::tempdir().unwrap();
            let name = format!("flair-bloom.{}", today.date_naive());
            fs::write(
                temp.path().join(&name),
                format!(
                    "{} INFO older\n{} ERROR today\n  detail\n",
                    prior.to_rfc3339(),
                    today.to_rfc3339()
                ),
            )
            .unwrap();
            fs::write(
                temp.path().join(format!("crash-{}.log", today.timestamp())),
                "崩溃",
            )
            .unwrap();
            fs::write(temp.path().join("settings.json"), "不属于日志").unwrap();
            let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
            let mut report = String::new();
            add_logs(&mut writer, temp.path(), day, &mut report).unwrap();
            let mut zip = zip::ZipArchive::new(writer.finish().unwrap()).unwrap();
            assert_eq!(zip.len(), 2);
            let mut log = String::new();
            zip.by_name(&format!("日志/{name}"))
                .unwrap()
                .read_to_string(&mut log)
                .unwrap();
            assert!(log.contains("ERROR today\n  detail"));
            assert!(!log.contains("older"));
        }
    }
}
