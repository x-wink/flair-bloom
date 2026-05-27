use crate::engine::BurstEngine;
use qzh_format::profile::{BurstRule, MAX_RULES};
use std::sync::{atomic::Ordering, Arc};
#[allow(unused_imports)]
use tauri::{AppHandle, Manager, State};

pub struct EngineState(pub Arc<BurstEngine>);

#[tauri::command]
pub fn set_global_enabled(app: AppHandle, state: State<EngineState>, enabled: bool) {
    state.0.global_enabled.store(enabled, Ordering::SeqCst);
    if let Some(tray) = app.tray_by_id("main") {
        if let Ok(menu) = crate::tray::build_menu(&app, enabled) {
            let _ = tray.set_menu(Some(menu));
        }
    }
}

#[tauri::command]
pub fn get_global_enabled(state: State<EngineState>) -> bool {
    state.0.global_enabled.load(Ordering::SeqCst)
}

#[tauri::command]
pub fn set_rules(state: State<EngineState>, rules: Vec<BurstRule>) -> Result<(), String> {
    if rules.len() > MAX_RULES {
        return Err(format!("规则数量 {} 超过上限 {}", rules.len(), MAX_RULES));
    }
    for (i, rule) in rules.iter().enumerate() {
        if !(10..=10000).contains(&rule.interval_ms) {
            return Err(format!(
                "第 {} 条规则间隔 {}ms 超出范围 [10, 10000]",
                i + 1,
                rule.interval_ms
            ));
        }
    }

    // DD-HID 模式仅 Toggle 模式要求 target_key 与 trigger_key/stop_key 不同；
    // Hold 模式靠 input.rs 的注入事件队列识别 sim 事件，允许 trigger == target
    #[cfg(windows)]
    {
        let mode = crate::engine::input::current_mode();
        if mode.requires_distinct_target_for_toggle() {
            for rule in rules
                .iter()
                .filter(|r| r.enabled && matches!(r.mode, qzh_format::profile::BurstMode::Toggle))
            {
                if rule.target_key == rule.trigger_key {
                    return Err(format!(
                        "究极HID 模式下，切换连发规则「{}」的目标键不可与启动热键相同",
                        rule.id
                    ));
                }
                let stop = rule.stop_key.unwrap_or(rule.trigger_key);
                if rule.target_key == stop {
                    return Err(format!(
                        "究极HID 模式下，切换连发规则「{}」的目标键不可与停止热键相同",
                        rule.id
                    ));
                }
            }
        }
    }

    state.0.set_rules(rules);
    Ok(())
}

#[tauri::command]
pub fn get_rules(state: State<EngineState>) -> Vec<BurstRule> {
    state.0.get_rules()
}

#[tauri::command]
pub fn get_active_rules(state: State<EngineState>) -> Vec<String> {
    state.0.get_active_ids()
}

#[tauri::command]
pub fn get_input_mode() -> String {
    #[cfg(windows)]
    {
        crate::engine::input::current_mode().as_str().to_string()
    }
    #[cfg(not(windows))]
    {
        "sendinput".to_string()
    }
}

#[tauri::command]
pub fn set_input_mode(
    app: AppHandle,
    state: State<EngineState>,
    mode: String,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        use crate::engine::input::{init_backend, InputMode};
        use tauri_plugin_store::StoreExt;

        let input_mode =
            InputMode::from_str(&mode).ok_or_else(|| format!("未知输入模式: {}", mode))?;

        // 切到 DD-HID 时要求当前已是管理员，否则前端应先发起提权重启
        if input_mode.requires_admin() && !is_process_elevated() {
            return Err("究极HID 模式需要管理员权限，请先以管理员身份重启应用".to_string());
        }

        // 切到 DD-HID 前用新规则约束做静态校验：仅 Toggle 模式要求 target 与 trigger/stop 互异
        if input_mode.requires_distinct_target_for_toggle() {
            let rules = state.0.get_rules();
            for rule in rules
                .iter()
                .filter(|r| r.enabled && matches!(r.mode, qzh_format::profile::BurstMode::Toggle))
            {
                if rule.target_key == rule.trigger_key {
                    return Err(format!(
                        "切换失败：切换连发规则「{}」的目标键与启动热键相同。\n究极HID 模式下，切换连发的目标键不可与启动/停止热键相同。请修改后再切换。",
                        rule.id
                    ));
                }
                let stop = rule.stop_key.unwrap_or(rule.trigger_key);
                if rule.target_key == stop {
                    return Err(format!(
                        "切换失败：切换连发规则「{}」的目标键与停止热键相同。\n究极HID 模式下，切换连发的目标键不可与启动/停止热键相同。请修改后再切换。",
                        rule.id
                    ));
                }
            }
        }

        init_backend(input_mode);

        if let Ok(store) = app.store(crate::STORE_PATH) {
            store.set("input_mode", serde_json::json!(input_mode.as_str()));
            let _ = store.save();
        }
        crate::commands::status::emit_status_changed(&app);
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (app, state, mode);
        Err("仅 Windows 平台支持切换输入模式".to_string())
    }
}

