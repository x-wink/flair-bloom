//! `judge_install_result` / `judge_uninstall_result` 的判定逻辑测试。
//!
//! 这层抽象的目的：`ddc.exe` 的退出码不可信（交互式 `pause` 收尾会被用户按键污染），
//! 真正的判定标准是驱动 `.sys` 文件是否落盘 / 移除。这里把两种来源的信号交叉测一遍。

use super::*;

// ---- judge_install_result ----------------------------------------------

#[test]
fn install_success_when_sys_present_regardless_of_exit_ok() {
    assert_eq!(judge_install_result(true, Ok(())), Ok(()));
}

#[test]
fn install_success_when_sys_present_even_if_exit_failed() {
    // 这是 HID bug 的核心场景：ddc.exe 因 pause 报错，但驱动其实已落盘
    assert_eq!(
        judge_install_result(true, Err("退出码 1".to_string())),
        Ok(())
    );
}

#[test]
fn install_failure_uses_exe_error_when_sys_missing() {
    // 进程级失败信息保留下来，便于用户看出是 UAC 取消还是别的原因
    assert_eq!(
        judge_install_result(false, Err("已取消管理员授权".to_string())),
        Err("已取消管理员授权".to_string())
    );
}

#[test]
fn install_failure_synthesizes_message_when_sys_missing_but_exe_ok() {
    // exe 自报成功却没看到 .sys，说明驱动没真正生效
    assert_eq!(
        judge_install_result(false, Ok(())),
        Err("驱动安装未生效".to_string())
    );
}

// ---- judge_uninstall_result --------------------------------------------

#[test]
fn uninstall_success_when_sys_absent_regardless_of_exit_ok() {
    assert_eq!(judge_uninstall_result(false, Ok(())), Ok(()));
}

#[test]
fn uninstall_success_when_sys_absent_even_if_exit_failed() {
    // HID 卸载 bug 的核心场景
    assert_eq!(
        judge_uninstall_result(false, Err("退出码 1".to_string())),
        Ok(())
    );
}

#[test]
fn uninstall_failure_uses_exe_error_when_sys_still_present() {
    assert_eq!(
        judge_uninstall_result(true, Err("已取消管理员授权".to_string())),
        Err("已取消管理员授权".to_string())
    );
}

#[test]
fn uninstall_failure_synthesizes_message_when_sys_present_but_exe_ok() {
    assert_eq!(
        judge_uninstall_result(true, Ok(())),
        Err("驱动卸载未生效".to_string())
    );
}
