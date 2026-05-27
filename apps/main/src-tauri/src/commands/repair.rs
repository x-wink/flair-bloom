//! 环境诊断与修复。
//!
//! 解决用户在卸载 / 强删驱动文件后陷入"半卸载"状态，以及全新装机环境下
//! 因 Windows 安全策略 / 杀软拦截导致驱动首次安装失败的问题：
//! - DD-HID：sys 没了但 PnP 服务键 / OEM INF 残留 → 重装报 install error
//! - Interception：服务键残留但 sys 缺失 → 安装器复装失败
//! - .qzh 损坏：AES Tag 校验失败时启动会回退默认配置，但损坏文件还在原地占位
//! - 安装前置环境：HVCI 阻断内核驱动加载、SAC 阻断未知信誉 exe、
//!   Defender 实时保护拦截 ddc.exe / ddhid63340.sys、待重启的 PnP 事务等
//!
//! 模块对外暴露三类入口：
//! 1. [`diagnose_environment`]：只读，不提权，列出所有可疑残留与安装前置异常
//! 2. `repair_*`：每类问题一个修复命令，全部走现有 `run_elevated_exe_capture`
//! 3. 修复前自动 `Export-WindowsDriver` / `reg export` 到
//!    `{app_local_data_dir}/repair_backup/<timestamp>/`，留回滚余地
//!
//! 所有 PowerShell 脚本都是幂等的（`Test-Path` / `Get-Service` 先判存再动手），
//! 多次调用不会重复破坏，方便用户反复点击"修复"。

#![allow(dead_code)] // 非 Windows 平台修复函数体为 stub，但类型仍需导出供前端 invoke

use serde::Serialize;
#[allow(unused_imports)]
use tauri::{AppHandle, Manager};

// ===== 公共数据结构 =====

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// 仅供参考的状态项（如"配置目录可写"）
    Info,
    /// 异常但不影响核心功能（如旧日志未清理）
    Warn,
    /// 影响功能（如驱动残留导致重装失败）
    Error,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ItemStatus {
    /// 一切正常
    Ok,
    /// PnP / 服务键残留但实际驱动文件不存在
    Orphan,
    /// 应该存在的资源缺失
    Missing,
    /// 文件存在但内容损坏
    Corrupted,
    /// 检测过程异常，状态未知
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticItem {
    /// 稳定的项目 ID，前端用作 key
    pub id: String,
    /// 分类，用于前端分组
    pub category: String,
    /// 用户友好的项目名
    pub label: String,
    pub severity: Severity,
    pub status: ItemStatus,
    /// 详情：路径、OEM INF 编号、错误描述等
    pub detail: String,
    /// 推荐执行的修复 command 名（前端 invoke 用）
    pub recommended_action: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RepairReport {
    /// ISO-8601 形式的时间戳
    pub timestamp: String,
    pub items: Vec<DiagnosticItem>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Ok,
    /// 该步骤本就不需要执行（如目标本来就不存在）
    Skipped,
    /// 物理执行失败
    Failed,
    /// 已标记重启删除
    PendingReboot,
}

#[derive(Debug, Clone, Serialize)]
pub struct RepairStep {
    pub name: String,
    pub status: StepStatus,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RepairOutcome {
    /// 整体是否成功（任意 Failed 即视为整体失败）
    pub success: bool,
    /// 是否需要重启
    pub pending_reboot: bool,
    /// 用户可见的总结文案
    pub summary: String,
    /// 每个修复步骤的详细结果，前端按列表展示
    pub steps: Vec<RepairStep>,
    /// 备份目录（若产生备份），出错时用户可手动恢复
    pub backup_dir: Option<String>,
}

// ===== 时间戳 / 备份目录 =====

/// 形如 `20260527-153045` 的时间戳，跨平台、文件系统安全。
fn timestamp_slug() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    // 朴素 UTC 拆分；只用于文件名，不需要时区精确
    let day_secs = 86_400u64;
    let days_since_epoch = secs / day_secs;
    let time_of_day = secs % day_secs;
    let h = time_of_day / 3600;
    let m = (time_of_day / 60) % 60;
    let s = time_of_day % 60;
    let (y, mo, d) = ymd_from_days(days_since_epoch as i64);
    format!("{y:04}{mo:02}{d:02}-{h:02}{m:02}{s:02}")
}

/// 1970-01-01 起 N 天 → (年, 月, 日)。简易实现，足够文件名用。
fn ymd_from_days(days: i64) -> (i32, u32, u32) {
    // Howard Hinnant 的 days_from_civil 反算
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

#[cfg(windows)]
fn ensure_backup_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let base = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("无法获取本地数据目录: {e}"))?
        .join("repair_backup")
        .join(timestamp_slug());
    std::fs::create_dir_all(&base).map_err(|e| format!("创建备份目录失败: {e}"))?;
    Ok(base)
}

// ===== 诊断（只读）=====

#[tauri::command]
pub async fn diagnose_environment(app: AppHandle) -> Result<RepairReport, String> {
    let mut items = Vec::new();

    #[cfg(windows)]
    {
        items.extend(diagnose_install_prerequisites(&app).await);
        items.extend(diagnose_dd_hid(&app));
        items.extend(diagnose_interception(&app));
    }

    items.extend(diagnose_profiles(&app));
    items.extend(diagnose_logs(&app));

    #[cfg(not(windows))]
    let _ = &app;

    Ok(RepairReport {
        timestamp: timestamp_slug(),
        items,
    })
}

// ===== 安装前置检查（Windows 安全策略 / 杀软 / 资源 / 架构）=====

/// HVCI / 内存完整性是否启用。
///
/// 注册表里 `Enabled=1` 表示策略要求启用；但 Windows 重启后 HVCI 才真正激活，
/// 因此还要看 `RunningEnforcement` 状态。两个都为 1 才是"已生效"。
/// 任何一个非 0 就要给用户提示——它会拒绝加载非 HVCI 兼容的内核驱动。
#[cfg(windows)]
fn detect_hvci_active() -> Option<bool> {
    let policy = read_hklm_dword(
        "SYSTEM\\CurrentControlSet\\Control\\DeviceGuard\\Scenarios\\HypervisorEnforcedCodeIntegrity",
        "Enabled",
    );
    let running = read_hklm_dword(
        "SYSTEM\\CurrentControlSet\\Control\\DeviceGuard\\Scenarios\\HypervisorEnforcedCodeIntegrity",
        "RunningEnforcement",
    );
    match (policy, running) {
        (None, None) => None,
        (p, r) => Some(p.unwrap_or(0) == 1 || r.unwrap_or(0) == 1),
    }
}

/// Smart App Control 状态。
///
/// `VerifiedAndReputablePolicyState` 取值：
///   0 = Off, 1 = Enforce, 2 = Evaluation
/// Enforce / Evaluation 都会阻止 ddc.exe 这类无强信誉的可执行文件运行。
#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SacState {
    Off,
    Enforce,
    Evaluation,
    Unknown,
}

#[cfg(windows)]
fn detect_sac_state() -> SacState {
    match read_hklm_dword(
        "SYSTEM\\CurrentControlSet\\Control\\CI\\Policy",
        "VerifiedAndReputablePolicyState",
    ) {
        Some(0) => SacState::Off,
        Some(1) => SacState::Enforce,
        Some(2) => SacState::Evaluation,
        Some(_) => SacState::Unknown,
        None => SacState::Off, // 多数设备不存在该键 = 未启用
    }
}