#[tauri::command]
pub fn is_driver_installed() -> bool {
    #[cfg(windows)]
    {
        crate::engine::interception::is_driver_installed()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(windows)]
async fn run_elevated_exe(
    app: AppHandle,
    file_path: std::path::PathBuf,
    params: Option<&str>,
) -> Result<(), String> {
    match run_elevated_exe_capture(app, file_path, params).await? {
        0 => Ok(()),
        n => Err(format!("程序返回错误码 {}", n)),
    }
}

/// 与 [`run_elevated_exe`] 同语义，但返回真实退出码而非把非 0 视为错误。
/// PowerShell 脚本约定退出码 0/1/2 表示不同结果时使用。
#[cfg(windows)]
pub(crate) async fn run_elevated_exe_capture(
    app: AppHandle,
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

    let _ = app;
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
        Some(p) => format!("{}\0", p).encode_utf16().collect(),
        None => vec![0u16],
    };
    let working_dir: Vec<u16> = file_path
        .parent()
        .ok_or_else(|| "无法获取所在目录".to_string())?
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    tauri::async_runtime::spawn_blocking(move || -> Result<u32, String> {
        // SAFETY: SHELLEXECUTEINFOW 是 POD,全 0 初始化合法,后续逐字段填写
        let mut sei: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
        sei.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        sei.fMask = SEE_MASK_NOCLOSEPROCESS;
        sei.lpVerb = verb.as_ptr();
        sei.lpFile = path_wide.as_ptr();
        sei.lpParameters = params_wide.as_ptr();
        sei.lpDirectory = working_dir.as_ptr();
        sei.nShow = 1;

        // SAFETY: 所有指针字段所指向的 Vec 在闭包结束前都存活,且都是 NUL 结尾的宽串
        let ok = unsafe { ShellExecuteExW(&mut sei) };
        if ok == 0 {
            // SAFETY: GetLastError 无参,任意线程任意时刻调用安全
            let err = unsafe { windows_sys::Win32::Foundation::GetLastError() };
            return if err == ERROR_CANCELLED {
                Err("已取消管理员授权".to_string())
            } else {
                Err(format!("启动程序失败 (Win32 错误码 {})", err))
            };
        }

        if sei.hProcess.is_null() {
            return Err("无法获取进程句柄".to_string());
        }

        // SAFETY: hProcess 是上面 ShellExecuteExW 在 SEE_MASK_NOCLOSEPROCESS
        // 模式下返回的有效进程句柄,INFINITE 是合法等待时长
        let wait = unsafe { WaitForSingleObject(sei.hProcess, INFINITE) };
        if wait != WAIT_OBJECT_0 {
            // SAFETY: hProcess 仍是上面返回的有效句柄
            unsafe { CloseHandle(sei.hProcess) };
            return Err("等待程序结束时出错".to_string());
        }

        let mut exit_code: u32 = 0;
        // SAFETY: hProcess 仍有效;exit_code 是栈上 u32,&mut 在调用期间有效
        let got = unsafe { GetExitCodeProcess(sei.hProcess, &mut exit_code) };
        // SAFETY: hProcess 是上面返回的有效句柄,函数返回前 hProcess 不再被读
        unsafe { CloseHandle(sei.hProcess) };

        if got == 0 {
            return Err("无法读取退出码".to_string());
        }

        Ok(exit_code)
    })
    .await
    .map_err(|e| format!("任务异常: {}", e))?
}

