//! 提权相关：检测当前进程是否以管理员运行，以及通过 ShellExecuteExW 提权启动。

/// 当前进程是否以管理员权限运行。
#[cfg(windows)]
pub fn is_process_elevated() -> bool {
    use std::mem;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    const TOKEN_QUERY: u32 = 0x0008;
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: GetCurrentProcess 返回伪句柄无需释放
    let ok = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) };
    if ok == 0 {
        return false;
    }
    let mut elev: TOKEN_ELEVATION = unsafe { mem::zeroed() };
    let mut ret_len: u32 = 0;
    // SAFETY: token 来自 OpenProcessToken 成功调用
    let got = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            &mut elev as *mut _ as *mut _,
            mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret_len,
        )
    };
    // SAFETY: token 是上面返回的有效句柄
    unsafe { CloseHandle(token) };
    got != 0 && elev.TokenIsElevated != 0
}

#[cfg(not(windows))]
pub fn is_process_elevated() -> bool {
    false
}

/// 以管理员（runas）方式启动可执行文件并等待退出，返回退出码。
///
/// 不需要 AppHandle — 原代码中 `let _ = app;` 实际上并未使用 AppHandle。
/// 异步运行，内部用 `tokio::task::spawn_blocking` 包裹阻塞的 WaitForSingleObject。
#[cfg(windows)]
pub async fn run_elevated_exe_capture(
    file_path: std::path::PathBuf,
    params: Option<&str>,
) -> Result<u32, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, WaitForSingleObject, INFINITE,
    };
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };

    if !file_path.exists() {
        return Err(format!("可执行文件不存在: {}", file_path.display()));
    }

    let path_wide: Vec<u16> = file_path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let verb: Vec<u16> = "runas\0".encode_utf16().collect();
    let params_wide: Vec<u16> = match params {
        Some(p) => format!("{p}\0").encode_utf16().collect(),
        None => vec![0u16],
    };
    let working_dir: Vec<u16> = file_path
        .parent()
        .ok_or_else(|| "无法获取所在目录".to_string())?
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    tokio::task::spawn_blocking(move || -> Result<u32, String> {
        // SAFETY: SHELLEXECUTEINFOW 是 POD，全 0 初始化合法
        let mut sei: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
        sei.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        sei.fMask = SEE_MASK_NOCLOSEPROCESS;
        sei.lpVerb = verb.as_ptr();
        sei.lpFile = path_wide.as_ptr();
        sei.lpParameters = params_wide.as_ptr();
        sei.lpDirectory = working_dir.as_ptr();
        sei.nShow = 1;

        // SAFETY: 所有指针字段所指向的 Vec 在闭包结束前都存活，且都是 NUL 结尾宽串
        let ok = unsafe { ShellExecuteExW(&mut sei) };
        if ok == 0 {
            // SAFETY: GetLastError 无参，任意线程任意时刻调用安全
            let err = unsafe { windows_sys::Win32::Foundation::GetLastError() };
            return if err == ERROR_CANCELLED {
                Err("已取消管理员授权".to_string())
            } else {
                Err(format!("启动程序失败 (Win32 错误码 {err})"))
            };
        }

        if sei.hProcess.is_null() {
            return Err("无法获取进程句柄".to_string());
        }

        // SAFETY: hProcess 是 ShellExecuteExW 在 SEE_MASK_NOCLOSEPROCESS 下返回的有效句柄
        let wait = unsafe { WaitForSingleObject(sei.hProcess, INFINITE) };
        if wait != WAIT_OBJECT_0 {
            unsafe { CloseHandle(sei.hProcess) };
            return Err("等待程序结束时出错".to_string());
        }

        let mut exit_code: u32 = 0;
        // SAFETY: hProcess 仍有效
        let got = unsafe { GetExitCodeProcess(sei.hProcess, &mut exit_code) };
        unsafe { CloseHandle(sei.hProcess) };

        if got == 0 {
            return Err("无法读取退出码".to_string());
        }
        Ok(exit_code)
    })
    .await
    .map_err(|e| format!("任务异常: {e}"))?
}

/// 与 [`run_elevated_exe_capture`] 同语义，但把非 0 退出码视为错误。
#[cfg(windows)]
pub async fn run_elevated_exe(
    file_path: std::path::PathBuf,
    params: Option<&str>,
) -> Result<(), String> {
    match run_elevated_exe_capture(file_path, params).await? {
        0 => Ok(()),
        n => Err(format!("程序返回错误码 {n}")),
    }
}
