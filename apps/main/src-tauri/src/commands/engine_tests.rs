use super::*;
use std::path::PathBuf;

#[test]
fn strip_verbatim_removes_drive_prefix() {
    let p = PathBuf::from(r"\\?\C:\Windows\System32");
    assert_eq!(strip_verbatim(p), PathBuf::from(r"C:\Windows\System32"));
}

#[test]
fn strip_verbatim_handles_lowercase_drive() {
    let p = PathBuf::from(r"\\?\d:\foo\bar");
    assert_eq!(strip_verbatim(p), PathBuf::from(r"d:\foo\bar"));
}

#[test]
fn strip_verbatim_converts_unc_back_to_double_slash() {
    let p = PathBuf::from(r"\\?\UNC\server\share\dir");
    assert_eq!(strip_verbatim(p), PathBuf::from(r"\\server\share\dir"));
}

#[test]
fn strip_verbatim_keeps_non_verbatim_unchanged() {
    let p = PathBuf::from(r"C:\Users\me");
    assert_eq!(strip_verbatim(p.clone()), p);
}

#[test]
fn strip_verbatim_keeps_unrecognized_verbatim_form() {
    // \\?\Volume{GUID}\... 类形式不是 drive,也不是 UNC,应保持原样
    let p = PathBuf::from(r"\\?\Volume{12345}\foo");
    assert_eq!(strip_verbatim(p.clone()), p);
}

#[test]
fn strip_verbatim_handles_normal_unc() {
    // \\server\share 已经是 UNC,无 verbatim 前缀,保持不变
    let p = PathBuf::from(r"\\server\share\file");
    assert_eq!(strip_verbatim(p.clone()), p);
}