#[cfg(windows)]
fn resource_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let raw = app
        .path()
        .resource_dir()
        .map_err(|e| format!("无法获取资源目录: {}", e))?
        .join("resources");
    // Tauri 的 resource_dir 在 Windows 上返回 \\?\<drive>:\... 形式的 verbatim 路径。
    // 直接用作 ShellExecuteEx 的 lpDirectory 时，被启动进程（如 ddc.exe）再 spawn cmd
    // 会因「UNC 路径不受支持」回退到 C:\Windows\，导致 INF/SYS 找不到、驱动安装失败。
    Ok(strip_verbatim(raw))
}

/// 去掉 Windows verbatim 路径前缀（`\\?\<drive>:\...` → `<drive>:\...`），
/// 保留真正的 UNC 路径（`\\?\UNC\server\share\...` → `\\server\share\...`）。
#[cfg(windows)]
fn strip_verbatim(path: std::path::PathBuf) -> std::path::PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        return std::path::PathBuf::from(format!(r"\\{}", rest));
    }
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        // 仅当剥离后形如 "<letter>:\..." 时认为是普通本地路径
        let bytes = rest.as_bytes();
        if bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes[2] == b'\\' || bytes[2] == b'/')
        {
            return std::path::PathBuf::from(rest);
        }
    }
    path
}

#[tauri::command]
pub async fn install_driver(app: AppHandle) -> Result<(), String> {
    #[cfg(windows)]
    {
        let exe = resource_dir(&app)?.join("install-interception.exe");
        let result = run_elevated_exe(app.clone(), exe, Some("/install")).await;
        if let Err(ref e) = result {
            tracing::error!("Interception 驱动安装失败：{e}");
        }
        crate::commands::status::emit_status_changed(&app);
        result
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err("仅 Windows 平台支持安装驱动".to_string())
    }
}

#[tauri::command]
pub async fn uninstall_driver(app: AppHandle) -> Result<(), String> {
    #[cfg(windows)]
    {
        use crate::engine::input::{init_backend, InputMode};
        use tauri_plugin_store::StoreExt;
        init_backend(InputMode::SendInput);
        if let Ok(store) = app.store(crate::STORE_PATH) {
            store.set("input_mode", serde_json::json!("sendinput"));
            let _ = store.save();
        }

        let exe = resource_dir(&app)?.join("install-interception.exe");
        let result = run_elevated_exe(app.clone(), exe, Some("/uninstall")).await;
        if let Err(ref e) = result {
            tracing::error!("Interception 驱动卸载失败：{e}");
        }
        crate::commands::status::emit_status_changed(&app);
        result
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err("仅 Windows 平台支持卸载驱动".to_string())
    }
}

// ===== DD-HID 驱动管理 =====

#[cfg(windows)]
pub(crate) fn dd_hid_sys_installed() -> bool {
    dd_hid_sys_path().exists()
}

/// `ddhid63340.sys` 的绝对路径（基于 `%SystemRoot%`）。
#[cfg(windows)]
fn dd_hid_sys_path() -> std::path::PathBuf {
    let sysroot = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    std::path::Path::new(&sysroot)
        .join("System32")
        .join("drivers")
        .join("ddhid63340.sys")
}

/// 兜底删除 `ddhid63340.sys` 的结果。
#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SysRemoveOutcome {
    /// 已从磁盘移除
    Removed,
    /// 文件被内核占用删不掉，但已通过 MoveFileEx 标记重启时删除
    PendingReboot,
}