/// 系统是否有挂起的重启请求。
///
/// 任何一个常见标记命中即视为 pending：
/// - `PendingFileRenameOperations` (SessionManager)
/// - `RebootPending` 子键（Component Based Servicing）
/// - `RebootRequired` 子键（WindowsUpdate / AutoUpdate）
///
/// 这种状态下任何 PnP 安装事务都会被推迟到下次开机，重装驱动必然失败。
#[cfg(windows)]
fn detect_pending_reboot() -> bool {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ,
    };
    // 1. PendingFileRenameOperations（REG_MULTI_SZ，存在即代表有 pending 文件改名）
    let mut sm: HKEY = std::ptr::null_mut();
    let path = wide("SYSTEM\\CurrentControlSet\\Control\\Session Manager");
    // SAFETY: path NUL 结尾；sm 是栈上出参指针
    let r = unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, path.as_ptr(), 0, KEY_READ, &mut sm) };
    if r == 0 {
        let name = wide("PendingFileRenameOperations");
        let mut ty: u32 = 0;
        let mut len: u32 = 0;
        // SAFETY: sm 已 open；data 传 null 仅探测大小
        let q = unsafe {
            RegQueryValueExW(
                sm,
                name.as_ptr(),
                std::ptr::null_mut(),
                &mut ty,
                std::ptr::null_mut(),
                &mut len,
            )
        };
        // SAFETY: sm 上面 open 成功
        unsafe { RegCloseKey(sm) };
        if q == 0 && len > 2 {
            return true;
        }
    }
    // 2. CBS / WindowsUpdate 的 RebootPending 子键
    if hklm_subkey_present(
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Component Based Servicing\\RebootPending",
    ) {
        return true;
    }
    if hklm_subkey_present(
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\WindowsUpdate\\Auto Update\\RebootRequired",
    ) {
        return true;
    }
    false
}

/// 用 `Get-MpPreference` 异步查询 Defender 排除路径列表。
///
/// 返回 None 表示查询失败（PowerShell 不可用 / 进程被卡住等），UI 应显示"无法判断"。
/// 正常返回的字符串列表里每行是一条排除路径，调用方对比 install_path 即可。
#[cfg(windows)]
async fn read_defender_exclusion_paths() -> Option<Vec<String>> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    // 单行管道：用 ConvertTo-Json 避免本地化和换行差异，失败时不打印噪音
    // -Compress 让输出贴一行，更好解析；-Depth 1 足够字符串数组
    let script = "$ErrorActionPreference='Stop'; \
                  try { \
                      $p = Get-MpPreference -ErrorAction Stop; \
                      $arr = @($p.ExclusionPath); \
                      ConvertTo-Json -InputObject $arr -Compress -Depth 2 \
                  } catch { '[]' }";

    // 不提权运行：Get-MpPreference 普通用户即可读
    // 用 spawn_blocking 隔离阻塞调用，避免堵 Tauri 异步执行器
    tauri::async_runtime::spawn_blocking(move || {
        let out = Command::new("C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                script,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .stdin(Stdio::null())
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        parse_exclusion_json(&text)
    })
    .await
    .ok()
    .flatten()
}
///
/// 为了不引入完整 JSON parser，这里只识别 `[..]` 内被双引号括起来的字段；
/// 非数组形式（PowerShell 在数组只有一个元素时输出裸字符串）直接当成单元素。
#[cfg(windows)]
/// 简单 JSON 字符串数组解析：能容忍 `null` / `""` / 单字符串退化为非数组。
///
/// 为了不引入完整 JSON parser，这里只识别 `[..]` 内被双引号括起来的字段；
/// 非数组形式（PowerShell 在数组只有一个元素时输出裸字符串）直接当成单元素。
pub(crate) fn parse_exclusion_json(text: &str) -> Option<Vec<String>> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed == "null" {
        return Some(Vec::new());
    }
    // PowerShell ConvertTo-Json 单元素数组 + -Compress 仍会输出 `["x"]`，
    // 但更老的版本会退化成裸字符串，这里两路都接住
    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        let inner = &trimmed[1..trimmed.len() - 1];
        return Some(vec![unescape_json_string(inner)]);
    }
    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return None;
    }
    let mut out = Vec::new();
    let bytes = trimmed.as_bytes();
    let mut i = 1usize;
    while i < bytes.len() - 1 {
        // 跳过空白与逗号
        while i < bytes.len() - 1
            && (bytes[i] == b' '
                || bytes[i] == b','
                || bytes[i] == b'\n'
                || bytes[i] == b'\r'
                || bytes[i] == b'\t')
        {
            i += 1;
        }
        if i >= bytes.len() - 1 {
            break;
        }
        if bytes[i] != b'"' {
            // 非引号开头视为格式异常
            return None;
        }
        i += 1;
        let start = i;
        while i < bytes.len() - 1 {
            if bytes[i] == b'\\' && i + 1 < bytes.len() - 1 {
                i += 2;
                continue;
            }
            if bytes[i] == b'"' {
                break;
            }
            i += 1;
        }
        if i >= bytes.len() - 1 {
            return None;
        }
        let raw = &trimmed[start..i];
        out.push(unescape_json_string(raw));
        i += 1; // 跳过结束引号
    }
    Some(out)
}

