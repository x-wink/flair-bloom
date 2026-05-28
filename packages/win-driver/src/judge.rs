//! 驱动安装/卸载结果裁决：以物理文件 + 服务键作为最终判据，而非 ddc.exe 退出码。

/// 裁决驱动安装结果。
///
/// `ddc.exe` 在交互式 cmd 中收尾会 `pause`，用户按键后退出码不可信；
/// 必须以驱动 `.sys` 落盘 **并且** 服务键存在为最终判据。
pub fn judge_install_result(
    sys_installed: bool,
    service_present: bool,
    exe_result: Result<(), String>,
) -> Result<(), String> {
    if sys_installed && service_present {
        return Ok(());
    }
    if sys_installed && !service_present {
        return Err("检测到上次卸载留下的驱动残留尚未清理，本次安装未生效。\n\
             请重启电脑让 PnP 完成清理后再尝试安装。"
            .to_string());
    }
    Err(match exe_result {
        Ok(()) => "驱动安装未生效".to_string(),
        Err(e) => e,
    })
}

/// 裁决驱动卸载结果：以驱动文件是否被移除作为卸载成功的最终标志。
pub fn judge_uninstall_result(
    sys_installed: bool,
    exe_result: Result<(), String>,
) -> Result<(), String> {
    if !sys_installed {
        Ok(())
    } else {
        Err(match exe_result {
            Ok(()) => "驱动卸载未生效".to_string(),
            Err(e) => e,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_success_when_sys_present_regardless_of_exit_ok() {
        assert_eq!(judge_install_result(true, true, Ok(())), Ok(()));
    }

    #[test]
    fn install_success_when_sys_present_even_if_exit_failed() {
        assert_eq!(
            judge_install_result(true, true, Err("退出码 1".to_string())),
            Ok(())
        );
    }

    #[test]
    fn install_fails_when_sys_absent() {
        assert!(judge_install_result(false, false, Ok(())).is_err());
    }

    #[test]
    fn install_fails_with_service_missing_even_if_sys_present() {
        // 半卸载残留
        let r = judge_install_result(true, false, Ok(()));
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("残留"));
    }

    #[test]
    fn uninstall_success_when_sys_absent() {
        assert_eq!(judge_uninstall_result(false, Ok(())), Ok(()));
        assert_eq!(
            judge_uninstall_result(false, Err("退出码 1".to_string())),
            Ok(())
        );
    }

    #[test]
    fn uninstall_fails_when_sys_still_present() {
        assert!(judge_uninstall_result(true, Ok(())).is_err());
        assert!(judge_uninstall_result(true, Err("err".to_string())).is_err());
    }
}
