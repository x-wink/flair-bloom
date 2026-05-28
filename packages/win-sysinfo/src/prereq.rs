//! Windows 安装前置检查：HVCI / SAC / PendingReboot / Defender 排除路径 / 架构兼容性。

#[cfg(windows)]
use crate::registry::{hklm_subkey_present, read_hklm_dword, wide};

/// HVCI / 内存完整性是否启用。
///
/// 两个都为 1 才是"已生效"；任何一个非 0 就要给用户提示——会拒绝加载非 HVCI 兼容的内核驱动。
#[cfg(windows)]
pub fn detect_hvci_active() -> Option<bool> {
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
#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SacState {
    /// SAC 关闭
    Off,
    /// 强制执行（阻止未知信誉程序）
    Enforce,
    /// 评估模式（不阻止但记录）
    Evaluation,
    /// 未知值
    Unknown,
}

/// 读取 Smart App Control 当前状态。
#[cfg(windows)]
pub fn detect_sac_state() -> SacState {
    match read_hklm_dword(
        "SYSTEM\\CurrentControlSet\\Control\\CI\\Policy",
        "VerifiedAndReputablePolicyState",
    ) {
        Some(0) => SacState::Off,
        Some(1) => SacState::Enforce,
        Some(2) => SacState::Evaluation,
        Some(_) => SacState::Unknown,
        None => SacState::Off,
    }
}

/// 系统是否有挂起的重启请求。
///
/// 任何一个常见标记命中即视为 pending：
/// - `PendingFileRenameOperations`（SessionManager）
/// - `RebootPending` 子键（CBS）
/// - `RebootRequired` 子键（WindowsUpdate）
#[cfg(windows)]
pub fn detect_pending_reboot() -> bool {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ,
    };
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
/// 返回 `None` 表示查询失败（PowerShell 不可用 / 进程被卡住等）。
#[cfg(windows)]
pub async fn read_defender_exclusion_paths() -> Option<Vec<String>> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    let script = "$ErrorActionPreference='Stop'; \
                  try { \
                      $p = Get-MpPreference -ErrorAction Stop; \
                      $arr = @($p.ExclusionPath); \
                      ConvertTo-Json -InputObject $arr -Compress -Depth 2 \
                  } catch { '[]' }";

    tokio::task::spawn_blocking(move || {
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

/// 简单 JSON 字符串数组解析：能容忍 `null` / `""` / 单字符串退化为非数组。
///
/// 为了不引入完整 JSON parser，只识别 `[..]` 内被双引号括起来的字段；
/// 非数组形式（PowerShell 单元素时输出裸字符串）直接当成单元素。
pub fn parse_exclusion_json(text: &str) -> Option<Vec<String>> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed == "null" {
        return Some(Vec::new());
    }
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
        i += 1;
    }
    Some(out)
}

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
/// 命中规则：忽略大小写 + 去尾斜杠后，target 等于某条排除项，
/// 或 target 以"排除项 + `\`"开头（排除项是父目录）。
pub fn is_path_excluded(target: &str, exclusions: &[String]) -> bool {
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

pub(crate) fn normalize_path(p: &str) -> String {
    let mut s = p.trim().replace('/', "\\").to_lowercase();
    while s.ends_with('\\') {
        s.pop();
    }
    s
}

/// 把架构与 sys 文件位宽匹配为友好状态。
///
/// 返回 (是否兼容, 详情文案)。
pub fn classify_arch_compat(arch: &str) -> (bool, String) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn parse_exclusion_json_handles_common_shapes() {
        assert_eq!(
            parse_exclusion_json(r#"["C:\\Foo","C:\\Bar"]"#).unwrap(),
            vec!["C:\\Foo".to_string(), "C:\\Bar".to_string()]
        );
        assert_eq!(parse_exclusion_json("[]").unwrap(), Vec::<String>::new());
        assert_eq!(parse_exclusion_json("null").unwrap(), Vec::<String>::new());
        assert_eq!(parse_exclusion_json("").unwrap(), Vec::<String>::new());
        assert_eq!(
            parse_exclusion_json(r#""C:\\Only One""#).unwrap(),
            vec!["C:\\Only One".to_string()]
        );
        assert_eq!(
            parse_exclusion_json(r#"["C:\\O\"B"]"#).unwrap(),
            vec!["C:\\O\"B".to_string()]
        );
        assert!(parse_exclusion_json("garbage").is_none());
    }

    #[test]
    fn is_path_excluded_matches_dir_and_parent() {
        assert!(is_path_excluded(
            "C:\\Program Files\\FlairBloom",
            &["c:\\program files\\flairbloom".to_string()]
        ));
        assert!(is_path_excluded(
            "C:\\Program Files\\FlairBloom\\bin",
            &["C:\\Program Files\\FlairBloom".to_string()]
        ));
        assert!(!is_path_excluded(
            "C:\\Program Files\\FlairBloom",
            &["C:\\Program Files\\FlairBloom\\bin".to_string()]
        ));
        assert!(!is_path_excluded(
            "C:\\Program Files\\FlairBloomCorp",
            &["C:\\Program Files\\FlairBloom".to_string()]
        ));
        assert!(is_path_excluded("C:\\App", &["C:\\App\\".to_string()]));
        assert!(!is_path_excluded("", &["C:\\App".to_string()]));
        assert!(!is_path_excluded("C:\\App", &["".to_string()]));
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