#[cfg(windows)]
fn unescape_json_string(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('/') => out.push('/'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('b') => out.push('\u{0008}'),
            Some('f') => out.push('\u{000C}'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// 给定排除路径列表，判断 `target_dir` 是否已被覆盖。
///
/// 命中规则：忽略大小写 + 去尾斜杠后，target 必须等于某条排除项，
/// 或 target 以"排除项 + `\`"开头（排除项是父目录）。
/// 这样既支持用户加了具体安装目录，也支持加了上级 ProgramFiles 目录。
pub(crate) fn is_path_excluded(target: &str, exclusions: &[String]) -> bool {
    let t = normalize_path(target);
    if t.is_empty() {
        return false;
    }
    for e in exclusions {
        let n = normalize_path(e);
        if n.is_empty() {
            continue;
        }
        if t == n {
            return true;
        }
        let prefix = format!("{n}\\");
        if t.starts_with(&prefix) {
            return true;
        }
    }
    false
}

fn normalize_path(p: &str) -> String {
    let mut s = p.trim().replace('/', "\\").to_lowercase();
    while s.ends_with('\\') {
        s.pop();
    }
    s
}

/// 把架构与 sys 文件位宽匹配为友好状态。
///
/// 仓库内目前只发 `ddhid63340.sys` 的 x64 版本，ARM64 设备无法加载。
/// 返回 (是否兼容, 详情文案)。
fn classify_arch_compat(arch: &str) -> (bool, String) {
    let lower = arch.to_lowercase();
    if lower == "x64" || lower == "amd64" {
        (true, format!("当前架构 {arch}，与 x64 驱动匹配"))
    } else if lower == "arm64" || lower == "aarch64" {
        (
            false,
            format!("当前架构 {arch}，DD-HID 仅提供 x64 驱动，无法加载"),
        )
    } else {
        (false, format!("当前架构 {arch}，DD-HID 仅适用于 x64 系统"))
    }
}

#[cfg(windows)]
async fn diagnose_install_prerequisites(app: &AppHandle) -> Vec<DiagnosticItem> {
    let mut out = Vec::new();

    // ---- 资源完整性（先于一切其它检查，缺文件直接出错）----
    let (resources_ok, missing) = collect_install_resources(app);
    out.push(DiagnosticItem {
        id: "prereq.resources".to_string(),
        category: "安装前置检查".to_string(),
        label: "驱动安装资源".to_string(),
        severity: if resources_ok {
            Severity::Info
        } else {
            Severity::Error
        },
        status: if resources_ok {
            ItemStatus::Ok
        } else {
            ItemStatus::Missing
        },
        detail: if resources_ok {
            "ddc.exe / ddhid63340.sys / .inf / .cat 全部就位".to_string()
        } else {
            format!("缺失或被拦截：{}", missing.join(", "))
        },
        recommended_action: None,
    });

    // ---- 系统架构 ----
    let arch = crate::commands::sysinfo::host_arch();
    let (arch_ok, arch_detail) = classify_arch_compat(&arch);
    out.push(DiagnosticItem {
        id: "prereq.arch".to_string(),
        category: "安装前置检查".to_string(),
        label: "系统架构".to_string(),
        severity: if arch_ok {
            Severity::Info
        } else {
            Severity::Error
        },
        status: if arch_ok {
            ItemStatus::Ok
        } else {
            ItemStatus::Missing
        },
        detail: arch_detail,
        recommended_action: None,
    });

    // ---- HVCI / 内存完整性 ----
    let hvci = detect_hvci_active();
    out.push(DiagnosticItem {
        id: "prereq.hvci".to_string(),
        category: "安装前置检查".to_string(),
        label: "内存完整性 (HVCI)".to_string(),
        severity: match hvci {
            Some(true) => Severity::Error,
            _ => Severity::Info,
        },
        status: match hvci {
            Some(true) => ItemStatus::Orphan, // 借用 Orphan 表示"启用了应该关闭的策略"
            Some(false) => ItemStatus::Ok,
            None => ItemStatus::Unknown,
        },
        detail: match hvci {
            Some(true) => "内核隔离已启用，会拒绝加载未通过 HVCI 兼容认证的驱动。\
                请在 Windows 安全中心 → 设备安全性 → 内核隔离详细信息 → 关闭"
                .to_string(),
            Some(false) => "内核隔离未启用".to_string(),
            None => "未读取到 HVCI 策略键，可能为旧版 Windows".to_string(),
        },
        recommended_action: None,
    });

    // ---- Smart App Control ----
    let sac = detect_sac_state();
    out.push(DiagnosticItem {
        id: "prereq.sac".to_string(),
        category: "安装前置检查".to_string(),
        label: "Smart App Control".to_string(),
        severity: match sac {
            SacState::Enforce | SacState::Evaluation => Severity::Error,
            _ => Severity::Info,
        },
        status: match sac {
            SacState::Enforce | SacState::Evaluation => ItemStatus::Orphan,
            SacState::Off => ItemStatus::Ok,
            SacState::Unknown => ItemStatus::Unknown,
        },
        detail: match sac {
            SacState::Enforce => "已强制启用，会拦截无强信誉的 ddc.exe。\
                请在 Windows 设置 → 隐私与安全 → 应用控制中关闭"
                .to_string(),
            SacState::Evaluation => "处于评估模式，仍会拦截未知信誉应用。\
                请在 Windows 设置 → 隐私与安全 → 应用控制中关闭"
                .to_string(),
            SacState::Off => "未启用".to_string(),
            SacState::Unknown => "状态未知".to_string(),
        },
        recommended_action: None,
    });

    // ---- 待重启 ----
    let pending = detect_pending_reboot();
    out.push(DiagnosticItem {
        id: "prereq.pending_reboot".to_string(),
        category: "安装前置检查".to_string(),
        label: "系统待重启".to_string(),
        severity: if pending {
            Severity::Warn
        } else {
            Severity::Info
        },
        status: if pending {
            ItemStatus::Orphan
        } else {
            ItemStatus::Ok
        },
        detail: if pending {
            "系统存在挂起的重启请求（更新 / 驱动事务），先重启再装驱动可避免 install error"
                .to_string()
        } else {
            "无挂起的重启请求".to_string()
        },
        recommended_action: None,
    });

    // ---- Defender 白名单 ----
    // 不提供一键加白：调用 Add-MpPreference 容易被 Defender 自身的"行为分析"或第三方
    // 杀软拦截，反而把 FlairBloom 标红。这里只做检测和文字引导，让用户自己去
    // Windows 安全中心 → 病毒和威胁防护 → 排除项 添加。
    let install_dir = crate::commands::sysinfo::install_path();
    let exclusions = read_defender_exclusion_paths().await;
    let (severity, status, detail) = match &exclusions {
        None => (
            Severity::Warn,
            ItemStatus::Unknown,
            "无法读取 Defender 排除项（可能被组策略限制或 PowerShell 不可用）".to_string(),
        ),
        Some(list) => {
            if install_dir.is_empty() {
                (
                    Severity::Info,
                    ItemStatus::Unknown,
                    "无法定位安装目录，跳过白名单核对".to_string(),
                )
            } else if is_path_excluded(&install_dir, list) {
                (
                    Severity::Info,
                    ItemStatus::Ok,
                    format!("安装目录已在 Defender 排除列表：{install_dir}"),
                )
            } else {
                (
                    Severity::Warn,
                    ItemStatus::Missing,
                    format!(
                        "安装目录未加入 Defender 排除列表：{install_dir}\n\
                        若驱动安装屡次失败，可在 Windows 安全中心 → 病毒和威胁防护 → \
                        管理设置 → 排除项 中手动添加该目录"
                    ),
                )
            }
        }
    };
    out.push(DiagnosticItem {
        id: "prereq.defender_exclusion".to_string(),
        category: "安装前置检查".to_string(),
        label: "Windows Defender 白名单".to_string(),
        severity,
        status,
        detail,
        recommended_action: None,
    });

    out
}

/// 资源完整性自检（独立于 status.rs 的结果，避免它被缓存）。
#[cfg(windows)]
fn collect_install_resources(app: &AppHandle) -> (bool, Vec<String>) {
    let resources = match app.path().resource_dir() {
        Ok(d) => d.join("resources"),
        Err(_) => return (false, vec!["<resource_dir 不可达>".to_string()]),
    };
    let mut missing = Vec::new();
    let expected = [
        "install-interception.exe",
        "ddhid-driver/ddc.exe",
        "ddhid-driver/ddhid63340.sys",
        "ddhid-driver/ddhid63340.inf",
        "ddhid-driver/ddhid63340.cat",
    ];
    for rel in expected {
        if !resources.join(rel).exists() {
            missing.push(rel.to_string());
        }
    }
    (missing.is_empty(), missing)
}

#[cfg(windows)]
fn diagnose_dd_hid(_app: &AppHandle) -> Vec<DiagnosticItem> {
    let mut out = Vec::new();
    let sys_present = crate::commands::engine::dd_hid_sys_installed();
    let service_present = service_key_present("ddhid63340");
    let oem_inf = find_dd_hid_oem_inf();
    let driverstore = list_dd_hid_driverstore();

    out.push(DiagnosticItem {
        id: "dd_hid.sys".to_string(),
        category: "DD-HID 驱动".to_string(),
        label: "驱动文件 ddhid63340.sys".to_string(),
        severity: Severity::Info,
        status: if sys_present {
            ItemStatus::Ok
        } else {
            ItemStatus::Missing
        },
        detail: if sys_present {
            "已落盘".to_string()
        } else {
            "未安装或已被删除".to_string()
        },
        recommended_action: None,
    });

    out.push(DiagnosticItem {
        id: "dd_hid.service".to_string(),
        category: "DD-HID 驱动".to_string(),
        label: "服务键 ddhid63340".to_string(),
        severity: classify_residue_severity(service_present, sys_present),
        status: classify_residue_status(service_present, sys_present),
        detail: format!(
            "{}\nHKLM\\SYSTEM\\CurrentControlSet\\Services\\ddhid63340",
            residue_detail(service_present, sys_present, "服务键"),
        ),
        recommended_action: if service_present && !sys_present {
            Some("repair_dd_hid_residue".to_string())
        } else {
            None
        },
    });

    let oem_count = oem_inf.len();
    out.push(DiagnosticItem {
        id: "dd_hid.oem_inf".to_string(),
        category: "DD-HID 驱动".to_string(),
        label: "PnP 注册的 OEM INF".to_string(),
        severity: if oem_count > 0 && !sys_present {
            Severity::Error
        } else if oem_count > 1 {
            Severity::Warn
        } else {
            Severity::Info
        },
        status: if oem_count == 0 {
            ItemStatus::Ok
        } else if !sys_present {
            ItemStatus::Orphan
        } else {
            ItemStatus::Ok
        },
        detail: if oem_count == 0 {
            "无残留".to_string()
        } else {
            format!("发现 {oem_count} 项: {}", oem_inf.join(", "))
        },
        recommended_action: if oem_count > 0 && !sys_present {
            Some("repair_dd_hid_residue".to_string())
        } else {
            None
        },
    });

    out.push(DiagnosticItem {
        id: "dd_hid.driverstore".to_string(),
        category: "DD-HID 驱动".to_string(),
        label: "Driver Store 副本".to_string(),
        severity: if !driverstore.is_empty() && !sys_present {
            Severity::Warn
        } else {
            Severity::Info
        },
        status: if driverstore.is_empty() {
            ItemStatus::Ok
        } else if !sys_present {
            ItemStatus::Orphan
        } else {
            ItemStatus::Ok
        },
        detail: if driverstore.is_empty() {
            "无残留".to_string()
        } else {
            format!("发现 {} 项目录副本", driverstore.len())
        },
        recommended_action: if !driverstore.is_empty() && !sys_present {
            Some("repair_dd_hid_residue".to_string())
        } else {
            None
        },
    });

    out
}

#[cfg(windows)]
fn diagnose_interception(_app: &AppHandle) -> Vec<DiagnosticItem> {
    let mut out = Vec::new();
    let api_ok = crate::engine::interception::is_driver_installed();
    // 仅当服务键 ImagePath 指向 Interception 的 keyboard.sys / mouse.sys 才视为残留，
    // 避免把同名第三方服务误判
    let kbd = is_interception_service("keyboard", "keyboard.sys");
    let mouse = is_interception_service("mouse", "mouse.sys");
    let kbd_present_raw = service_key_present("keyboard");
    let mouse_present_raw = service_key_present("mouse");
    let foreign_kbd = kbd_present_raw && !kbd;
    let foreign_mouse = mouse_present_raw && !mouse;

    out.push(DiagnosticItem {
        id: "interception.runtime".to_string(),
        category: "Interception 驱动".to_string(),
        label: "运行时可用性".to_string(),
        severity: Severity::Info,
        status: if api_ok {
            ItemStatus::Ok
        } else {
            ItemStatus::Missing
        },
        detail: if api_ok {
            "create_context 成功".to_string()
        } else {
            "create_context 返回 null（驱动未装或被禁用）".to_string()
        },
        recommended_action: None,
    });

    let mut detail = format!("keyboard: {} / mouse: {}", yes_no(kbd), yes_no(mouse),);
    if foreign_kbd || foreign_mouse {
        detail.push_str("（检测到同名但非 Interception 的服务键，已跳过，不会清理）");
    }

    out.push(DiagnosticItem {
        id: "interception.services".to_string(),
        category: "Interception 驱动".to_string(),
        label: "keyboard / mouse 服务键".to_string(),
        severity: if (kbd || mouse) && !api_ok {
            Severity::Error
        } else {
            Severity::Info
        },
        status: match (kbd || mouse, api_ok) {
            (true, true) => ItemStatus::Ok,
            (true, false) => ItemStatus::Orphan,
            (false, _) => ItemStatus::Missing,
        },
        detail,
        recommended_action: if (kbd || mouse) && !api_ok {
            Some("repair_interception_residue".to_string())
        } else {
            None
        },
    });

    out
}

fn diagnose_profiles(app: &AppHandle) -> Vec<DiagnosticItem> {
    let mut out = Vec::new();
    let profiles_dir = match app.path().app_data_dir() {
        Ok(d) => d.join("profiles"),
        Err(_) => return out,
    };
    if !profiles_dir.exists() {
        return out;
    }
    let mut total = 0usize;
    let mut corrupted = Vec::new();
    let entries = match std::fs::read_dir(&profiles_dir) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("qzh") {
            continue;
        }
        total += 1;
        if !is_profile_readable(&path) {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                corrupted.push(name.to_string());
            }
        }
    }
    out.push(DiagnosticItem {
        id: "profiles.health".to_string(),
        category: "配置文件".to_string(),
        label: "已存配置完整性".to_string(),
        severity: if corrupted.is_empty() {
            Severity::Info
        } else {
            Severity::Warn
        },
        status: if corrupted.is_empty() {
            ItemStatus::Ok
        } else {
            ItemStatus::Corrupted
        },
        detail: if corrupted.is_empty() {
            format!("{total} 份配置全部可解密")
        } else {
            format!(
                "{} / {} 份损坏: {}",
                corrupted.len(),
                total,
                corrupted.join(", ")
            )
        },
        recommended_action: if corrupted.is_empty() {
            None
        } else {
            Some("repair_corrupted_profiles".to_string())
        },
    });
    out
}

fn diagnose_logs(_app: &AppHandle) -> Vec<DiagnosticItem> {
    let dir = crate::log_dir();
    let mut count = 0usize;
    let mut bytes = 0u64;
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    count += 1;
                    bytes += meta.len();
                }
            }
        }
    }
    let mb = bytes as f64 / 1024.0 / 1024.0;
    vec![DiagnosticItem {
        id: "logs.size".to_string(),
        category: "日志".to_string(),
        label: "本地日志文件".to_string(),
        severity: if mb > 50.0 {
            Severity::Warn
        } else {
            Severity::Info
        },
        status: ItemStatus::Ok,
        detail: format!("{count} 份, 共 {mb:.1} MB"),
        recommended_action: if mb > 50.0 {
            Some("repair_clean_logs".to_string())
        } else {
            None
        },
    }]
}