/// 兜底删除 `ddhid63340.sys`：通过 elevated PowerShell 单脚本完成。
///
/// `ddc.exe` 安装驱动时 sys 落在 `%SystemRoot%\System32\drivers\` 下且所有者被
/// 设为 **TrustedInstaller**，即便已 elevated 的管理员也会拿到 ERROR_ACCESS_DENIED。
/// 因此必须先夺权再删，单独的 `MoveFileEx` 标记重启删除会被 SMSS 同样的 ACL 拦下。
///
/// 脚本流程：
/// 1. 文件不存在 → exit 0
/// 2. `takeown /F $p /A` 把所有权转给 Administrators 组
/// 3. `icacls $p /grant *S-1-5-32-544:(F)` 给 Administrators 完整控制
///    （用 SID 避开本地化名称差异）
/// 4. `Remove-Item` 删除，成功 → exit 0
/// 5. 仍删不掉（罕见，通常是 SetupAPI 残留锁）→ 写 `PendingFileRenameOperations`
///    多字符串标记重启清理（路径已经 takeown，SMSS 删除阶段不会再被 ACL 拦） → exit 1
/// 6. 注册表写失败 → exit 2
#[cfg(windows)]
async fn try_force_remove_dd_hid_sys(app: &AppHandle) -> Result<SysRemoveOutcome, String> {
    let sys = dd_hid_sys_path();
    if !sys.exists() {
        return Ok(SysRemoveOutcome::Removed);
    }

    let sys_lit = sys.display().to_string();
    // PendingFileRenameOperations 文档要求 NT 命名空间路径 (\??\<drive>:\...)
    let nt_path = format!("\\??\\{sys_lit}");
    let script = format!(
        "$ErrorActionPreference='SilentlyContinue';\n\
         $p='{sys_lit}';\n\
         if (-not (Test-Path -LiteralPath $p)) {{ exit 0 }}\n\
         & takeown.exe /F $p /A | Out-Null;\n\
         & icacls.exe $p /grant '*S-1-5-32-544:(F)' /C | Out-Null;\n\
         Remove-Item -LiteralPath $p -Force;\n\
         if (-not (Test-Path -LiteralPath $p)) {{ exit 0 }}\n\
         $k='HKLM:\\SYSTEM\\CurrentControlSet\\Control\\Session Manager';\n\
         $name='PendingFileRenameOperations';\n\
         $existing=(Get-ItemProperty -Path $k -Name $name -ErrorAction SilentlyContinue).$name;\n\
         $entry=@('{nt_path}','');\n\
         if ($existing) {{ $new=$existing + $entry }} else {{ $new=$entry }};\n\
         New-ItemProperty -Path $k -Name $name -PropertyType MultiString -Value $new -Force | Out-Null;\n\
         if (Test-Path -LiteralPath $p) {{\n\
           if ((Get-ItemProperty -Path $k -Name $name).$name -contains '{nt_path}') {{ exit 1 }} else {{ exit 2 }}\n\
         }} else {{ exit 0 }}",
    );
    let utf16: Vec<u16> = script.encode_utf16().collect();
    let bytes: Vec<u8> = utf16.iter().flat_map(|c| c.to_le_bytes()).collect();
    let encoded = base64_std_encode(&bytes);
    let arg = format!(
        "-NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand {}",
        encoded
    );
    let exit = run_elevated_exe_capture(
        app.clone(),
        std::path::PathBuf::from(
            "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe",
        ),
        Some(&arg),
    )
    .await?;
    match exit {
        0 => Ok(SysRemoveOutcome::Removed),
        1 => Ok(SysRemoveOutcome::PendingReboot),
        n => Err(format!(
            "兜底删除驱动文件失败（PowerShell 退出码 {}）",
            n
        )),
    }
}

