//! 「以管理员身份运行」的兼容性标志开关。
//!
//! Windows 把「右键 → 属性 → 以管理员身份运行此程序」记在
//! `HKCU\Software\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers` 下：
//! 值名是 exe 绝对路径，值是空格分隔的标志串，形如 `~ HIGHDPIAWARE RUNASADMIN`。
//! 写 HKCU 不需要管理员权限，开关即时生效，下次启动由系统弹 UAC。

/// Layers 键的 HKCU 子路径。
pub const LAYERS_SUBKEY: &str =
    "Software\\Microsoft\\Windows NT\\CurrentVersion\\AppCompatFlags\\Layers";

const RUN_AS_ADMIN: &str = "RUNASADMIN";

fn has_run_as_admin(value: &str) -> bool {
    value
        .split_whitespace()
        .any(|token| token.eq_ignore_ascii_case(RUN_AS_ADMIN))
}

/// 算出写回 Layers 的新值；`None` 表示该删掉这个值。
///
/// 只增删 `RUNASADMIN` 这一个标志，别人（兼容性助手、高 DPI 设置）写进来的标志原样保留——
/// 整串覆盖会把用户自己设的兼容性选项抹掉。开头的 `~` 是 Layers 的固定前缀，不是标志。
fn apply_layer(current: Option<&str>, enabled: bool) -> Option<String> {
    let mut flags: Vec<&str> = current
        .unwrap_or_default()
        .split_whitespace()
        .filter(|token| *token != "~")
        .collect();

    if enabled {
        if !flags
            .iter()
            .any(|token| token.eq_ignore_ascii_case(RUN_AS_ADMIN))
        {
            flags.push(RUN_AS_ADMIN);
        }
    } else {
        flags.retain(|token| !token.eq_ignore_ascii_case(RUN_AS_ADMIN));
    }

    if flags.is_empty() {
        None
    } else {
        Some(format!("~ {}", flags.join(" ")))
    }
}

/// 指定 exe 是否已标记为以管理员身份启动。
#[cfg(windows)]
pub fn is_enabled(exe: &str) -> bool {
    use crate::registry::{read_reg_sz_at, RegRoot};

    read_reg_sz_at(RegRoot::Hkcu, LAYERS_SUBKEY, exe).is_some_and(|value| has_run_as_admin(&value))
}

#[cfg(not(windows))]
pub fn is_enabled(_exe: &str) -> bool {
    false
}

/// 打开 / 关闭指定 exe 的管理员启动标志。
#[cfg(windows)]
pub fn set_enabled(exe: &str, enabled: bool) -> Result<(), String> {
    use crate::registry::{delete_reg_value, read_reg_sz_at, write_reg_sz, RegRoot};

    let current = read_reg_sz_at(RegRoot::Hkcu, LAYERS_SUBKEY, exe);
    match apply_layer(current.as_deref(), enabled) {
        Some(next) => write_reg_sz(RegRoot::Hkcu, LAYERS_SUBKEY, exe, &next),
        None => delete_reg_value(RegRoot::Hkcu, LAYERS_SUBKEY, exe),
    }
}

#[cfg(not(windows))]
pub fn set_enabled(_exe: &str, _enabled: bool) -> Result<(), String> {
    Err("仅 Windows 支持以管理员模式启动".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_flag_to_missing_value() {
        assert_eq!(apply_layer(None, true).as_deref(), Some("~ RUNASADMIN"));
    }

    #[test]
    fn keeps_other_flags_when_adding() {
        assert_eq!(
            apply_layer(Some("~ HIGHDPIAWARE"), true).as_deref(),
            Some("~ HIGHDPIAWARE RUNASADMIN")
        );
    }

    #[test]
    fn adding_twice_does_not_duplicate() {
        assert_eq!(
            apply_layer(Some("~ runasadmin"), true).as_deref(),
            Some("~ runasadmin")
        );
    }

    #[test]
    fn removing_last_flag_deletes_value() {
        assert_eq!(apply_layer(Some("~ RUNASADMIN"), false), None);
    }

    #[test]
    fn keeps_other_flags_when_removing() {
        assert_eq!(
            apply_layer(Some("~ HIGHDPIAWARE RUNASADMIN"), false).as_deref(),
            Some("~ HIGHDPIAWARE")
        );
    }

    #[test]
    fn detects_flag_case_insensitively() {
        assert!(has_run_as_admin("~ HIGHDPIAWARE RunAsAdmin"));
        assert!(!has_run_as_admin("~ HIGHDPIAWARE"));
        // 子串不算：别的标志里恰好含这几个字母不该被当成已开启
        assert!(!has_run_as_admin("~ NOTRUNASADMINX"));
    }
}