// ===== 诊断辅助：纯逻辑分类（可单测）=====

/// 服务键 / OEM INF 这类"PnP 残留"的 severity 判定。
///
/// 残留本身在驱动正常工作时是允许的；只有 sys 文件已经消失却仍留有登记
/// 时才会真正阻塞重装，所以这种状态升级为 Error。
pub(crate) fn classify_residue_severity(residue_present: bool, sys_present: bool) -> Severity {
    match (residue_present, sys_present) {
        (true, false) => Severity::Error,
        (true, true) => Severity::Info,
        (false, _) => Severity::Info,
    }
}

pub(crate) fn classify_residue_status(residue_present: bool, sys_present: bool) -> ItemStatus {
    match (residue_present, sys_present) {
        (true, false) => ItemStatus::Orphan,
        (true, true) => ItemStatus::Ok,
        (false, _) => ItemStatus::Ok,
    }
}

fn residue_detail(residue_present: bool, sys_present: bool, label: &str) -> String {
    match (residue_present, sys_present) {
        (true, false) => format!("{label}存在但驱动文件已缺失（半卸载状态，会阻塞重装）"),
        (true, true) => format!("{label}存在且驱动文件正常"),
        (false, _) => format!("{label}不存在"),
    }
}

fn yes_no(v: bool) -> &'static str {
    if v {
        "存在"
    } else {
        "不存在"
    }
}

fn is_profile_readable(path: &std::path::Path) -> bool {
    let Ok(data) = std::fs::read(path) else {
        return false;
    };
    let Some(header) = qzh_format::header::FileHeader::from_bytes(&data) else {
        return false;
    };
    if data.len() <= qzh_format::header::FileHeader::SIZE {
        return false;
    }
    let aad = header.aad();
    let ciphertext = &data[qzh_format::header::FileHeader::SIZE..];
    crypto::aes::decrypt(ciphertext, &header.nonce, &aad).is_ok()
}

// ===== Windows 注册表辅助 =====

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
pub(crate) fn service_key_present(name: &str) -> bool {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ,
    };
    let path = format!("SYSTEM\\CurrentControlSet\\Services\\{name}");
    let wpath = wide(&path);
    let mut hkey: HKEY = std::ptr::null_mut();
    // SAFETY: wpath NUL 结尾；hkey 是栈上出参指针
    let r = unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, wpath.as_ptr(), 0, KEY_READ, &mut hkey) };
    if r != 0 {
        return false;
    }
    // SAFETY: 上面 RegOpenKeyExW 成功
    unsafe { RegCloseKey(hkey) };
    true
}

/// 读取 HKLM 下指定子键的 DWORD 值。读不到（键不存在 / 值不存在 / 类型不符）返回 None。
///
/// 用于 HVCI / Smart App Control / Pending Reboot 这类只看一个 DWORD 标志位的检测。
#[cfg(windows)]
fn read_hklm_dword(subkey: &str, name: &str) -> Option<u32> {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ, REG_DWORD,
    };
    let wpath = wide(subkey);
    let mut hkey: HKEY = std::ptr::null_mut();
    // SAFETY: wpath NUL 结尾；hkey 是栈上出参指针
    let r = unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, wpath.as_ptr(), 0, KEY_READ, &mut hkey) };
    if r != 0 {
        return None;
    }
    let wname = wide(name);
    let mut ty: u32 = 0;
    let mut data: u32 = 0;
    let mut len: u32 = std::mem::size_of::<u32>() as u32;
    // SAFETY: hkey 已 open；data 是栈上 u32 与 len 匹配
    let q = unsafe {
        RegQueryValueExW(
            hkey,
            wname.as_ptr(),
            std::ptr::null_mut(),
            &mut ty,
            &mut data as *mut u32 as *mut u8,
            &mut len,
        )
    };
    // SAFETY: hkey 上面 open 成功
    unsafe { RegCloseKey(hkey) };
    if q == 0 && ty == REG_DWORD {
        Some(data)
    } else {
        None
    }
}