/// 极简 Base64 标准编码（PowerShell -EncodedCommand 用），无需引入 base64 crate。
#[cfg(windows)]
fn base64_std_encode(input: &[u8]) -> String {
    const TBL: &[u8] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
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

/// 把「驱动文件是否落盘」与「ddc.exe 退出码」合并成最终的安装判定结果。
///
/// `ddc.exe` 在交互式 cmd 中收尾会 `pause`，用户按键后退出码不可信；
/// 因此即便外部进程返回错误，只要驱动 `.sys` 已经落盘就视为安装成功。
// 非 Windows 编译路径下没有调用方（仅 DD-HID 命令使用），但函数本身跨平台，
// 留它在这里方便测试与未来移植。
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn judge_install_result(
    sys_installed: bool,
    exe_result: Result<(), String>,
) -> Result<(), String> {
    if sys_installed {
        Ok(())
    } else {
        Err(match exe_result {
            Ok(()) => "驱动安装未生效".to_string(),
            Err(e) => e,
        })
    }
}

/// 与 [`judge_install_result`] 对称：以驱动文件是否被移除作为卸载成功的最终标志。
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn judge_uninstall_result(
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

/// HID 驱动是否已安装：检测 system32\drivers\ddhid63340.sys 是否存在
#[tauri::command]
pub fn is_dd_hid_driver_installed() -> bool {
    #[cfg(windows)]
    {
        dd_hid_sys_installed()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[tauri::command]
pub async fn install_dd_hid_driver(app: AppHandle) -> Result<(), String> {
    #[cfg(windows)]
    {
        let exe = resource_dir(&app)?.join("ddhid-driver").join("ddc.exe");
        // ddc.exe 在交互式 cmd 中收尾会 `pause`，用户按键后退出码不可信；
        // 以驱动文件是否落盘为最终判定。
        let exe_result = run_elevated_exe(app.clone(), exe, None).await;
        let sys_installed = dd_hid_sys_installed();
        let result = judge_install_result(sys_installed, exe_result.clone());
        if let Err(ref e) = result {
            // sys 没落盘时记一条 error，把 ddc.exe 的退出码 / 错误信息一并落盘，
            // 方便用户反馈时贴日志诊断 PnP 残留 / 资源缺失等问题。
            let exe_state = match &exe_result {
                Ok(()) => "ddc.exe 报告成功".to_string(),
                Err(msg) => format!("ddc.exe 失败: {msg}"),
            };
            tracing::error!(
                "DD-HID 驱动安装失败：{e}（{exe_state}，sys 落盘={sys_installed}）"
            );
        }
        crate::commands::status::emit_status_changed(&app);
        result
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err("仅 Windows 平台支持安装 DD-HID 驱动".to_string())
    }
}

/// 卸载结果。`pending_reboot=true` 表示驱动文件已标记为重启删除、卸载在逻辑上
/// 已完成，但物理文件要等下次开机才消失。
#[derive(Debug, Clone, serde::Serialize)]
pub struct UninstallOutcome {
    /// 用户友好提示文案
    pub message: String,
    /// 是否需要重启计算机以最终清理驱动文件
    pub pending_reboot: bool,
}

#[tauri::command]
pub async fn uninstall_dd_hid_driver(app: AppHandle) -> Result<UninstallOutcome, String> {
    #[cfg(windows)]
    {
        use crate::engine::input::{init_backend, InputMode};
        use tauri_plugin_store::StoreExt;
        // 卸载前先切回 SendInput，释放 DLL
        init_backend(InputMode::SendInput);
        if let Ok(store) = app.store(crate::STORE_PATH) {
            store.set("input_mode", serde_json::json!("sendinput"));
            let _ = store.save();
        }

        let exe = resource_dir(&app)?.join("ddhid-driver").join("ddc.exe");
        // ddc.exe 在交互式 cmd 中收尾会 `pause`，用户按键后退出码不可信；
        // 以驱动文件是否被移除为最终判定。
        let exe_result = run_elevated_exe(app.clone(), exe, Some("-u")).await;
        // 兜底：ddc.exe 偶发会以非 0 退出且把 sys 留在原地（TrustedInstaller 占有 ACL、
        // PnP 异步等）。统一交给 try_force_remove_dd_hid_sys 接管：takeown + icacls + 删除，
        // 删不掉再写 PendingFileRenameOperations 标记重启清理。
        let mut pending_reboot = false;
        if dd_hid_sys_installed() {
            match try_force_remove_dd_hid_sys(&app).await {
                Ok(SysRemoveOutcome::Removed) => {}
                Ok(SysRemoveOutcome::PendingReboot) => pending_reboot = true,
                Err(e) => tracing::warn!("强删 ddhid63340.sys 兜底失败：{}", e),
            }
        }
        crate::commands::status::emit_status_changed(&app);

        if pending_reboot {
            return Ok(UninstallOutcome {
                message: "驱动文件已标记为重启后清理，请重启电脑完成卸载。".to_string(),
                pending_reboot: true,
            });
        }
        let sys_still_present = dd_hid_sys_installed();
        match judge_uninstall_result(sys_still_present, exe_result.clone()) {
            Ok(()) => Ok(UninstallOutcome {
                message: "究极HID 驱动已卸载".to_string(),
                pending_reboot: false,
            }),
            Err(e) => {
                let exe_state = match &exe_result {
                    Ok(()) => "ddc.exe 报告成功".to_string(),
                    Err(msg) => format!("ddc.exe 失败: {msg}"),
                };
                tracing::error!(
                    "DD-HID 驱动卸载失败：{e}（{exe_state}，sys 仍存在={sys_still_present}）"
                );
                Err(e)
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err("仅 Windows 平台支持卸载 DD-HID 驱动".to_string())
    }
}

// ===== 提权重启 =====

#[cfg(windows)]
pub(crate) fn is_process_elevated() -> bool {
    use std::mem;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    const TOKEN_QUERY: u32 = 0x0008;
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: GetCurrentProcess 返回伪句柄无需释放;OpenProcessToken 写入 token 出参
    let ok = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) };
    if ok == 0 {
        return false;
    }
    // SAFETY: TOKEN_ELEVATION 是 POD,全 0 初始化合法
    let mut elev: TOKEN_ELEVATION = unsafe { mem::zeroed() };
    let mut ret_len: u32 = 0;
    // SAFETY: token 来自上面 OpenProcessToken 成功调用;elev 是栈上 POD,
    // 大小由 size_of 提供;ret_len 是栈上 u32 出参
    let got = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            &mut elev as *mut _ as *mut _,
            mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret_len,
        )
    };
    // SAFETY: token 是上面 OpenProcessToken 返回的有效句柄
    unsafe { CloseHandle(token) };
    got != 0 && elev.TokenIsElevated != 0
}

#[tauri::command]
pub fn is_elevated() -> bool {
    #[cfg(windows)]
    {
        is_process_elevated()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// 以管理员身份重启自身。当前进程会通过 ShellExecuteEx 启动新实例（带 runas verb），
/// 然后 `app.exit(0)` 触发本进程退出。新进程读取 `--switch-mode=<id>` 自动设定模式。
#[tauri::command]
pub async fn relaunch_as_admin(app: AppHandle, mode: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Foundation::ERROR_CANCELLED;
        use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SHELLEXECUTEINFOW};

        // 校验目标模式合法
        let _ = crate::engine::input::InputMode::from_str(&mode)
            .ok_or_else(|| format!("未知输入模式: {}", mode))?;

        let exe = std::env::current_exe().map_err(|e| format!("无法定位当前可执行文件: {}", e))?;
        let path_wide: Vec<u16> = exe
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let verb: Vec<u16> = "runas\0".encode_utf16().collect();
        let params: Vec<u16> = format!("--elevated --switch-mode={}\0", mode)
            .encode_utf16()
            .collect();

        let result = tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
            // SAFETY: SHELLEXECUTEINFOW 是 POD,全 0 初始化合法,后续逐字段填写
            let mut sei: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
            sei.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
            sei.lpVerb = verb.as_ptr();
            sei.lpFile = path_wide.as_ptr();
            sei.lpParameters = params.as_ptr();
            sei.nShow = 1;

            // SAFETY: verb / path_wide / params 都是 NUL 结尾的宽串,Vec 在闭包内存活
            let ok = unsafe { ShellExecuteExW(&mut sei) };
            if ok == 0 {
                // SAFETY: GetLastError 无参,任意线程任意时刻调用安全
                let err = unsafe { windows_sys::Win32::Foundation::GetLastError() };
                return if err == ERROR_CANCELLED {
                    Err("已取消管理员授权".to_string())
                } else {
                    Err(format!("启动管理员实例失败 (Win32 错误码 {})", err))
                };
            }
            Ok(())
        })
        .await
        .map_err(|e| format!("任务异常: {}", e))?;

        result.as_ref().map_err(|e| e.clone())?;

        // 新实例已启动（不等其退出）。让前端有时间收到响应，再触发本进程退出
        let app_clone = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            app_clone.exit(0);
        });
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (app, mode);
        Err("仅 Windows 平台支持提权重启".to_string())
    }
}

#[cfg(all(test, windows))]
#[path = "engine_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "engine_judge_tests.rs"]
mod judge_tests;