/// HKLM 子键是否存在（仅判存，不关心内容）。
#[cfg(windows)]
fn hklm_subkey_present(subkey: &str) -> bool {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ,
    };
    let wpath = wide(subkey);
    let mut hkey: HKEY = std::ptr::null_mut();
    // SAFETY: wpath NUL 结尾；hkey 是栈上出参指针
    let r = unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, wpath.as_ptr(), 0, KEY_READ, &mut hkey) };
    if r != 0 {
        return false;
    }
    // SAFETY: 上面 RegOpenKeyExW 成功
    unsafe { RegCloseKey(hkey) };
    true
}

/// 读取服务键 `ImagePath`（REG_SZ / REG_EXPAND_SZ），返回小写形式。
///
/// 用于在删除 keyboard / mouse 这种通用名服务前校验它确实是 Interception
/// 注册的——Interception 的硬编码 ImagePath 永远以 `\keyboard.sys` / `\mouse.sys`
/// 结尾。
#[cfg(windows)]
fn read_service_image_path(name: &str) -> Option<String> {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ,
    };
    let path = format!("SYSTEM\\CurrentControlSet\\Services\\{name}");
    let wpath = wide(&path);
    let mut hkey: HKEY = std::ptr::null_mut();
    // SAFETY: wpath NUL 结尾；hkey 是栈上出参指针
    let r = unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, wpath.as_ptr(), 0, KEY_READ, &mut hkey) };
    if r != 0 {
        return None;
    }
    let value_name = wide("ImagePath");
    let mut buf: [u16; 1024] = [0; 1024];
    let mut size: u32 = (buf.len() * 2) as u32;
    let mut ty: u32 = 0;
    // SAFETY: hkey 已 open，buf/size/ty 都是栈上出参；buf 容量足以容纳常规 ImagePath
    let q = unsafe {
        RegQueryValueExW(
            hkey,
            value_name.as_ptr(),
            std::ptr::null_mut(),
            &mut ty,
            buf.as_mut_ptr() as *mut u8,
            &mut size,
        )
    };
    // SAFETY: hkey 上面 open 成功
    unsafe { RegCloseKey(hkey) };
    if q != 0 {
        return None;
    }
    let chars = (size as usize).saturating_div(2);
    // 去掉末尾 NUL
    let trimmed: Vec<u16> = buf
        .iter()
        .take(chars)
        .copied()
        .take_while(|&c| c != 0)
        .collect();
    Some(String::from_utf16_lossy(&trimmed).to_lowercase())
}

/// 服务键名 + 期望的驱动文件名后缀（如 `keyboard.sys`），同时满足才视为 Interception 服务。
#[cfg(windows)]
pub(crate) fn is_interception_service(name: &str, expected_sys: &str) -> bool {
    if !service_key_present(name) {
        return false;
    }
    match read_service_image_path(name) {
        Some(p) => {
            // ImagePath 形如 `\??\C:\Windows\system32\drivers\keyboard.sys`，
            // 兼容大小写差异及驱动安装器使用的几种路径变体
            let needle = format!("\\{expected_sys}");
            p.ends_with(&needle) || p.ends_with(expected_sys)
        }
        None => false,
    }
}

/// 列出 PnP OEM INF 中匹配 ddhid 的 oem 编号（如 `["oem15.inf"]`）。
///
/// 用 `pnputil /enum-drivers` 取数据。pnputil 的输出是本地化文本，所以这里
/// 不解析它而是直接扫 `%SystemRoot%\INF\` 下所有 `oem*.inf` 文件，看 INF 内容
/// 是否含 ddhid63340 关键字——这是它唯一稳定且与语言无关的标识。
#[cfg(windows)]
fn find_dd_hid_oem_inf() -> Vec<String> {
    let inf_dir = std::env::var("SystemRoot")
        .map(|r| std::path::Path::new(&r).join("INF"))
        .unwrap_or_else(|_| std::path::PathBuf::from("C:\\Windows\\INF"));
    let entries = match std::fs::read_dir(&inf_dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy().to_lowercase();
        if !name_str.starts_with("oem") || !name_str.ends_with(".inf") {
            continue;
        }
        let path = entry.path();
        // INF 文件较小（通常 < 16 KB），全量读后做大小写无关匹配
        let Ok(content) = std::fs::read(&path) else {
            continue;
        };
        // INF 多为 UTF-16 LE 或 ANSI；这里两路都试一遍
        let utf8 = String::from_utf8_lossy(&content).to_lowercase();
        let utf16 = if content.len() >= 2 && content[0] == 0xFF && content[1] == 0xFE {
            let u16s: Vec<u16> = content[2..]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16_lossy(&u16s).to_lowercase()
        } else {
            String::new()
        };
        if utf8.contains("ddhid63340") || utf16.contains("ddhid63340") {
            out.push(name_str);
        }
    }
    out.sort();
    out
}

/// 扫描 `%SystemRoot%\System32\DriverStore\FileRepository\` 下所有
/// `ddhid*.inf_amd64_*` 目录。重装失败的另一个常见原因。
#[cfg(windows)]
fn list_dd_hid_driverstore() -> Vec<String> {
    let base = std::env::var("SystemRoot")
        .map(|r| {
            std::path::Path::new(&r)
                .join("System32")
                .join("DriverStore")
                .join("FileRepository")
        })
        .unwrap_or_else(|_| {
            std::path::PathBuf::from("C:\\Windows\\System32\\DriverStore\\FileRepository")
        });
    let entries = match std::fs::read_dir(&base) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if name.starts_with("ddhid") {
            out.push(name);
        }
    }
    out.sort();
    out
}

// ===== 修复：DD-HID 深度清理 =====

#[tauri::command]
pub async fn repair_dd_hid_residue(app: AppHandle) -> Result<RepairOutcome, String> {
    #[cfg(windows)]
    {
        run_dd_hid_repair(app).await
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err("仅 Windows 平台支持驱动修复".to_string())
    }
}

#[cfg(windows)]
async fn run_dd_hid_repair(app: AppHandle) -> Result<RepairOutcome, String> {
    use crate::engine::input::{init_backend, InputMode};
    use tauri_plugin_store::StoreExt;

    // 修复前先切回 SendInput，避免修复进行时 DLL 仍持有 sys 句柄
    init_backend(InputMode::SendInput);
    if let Ok(store) = app.store(crate::STORE_PATH) {
        store.set("input_mode", serde_json::json!("sendinput"));
        let _ = store.save();
    }

    let backup = ensure_backup_dir(&app)?;
    let oem_inf = find_dd_hid_oem_inf();
    let driverstore = list_dd_hid_driverstore();
    let service_present_before = service_key_present("ddhid63340");
    let sys_present_before = crate::commands::engine::dd_hid_sys_installed();

    let backup_lit = ps_single_quoted(&backup.display().to_string());
    let oem_inf_array = ps_string_array(&oem_inf);

    // 单脚本：备份 → pnputil 标准卸载 → 仅在 PnP 未清干净时移除注册表残留键。
    // 关键设计：不再 takeown / icacls / Remove-Item / PendingFileRenameOperations
    // 强夺 sys 文件——那种"半卸载"状态正是后续重装失败的根因。
    // PnP 子系统在 pnputil /delete-driver /uninstall /force 时会自己处理：
    //   停服务 → 释放设备实例 → 删 sys → 清 Driver Store → 移除 INF。
    // 即便 sys 被 TrustedInstaller 持有也由 PnP 提权完成，无需我们绕过 ACL。
    //
    // 退出码：
    //   0 = 全部完成
    //   2 = 某步骤失败（脚本不再使用 1 / PendingReboot 形式，pending_reboot 由
    //       Rust 侧根据"动了什么残留"来综合判定，更精确）
    let script = format!(
        "$ErrorActionPreference='Continue';\n\
         $backup={backup_lit};\n\
         $hardFail=$false;\n\
         function Backup-RegKey($path,$file){{\n\
             try {{ & reg.exe export $path (Join-Path $backup $file) /y | Out-Null }} catch {{ }}\n\
         }}\n\
         # 1. 备份服务键 + OEM INF + PNF\n\
         Backup-RegKey 'HKLM\\SYSTEM\\CurrentControlSet\\Services\\ddhid63340' 'service_ddhid63340.reg'\n\
         $oemInfList = {oem_inf_array}\n\
         foreach ($oem in $oemInfList) {{\n\
             $src = Join-Path $env:SystemRoot \"INF\\$oem\"\n\
             if (Test-Path -LiteralPath $src) {{\n\
                 try {{ Copy-Item -LiteralPath $src -Destination $backup -Force -ErrorAction Stop }}\n\
                 catch {{ }}\n\
                 $pnf = $src -replace '\\.inf$', '.PNF'\n\
                 if (Test-Path -LiteralPath $pnf) {{ try {{ Copy-Item -LiteralPath $pnf -Destination $backup -Force -ErrorAction Stop }} catch {{ }} }}\n\
             }}\n\
         }}\n\
         # 2. pnputil /delete-driver /uninstall /force（让 PnP 主导，不绕过）\n\
         foreach ($oem in $oemInfList) {{\n\
             try {{ & pnputil.exe /delete-driver $oem /uninstall /force | Out-Null }}\n\
             catch {{ $hardFail=$true }}\n\
             if ($LASTEXITCODE -ne 0) {{ $hardFail=$true }}\n\
         }}\n\
         # 3. pnputil 走完后若服务键仍在（罕见，常因 sys 已被手动删除导致 PnP\n\
         #    无法识别该 INF 归属的设备实例）→ 仅做注册表层面的兜底清理。\n\
         #    这一步只动注册表，不动文件系统、不动 ACL。\n\
         if (Test-Path 'HKLM:\\SYSTEM\\CurrentControlSet\\Services\\ddhid63340') {{\n\
             try {{ Remove-Item -LiteralPath 'HKLM:\\SYSTEM\\CurrentControlSet\\Services\\ddhid63340' -Recurse -Force -ErrorAction Stop }}\n\
             catch {{ $hardFail=$true }}\n\
         }}\n\
         if ($hardFail) {{ exit 2 }}\n\
         exit 0",
    );

    let exit = run_powershell_script_elevated(&app, &script).await;
    crate::commands::status::emit_status_changed(&app);

    let sys_present_after = crate::commands::engine::dd_hid_sys_installed();
    let service_present_after = service_key_present("ddhid63340");
    let oem_inf_after = find_dd_hid_oem_inf();
    let driverstore_after = list_dd_hid_driverstore();

    let mut steps = Vec::new();
    steps.push(make_step(
        "备份注册表与 INF",
        &backup,
        if backup.exists() {
            StepStatus::Ok
        } else {
            StepStatus::Failed
        },
    ));
    steps.push(removal_step(
        "卸载 OEM INF（pnputil）",
        oem_inf.len(),
        oem_inf_after.len(),
        |before, after| {
            if before == 0 {
                "无 OEM INF 需卸载".to_string()
            } else if after == 0 {
                format!("已卸载 {before} 项")
            } else {
                format!("仍残留 {after} 项 (原 {before} 项)")
            }
        },
    ));
    steps.push(boolean_removal_step(
        "清理服务键",
        service_present_before,
        service_present_after,
        "服务键 ddhid63340",
    ));
    steps.push(removal_step(
        "清理 Driver Store 副本",
        driverstore.len(),
        driverstore_after.len(),
        |before, after| {
            if before == 0 {
                "无 Driver Store 副本".to_string()
            } else if after == 0 {
                format!("已清理 {before} 项目录")
            } else {
                format!("仍残留 {after} 项 (原 {before} 项)")
            }
        },
    ));
    steps.push(pnp_sys_step(sys_present_before, sys_present_after));

    let mut success = matches!(exit, Ok(0));
    // 物理事实优先：即便脚本声称成功，只要服务键 / OEM INF 还在就是失败
    if service_present_after || !oem_inf_after.is_empty() {
        success = false;
    }
    // 只要动过任何残留就建议重启：PnP 内部状态（设备实例缓存、SCM 内存副本）
    // 必须等下次开机才能彻底刷新，立即重装大概率撞 install error
    let touched_anything = oem_inf.len() != oem_inf_after.len()
        || service_present_before != service_present_after
        || driverstore.len() != driverstore_after.len()
        || sys_present_before != sys_present_after;
    let pending_reboot = success && touched_anything;

    let summary = match (&exit, success, pending_reboot) {
        (_, true, true) => "残留已清理，请重启电脑后再尝试安装驱动".to_string(),
        (_, true, false) => "未发现 DD-HID 残留".to_string(),
        (Err(e), false, _) => format!("修复中断: {e}"),
        (_, false, _) => "修复部分完成，仍存在残留，建议重启后重试".to_string(),
    };

    Ok(RepairOutcome {
        success,
        pending_reboot,
        summary,
        steps,
        backup_dir: Some(backup.display().to_string()),
    })
}

// ===== 修复：Interception 残留 =====

#[tauri::command]
pub async fn repair_interception_residue(app: AppHandle) -> Result<RepairOutcome, String> {
    #[cfg(windows)]
    {
        run_interception_repair(app).await
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err("仅 Windows 平台支持驱动修复".to_string())
    }
}

#[cfg(windows)]
async fn run_interception_repair(app: AppHandle) -> Result<RepairOutcome, String> {
    let backup = ensure_backup_dir(&app)?;
    // 严格识别：仅当服务键 ImagePath 指向 Interception 自带的 keyboard.sys / mouse.sys
    // 才允许清理，绝不动同名的第三方服务
    let kbd_before = is_interception_service("keyboard", "keyboard.sys");
    let mouse_before = is_interception_service("mouse", "mouse.sys");
    let kbd_raw = service_key_present("keyboard");
    let mouse_raw = service_key_present("mouse");
    let api_before = crate::engine::interception::is_driver_installed();

    // 仅在驱动 API 不可用、但服务键残留时才动手清理；其它情况保守跳过
    if api_before {
        return Ok(RepairOutcome {
            success: true,
            pending_reboot: false,
            summary: "Interception 驱动运行正常，无需修复".to_string(),
            steps: vec![RepairStep {
                name: "检测驱动状态".to_string(),
                status: StepStatus::Skipped,
                detail: "create_context 成功，驱动正常工作".to_string(),
            }],
            backup_dir: None,
        });
    }
    if !kbd_before && !mouse_before {
        let detail = if kbd_raw || mouse_raw {
            "存在同名服务键但 ImagePath 不指向 Interception，已跳过以保护第三方驱动".to_string()
        } else {
            "keyboard / mouse 服务键均不存在".to_string()
        };
        return Ok(RepairOutcome {
            success: true,
            pending_reboot: false,
            summary: "未发现 Interception 服务键残留".to_string(),
            steps: vec![RepairStep {
                name: "扫描服务键".to_string(),
                status: StepStatus::Skipped,
                detail,
            }],
            backup_dir: None,
        });
    }

    let backup_lit = ps_single_quoted(&backup.display().to_string());
    // 注意：Windows 自带的 `kbdclass` / `mouclass` 服务名是别的；
    // Interception 注册的是 `keyboard` / `mouse`（小写、无后缀），与系统服务并存。
    // 这里只删确认是 Interception 的 key，由 PowerShell 脚本再次校验 ImagePath
    // 兜底，确保即便诊断与执行之间发生变化也不会误删。
    let targets: Vec<String> = [("keyboard", kbd_before), ("mouse", mouse_before)]
        .iter()
        .filter_map(|(n, ok)| if *ok { Some((*n).to_string()) } else { None })
        .collect();
    let target_array = ps_string_array(&targets);
    let script = format!(
        "$ErrorActionPreference='Continue';\n\
         $backup={backup_lit};\n\
         $hardFail=$false;\n\
         foreach ($svc in {target_array}) {{\n\
             $regHive='HKLM:\\SYSTEM\\CurrentControlSet\\Services\\' + $svc\n\
             $regExport='HKLM\\SYSTEM\\CurrentControlSet\\Services\\' + $svc\n\
             if (-not (Test-Path $regHive)) {{ continue }}\n\
             # 二次校验 ImagePath，避免删错\n\
             $img=(Get-ItemProperty -Path $regHive -Name 'ImagePath' -ErrorAction SilentlyContinue).ImagePath\n\
             if (-not $img) {{ continue }}\n\
             $expected = $svc + '.sys'\n\
             if (-not ($img.ToLower().EndsWith('\\' + $expected) -or $img.ToLower().EndsWith($expected))) {{ continue }}\n\
             try {{ & reg.exe export $regExport (Join-Path $backup ('service_' + $svc + '.reg')) /y | Out-Null }} catch {{ }}\n\
             try {{ Remove-Item -LiteralPath $regHive -Recurse -Force -ErrorAction Stop }}\n\
             catch {{ $hardFail=$true }}\n\
         }}\n\
         if ($hardFail) {{ exit 2 }}\n\
         exit 0",
    );

    let exit = run_powershell_script_elevated(&app, &script).await;
    crate::commands::status::emit_status_changed(&app);

    let kbd_after = is_interception_service("keyboard", "keyboard.sys");
    let mouse_after = is_interception_service("mouse", "mouse.sys");

    let steps = vec![
        make_step(
            "备份服务键",
            &backup,
            if backup.exists() {
                StepStatus::Ok
            } else {
                StepStatus::Failed
            },
        ),
        boolean_removal_step(
            "删除 keyboard 服务键",
            kbd_before,
            kbd_after,
            "keyboard 服务键",
        ),
        boolean_removal_step(
            "删除 mouse 服务键",
            mouse_before,
            mouse_after,
            "mouse 服务键",
        ),
    ];

    let success = !kbd_after && !mouse_after && matches!(exit, Ok(0));
    let summary = if success {
        "Interception 残留已清理，请重启电脑后再尝试安装".to_string()
    } else {
        match exit {
            Err(e) => format!("修复中断: {e}"),
            _ => "修复部分完成，仍存在残留".to_string(),
        }
    };

    Ok(RepairOutcome {
        success,
        pending_reboot: success, // 删服务键必须重启才能重新加载 Interception
        summary,
        steps,
        backup_dir: Some(backup.display().to_string()),
    })
}

// ===== 修复：损坏的 .qzh 配置 =====

#[tauri::command]
pub async fn repair_corrupted_profiles(app: AppHandle) -> Result<RepairOutcome, String> {
    let profiles_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("无法获取应用数据目录: {e}"))?
        .join("profiles");
    if !profiles_dir.exists() {
        return Ok(RepairOutcome {
            success: true,
            pending_reboot: false,
            summary: "未发现配置目录".to_string(),
            steps: Vec::new(),
            backup_dir: None,
        });
    }
    let corrupted_dir = profiles_dir.join("corrupted").join(timestamp_slug());

    let mut steps = Vec::new();
    let mut moved = 0usize;
    let entries = std::fs::read_dir(&profiles_dir).map_err(|e| format!("读取配置目录失败: {e}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("qzh") {
            continue;
        }
        if is_profile_readable(&path) {
            continue;
        }
        if moved == 0 {
            std::fs::create_dir_all(&corrupted_dir)
                .map_err(|e| format!("创建隔离目录失败: {e}"))?;
        }
        let name = path
            .file_name()
            .map(|n| n.to_owned())
            .unwrap_or_else(|| std::ffi::OsString::from("unknown.qzh"));
        let target = corrupted_dir.join(&name);
        match std::fs::rename(&path, &target) {
            Ok(()) => {
                moved += 1;
                steps.push(RepairStep {
                    name: format!("隔离 {}", name.to_string_lossy()),
                    status: StepStatus::Ok,
                    detail: target.display().to_string(),
                });
            }
            Err(e) => steps.push(RepairStep {
                name: format!("隔离 {}", name.to_string_lossy()),
                status: StepStatus::Failed,
                detail: format!("移动失败: {e}"),
            }),
        }
    }

    let failed = steps
        .iter()
        .filter(|s| s.status == StepStatus::Failed)
        .count();
    let success = failed == 0;
    // 失败优先：哪怕 moved == 0，只要有 rename 失败也要给出失败文案，
    // 不能再说"未发现损坏配置"——前端会同时收到 error toast，文案必须自洽
    let summary = if failed > 0 && moved == 0 {
        format!("发现损坏配置但全部移动失败（{failed} 份）")
    } else if failed > 0 {
        format!("已隔离 {moved} 份，但有 {failed} 份移动失败")
    } else if moved == 0 {
        "未发现损坏的配置".to_string()
    } else {
        format!("已隔离 {moved} 份损坏配置到 corrupted/ 目录")
    };

    Ok(RepairOutcome {
        success,
        pending_reboot: false,
        summary,
        steps,
        backup_dir: if moved > 0 {
            Some(corrupted_dir.display().to_string())
        } else {
            None
        },
    })
}

// ===== 修复：清理旧日志 =====

#[tauri::command]
pub async fn repair_clean_logs(_app: AppHandle) -> Result<RepairOutcome, String> {
    use std::time::{Duration, SystemTime};
    let dir = crate::log_dir();
    if !dir.exists() {
        return Ok(RepairOutcome {
            success: true,
            pending_reboot: false,
            summary: "无日志目录".to_string(),
            steps: Vec::new(),
            backup_dir: None,
        });
    }
    let cutoff = SystemTime::now() - Duration::from_secs(7 * 24 * 3600);
    let mut removed = 0usize;
    let mut bytes_freed = 0u64;
    let mut steps = Vec::new();
    for entry in std::fs::read_dir(&dir)
        .map_err(|e| format!("读取日志目录失败: {e}"))?
        .flatten()
    {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with("crash-") {
            continue; // 崩溃日志保留供反馈
        }
        let Ok(meta) = entry.metadata() else { continue };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        if modified < cutoff {
            let size = meta.len();
            if std::fs::remove_file(&path).is_ok() {
                removed += 1;
                bytes_freed += size;
            }
        }
    }
    steps.push(RepairStep {
        name: "清理 7 天前的日志".to_string(),
        status: StepStatus::Ok,
        detail: format!(
            "删除 {removed} 份, 释放 {:.1} MB",
            bytes_freed as f64 / 1024.0 / 1024.0
        ),
    });
    Ok(RepairOutcome {
        success: true,
        pending_reboot: false,
        summary: format!("已清理 {removed} 份旧日志"),
        steps,
        backup_dir: None,
    })
}

// ===== Step 构造辅助 =====

#[cfg(windows)]
fn make_step(name: &str, target: &std::path::Path, status: StepStatus) -> RepairStep {
    RepairStep {
        name: name.to_string(),
        status,
        detail: target.display().to_string(),
    }
}

#[cfg(windows)]
fn boolean_removal_step(name: &str, before: bool, after: bool, label: &str) -> RepairStep {
    let (status, detail) = match (before, after) {
        (false, _) => (StepStatus::Skipped, format!("{label}本就不存在")),
        (true, false) => (StepStatus::Ok, format!("{label}已删除")),
        (true, true) => (StepStatus::Failed, format!("{label}仍存在")),
    };
    RepairStep {
        name: name.to_string(),
        status,
        detail,
    }
}

#[cfg(windows)]
fn removal_step(
    name: &str,
    before: usize,
    after: usize,
    detail_fn: impl Fn(usize, usize) -> String,
) -> RepairStep {
    let status = if before == 0 {
        StepStatus::Skipped
    } else if after == 0 {
        StepStatus::Ok
    } else {
        // 残留数 > 0：无论比之前少多少都视为失败，没法保证下次安装走干净
        StepStatus::Failed
    };
    RepairStep {
        name: name.to_string(),
        status,
        detail: detail_fn(before, after),
    }
}

#[cfg(windows)]
fn pnp_sys_step(before: bool, after: bool) -> RepairStep {
    let (status, detail) = match (before, after) {
        (false, _) => (StepStatus::Skipped, "驱动文件本就不存在".to_string()),
        (true, false) => (StepStatus::Ok, "驱动文件已由 PnP 移除".to_string()),
        // pnputil 走完后 sys 仍在：通常是 PnP 还在异步释放设备实例，重启即可
        (true, true) => (
            StepStatus::PendingReboot,
            "驱动文件仍占用，将在重启后由 PnP 完成清理".to_string(),
        ),
    };
    RepairStep {
        name: "驱动文件 ddhid63340.sys".to_string(),
        status,
        detail,
    }
}

// ===== PowerShell 调用封装 =====

/// 把 [String] 编为 PowerShell 字面量数组：`@('a','b')`。
fn ps_string_array(items: &[String]) -> String {
    if items.is_empty() {
        return "@()".to_string();
    }
    let mut buf = String::from("@(");
    for (i, s) in items.iter().enumerate() {
        if i > 0 {
            buf.push(',');
        }
        buf.push_str(&ps_single_quoted(s));
    }
    buf.push(')');
    buf
}

/// 把字符串包成 PowerShell 单引号字面量，单引号转义为两个单引号。
///
/// 例如 `O'Brien\path` → `'O''Brien\path'`。Windows 用户名 / 路径里夹带的
/// 单引号不会再截断脚本。
fn ps_single_quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    out.push_str(&s.replace('\'', "''"));
    out.push('\'');
    out
}

/// 把脚本编码成 `-EncodedCommand` 形式并提权执行，返回真实退出码。
#[cfg(windows)]
async fn run_powershell_script_elevated(app: &AppHandle, script: &str) -> Result<u32, String> {
    let utf16: Vec<u16> = script.encode_utf16().collect();
    let bytes: Vec<u8> = utf16.iter().flat_map(|c| c.to_le_bytes()).collect();
    let encoded = base64_std_encode(&bytes);
    let arg =
        format!("-NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand {encoded}");
    let exe =
        std::path::PathBuf::from("C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe");
    crate::commands::engine::run_elevated_exe_capture(app.clone(), exe, Some(&arg)).await
}

/// 复用 engine.rs 的 base64 实现，避免新增依赖
#[cfg(windows)]
fn base64_std_encode(input: &[u8]) -> String {
    const TBL: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    let mut chunks = input.chunks_exact(3);
    for c in chunks.by_ref() {
        let n = ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | (c[2] as u32);
        out.push(TBL[((n >> 18) & 0x3F) as usize] as char);
        out.push(TBL[((n >> 12) & 0x3F) as usize] as char);
        out.push(TBL[((n >> 6) & 0x3F) as usize] as char);
        out.push(TBL[(n & 0x3F) as usize] as char);
    }
    let rem = chunks.remainder();
    match rem.len() {
        1 => {
            let n = (rem[0] as u32) << 16;
            out.push(TBL[((n >> 18) & 0x3F) as usize] as char);
            out.push(TBL[((n >> 12) & 0x3F) as usize] as char);
            out.push('=');
            out.push('=');
        }
        2 => {
            let n = ((rem[0] as u32) << 16) | ((rem[1] as u32) << 8);
            out.push(TBL[((n >> 18) & 0x3F) as usize] as char);
            out.push(TBL[((n >> 12) & 0x3F) as usize] as char);
            out.push(TBL[((n >> 6) & 0x3F) as usize] as char);
            out.push('=');
        }
        _ => {}
    }
    out
}

// ===== 单元测试：纯逻辑路径 =====

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn residue_severity_matrix() {
        // 残留 + sys 在 → 正常
        assert_eq!(classify_residue_severity(true, true), Severity::Info);
        // 残留 + sys 缺 → 阻塞重装，升级为 Error
        assert_eq!(classify_residue_severity(true, false), Severity::Error);
        // 无残留 → Info（无论 sys 是否在）
        assert_eq!(classify_residue_severity(false, true), Severity::Info);
        assert_eq!(classify_residue_severity(false, false), Severity::Info);
    }

    #[test]
    fn residue_status_matrix() {
        assert_eq!(classify_residue_status(true, true), ItemStatus::Ok);
        assert_eq!(classify_residue_status(true, false), ItemStatus::Orphan);
        assert_eq!(classify_residue_status(false, true), ItemStatus::Ok);
        assert_eq!(classify_residue_status(false, false), ItemStatus::Ok);
    }

    #[test]
    fn ps_string_array_escapes_quotes() {
        assert_eq!(ps_string_array(&[]), "@()");
        assert_eq!(
            ps_string_array(&["oem15.inf".to_string(), "oem99.inf".to_string()]),
            "@('oem15.inf','oem99.inf')"
        );
        // 单引号转义为两个单引号
        assert_eq!(ps_string_array(&["a'b".to_string()]), "@('a''b')");
    }

    #[test]
    fn ps_single_quoted_escapes_inner_quotes() {
        assert_eq!(ps_single_quoted("plain"), "'plain'");
        assert_eq!(ps_single_quoted(""), "''");
        // 路径中含单引号（如 Windows 用户名 O'Brien）必须被转义
        assert_eq!(
            ps_single_quoted("C:\\Users\\O'Brien\\Local"),
            "'C:\\Users\\O''Brien\\Local'"
        );
        // 多个单引号连续转义
        assert_eq!(ps_single_quoted("a''b"), "'a''''b'");
    }

    #[test]
    fn timestamp_slug_format() {
        let s = timestamp_slug();
        // 形如 20260527-153045，长度固定 15
        assert_eq!(s.len(), 15);
        assert!(s.chars().nth(8) == Some('-'));
        assert!(s.chars().filter(|c| c.is_ascii_digit()).count() == 14);
    }

    #[test]
    fn ymd_from_days_known_dates() {
        // 1970-01-01 → 0 days
        assert_eq!(ymd_from_days(0), (1970, 1, 1));
        // 2000-01-01 = 10957 days
        assert_eq!(ymd_from_days(10957), (2000, 1, 1));
        // 2026-05-27（约）：手动算，不要求和今天对齐到秒，只验算法
        // 2026-01-01 是 ymd_from_days(20454)；可以反推单日偏移
        let d_2026_01_01 = ymd_from_days(20454);
        assert_eq!(d_2026_01_01, (2026, 1, 1));
    }

    // ===== 安装前置检查 =====

    #[cfg(windows)]
    #[test]
    fn parse_exclusion_json_handles_common_shapes() {
        // 常规数组
        assert_eq!(
            parse_exclusion_json(r#"["C:\\Foo","C:\\Bar"]"#).unwrap(),
            vec!["C:\\Foo".to_string(), "C:\\Bar".to_string()]
        );
        // 空数组 / null / 空串
        assert_eq!(parse_exclusion_json("[]").unwrap(), Vec::<String>::new());
        assert_eq!(parse_exclusion_json("null").unwrap(), Vec::<String>::new());
        assert_eq!(parse_exclusion_json("").unwrap(), Vec::<String>::new());
        // PowerShell 单元素退化为裸字符串
        assert_eq!(
            parse_exclusion_json(r#""C:\\Only One""#).unwrap(),
            vec!["C:\\Only One".to_string()]
        );
        // 含转义引号
        assert_eq!(
            parse_exclusion_json(r#"["C:\\O\"B"]"#).unwrap(),
            vec!["C:\\O\"B".to_string()]
        );
        // 非法格式返回 None
        assert!(parse_exclusion_json("garbage").is_none());
    }

    #[test]
    fn is_path_excluded_matches_dir_and_parent() {
        // 完全等价（大小写无关 + 斜杠归一化）
        assert!(is_path_excluded(
            "C:\\Program Files\\FlairBloom",
            &["c:\\program files\\flairbloom".to_string()]
        ));
        // 父目录覆盖子目录
        assert!(is_path_excluded(
            "C:\\Program Files\\FlairBloom\\bin",
            &["C:\\Program Files\\FlairBloom".to_string()]
        ));
        // 子目录不覆盖父目录
        assert!(!is_path_excluded(
            "C:\\Program Files\\FlairBloom",
            &["C:\\Program Files\\FlairBloom\\bin".to_string()]
        ));
        // 前缀字符串相同但不是父目录
        assert!(!is_path_excluded(
            "C:\\Program Files\\FlairBloomCorp",
            &["C:\\Program Files\\FlairBloom".to_string()]
        ));
        // 排除项尾部带反斜杠也能匹配
        assert!(is_path_excluded("C:\\App", &["C:\\App\\".to_string()]));
        // 空目标 / 空排除项不会误判
        assert!(!is_path_excluded("", &["C:\\App".to_string()]));
        assert!(!is_path_excluded("C:\\App", &["".to_string()]));
        // 正反斜杠混用归一化
        assert!(is_path_excluded("C:/App/Bin", &["C:\\App".to_string()]));
    }

    #[test]
    fn classify_arch_compat_matrix() {
        let (ok, _) = classify_arch_compat("x64");
        assert!(ok);
        let (ok, _) = classify_arch_compat("AMD64");
        assert!(ok);
        let (ok, msg) = classify_arch_compat("ARM64");
        assert!(!ok);
        assert!(msg.contains("ARM64"));
        let (ok, _) = classify_arch_compat("x86");
        assert!(!ok);
        let (ok, _) = classify_arch_compat("unknown(99)");
        assert!(!ok);
    }
}
