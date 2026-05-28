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
    // Hold 模式靠 input.rs 的注入事件队列识别 sim 事件，允许 trigger == target；
    // 另外 X1/X2 鼠标键作为 target 在 DD 模式不被支持（DD_btn 值域所限）。
    #[cfg(windows)]
    {
        use qzh_format::key_id::{KeyId, MouseButton};

        let mode = crate::engine::input::current_mode();
        if mode.requires_distinct_target_for_toggle() {
            for rule in rules.iter().filter(|r| r.enabled) {
                if matches!(
                    rule.target_key,
                    KeyId::Mouse(MouseButton::X1) | KeyId::Mouse(MouseButton::X2)
                ) {
                    return Err(format!(
                        "究极HID 模式不支持鼠标侧键作为目标键，请把规则「{}」的目标键换成左/右/中键或键盘键",
                        rule.id
                    ));
                }
                if !matches!(rule.mode, qzh_format::profile::BurstMode::Toggle) {
                    continue;
                }
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
            InputMode::from_str(&mode).ok_or_else(|| format!("未知输入模式: {mode}"))?;

        // 切到 DD-HID 时要求当前已是管理员，否则前端应先发起提权重启
        if input_mode.requires_admin() && !is_process_elevated() {
            return Err("究极HID 模式需要管理员权限，请先以管理员身份重启应用".to_string());
        }

        // 切到 DD-HID 前用新规则约束做静态校验：
        // - Toggle 模式要求 target 与 trigger/stop 互异
        // - target 不允许是鼠标 X1/X2（DD_btn 值域所限）
        if input_mode.requires_distinct_target_for_toggle() {
            use qzh_format::key_id::{KeyId, MouseButton};
            let rules = state.0.get_rules();
            for rule in rules.iter().filter(|r| r.enabled) {
                if matches!(
                    rule.target_key,
                    KeyId::Mouse(MouseButton::X1) | KeyId::Mouse(MouseButton::X2)
                ) {
                    return Err(format!(
                        "切换失败：规则「{}」的目标键是鼠标侧键，究极HID 模式不支持。请把目标键改为左/右/中键或键盘键。",
                        rule.id
                    ));
                }
                if !matches!(rule.mode, qzh_format::profile::BurstMode::Toggle) {
                    continue;
                }
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
        n => Err(format!("程序返回错误码 {n}")),
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
                Err(format!("启动程序失败 (Win32 错误码 {err})"))
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
    .map_err(|e| format!("任务异常: {e}"))?
}

#[cfg(windows)]
fn resource_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let raw = app
        .path()
        .resource_dir()
        .map_err(|e| format!("无法获取资源目录: {e}"))?
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
        return std::path::PathBuf::from(format!(r"\\{rest}"));
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

/// 调用 `pnputil /enum-drivers`，扫描 `%SystemRoot%\INF\` 下注册的 OEM INF，
/// 找出归属 ddhid63340 的 oem 编号（如 `["oem15.inf"]`）。
///
/// 走 INF 文件内容匹配而非 pnputil 输出解析，因为 pnputil 是本地化文本而 INF 内
/// 的 `ddhid63340` 关键字与语言无关、稳定。
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
        let Ok(content) = std::fs::read(entry.path()) else {
            continue;
        };
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

/// 通过 `pnputil /delete-driver oemXX.inf /uninstall /force` 走 PnP 标准卸载流程。
///
/// PnP 子系统会自己处理：停服务 → 释放设备实例 → 删 sys → 清 Driver Store → 移除 INF。
/// 即便 sys 被 TrustedInstaller 持有也由 PnP 提权完成，无需 takeown/icacls 强夺。
///
/// 退出码：
/// - 0 = 全部 INF 卸载成功
/// - 1 = 没有需要卸载的 INF（视为成功，幂等）
/// - 2 = 至少一个 INF 卸载失败（PnP 仍持有设备实例 / 资源被占用，应建议重启）
#[cfg(windows)]
async fn pnputil_uninstall_dd_hid(app: &AppHandle) -> Result<u32, String> {
    let oem_list = find_dd_hid_oem_inf();
    if oem_list.is_empty() {
        return Ok(1);
    }
    let oem_array = build_ps_string_array(&oem_list);
    let script = format!(
        "$ErrorActionPreference='Continue';\n\
         $hardFail=$false;\n\
         foreach ($oem in {oem_array}) {{\n\
             try {{ & pnputil.exe /delete-driver $oem /uninstall /force | Out-Null }}\n\
             catch {{ $hardFail=$true }}\n\
             if ($LASTEXITCODE -ne 0) {{ $hardFail=$true }}\n\
         }}\n\
         if ($hardFail) {{ exit 2 }}\n\
         exit 0",
    );
    run_powershell_script_elevated(app, &script).await
}

/// 把字符串包成 PowerShell 单引号字面量，单引号转义为两个单引号。
#[cfg(windows)]
fn ps_single_quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    out.push_str(&s.replace('\'', "''"));
    out.push('\'');
    out
}

/// 把 `[String]` 编为 PowerShell 字面量数组：`@('a','b')`。
#[cfg(windows)]
fn build_ps_string_array(items: &[String]) -> String {
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

/// 把脚本编码成 `-EncodedCommand` 形式并提权执行，返回真实退出码。
#[cfg(windows)]
async fn run_powershell_script_elevated(app: &AppHandle, script: &str) -> Result<u32, String> {
    let utf16: Vec<u16> = script.encode_utf16().collect();
    let bytes: Vec<u8> = utf16.iter().flat_map(|c| c.to_le_bytes()).collect();
    let encoded = base64_std_encode(&bytes);
    let arg =
        format!("-NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand {encoded}");
    run_elevated_exe_capture(
        app.clone(),
        std::path::PathBuf::from("C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe"),
        Some(&arg),
    )
    .await
}

/// 极简 Base64 标准编码（PowerShell -EncodedCommand 用），无需引入 base64 crate。
#[cfg(windows)]
pub(crate) fn base64_std_encode(input: &[u8]) -> String {
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

/// 把「驱动文件是否落盘」「服务键是否注册」与「ddc.exe 退出码」合并成最终的安装判定结果。
///
/// `ddc.exe` 在交互式 cmd 中收尾会 `pause`，用户按键后退出码不可信；
/// 必须以驱动 `.sys` 落盘 *并且* `HKLM\...\Services\ddhid63340` 服务键存在为最终判据。
///
/// 单看 sys 文件会误判一种边界场景：用户卸载后未重启就立刻点重装，
/// 卸载阶段 PnP 已经把服务键删了，但 sys 文件被设备实例锁住要重启才能清理。
/// 这种状态下 ddc.exe 走完一遍并不会重新注册服务（PnP 拒绝），sys 看起来还在原地，
/// 但驱动其实没生效——必须靠 service key 这一维区分。
// 非 Windows 编译路径下没有调用方（仅 DD-HID 命令使用），但函数本身跨平台，
// 留它在这里方便测试与未来移植。
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn judge_install_result(
    sys_installed: bool,
    service_present: bool,
    exe_result: Result<(), String>,
) -> Result<(), String> {
    if sys_installed && service_present {
        return Ok(());
    }
    if sys_installed && !service_present {
        // 半卸载残留：sys 文件还在但服务键已删，PnP 拒绝重新注册
        return Err("检测到上次卸载留下的驱动残留尚未清理，本次安装未生效。\n\
             请重启电脑让 PnP 完成清理后再尝试安装。"
            .to_string());
    }
    Err(match exe_result {
        Ok(()) => "驱动安装未生效".to_string(),
        Err(e) => e,
    })
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
        // 以驱动文件 + 服务键是否同时就位为最终判定。
        let exe_result = run_elevated_exe(app.clone(), exe, None).await;
        let sys_installed = dd_hid_sys_installed();
        let service_present = crate::commands::repair::service_key_present("ddhid63340");
        let result = judge_install_result(sys_installed, service_present, exe_result.clone());
        if let Err(ref e) = result {
            // 真正失败时把退出码 / 错误信息一并落盘，方便用户反馈时贴日志诊断
            // 半卸载残留 / 资源缺失等问题。
            let exe_state = match &exe_result {
                Ok(()) => "ddc.exe 报告成功".to_string(),
                Err(msg) => format!("ddc.exe 失败: {msg}"),
            };
            tracing::error!(
                "DD-HID 驱动安装失败：{e}（{exe_state}，sys 落盘={sys_installed}，服务键={service_present}）"
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
        // 兜底：ddc.exe 偶发会以非 0 退出且把 sys 留在原地（PnP 异步、用户取消等）。
        // 走 pnputil /delete-driver oemXX.inf /uninstall /force 让 PnP 子系统按
        // 标准流程处理：停服务 → 释放设备实例 → 删 sys → 清 Driver Store → 移除 INF。
        // 不再 takeown / icacls / PendingFileRenameOperations 强夺 sys——那会
        // 留下半卸载状态阻塞重装。
        let mut pending_reboot = false;
        if dd_hid_sys_installed() {
            match pnputil_uninstall_dd_hid(&app).await {
                Ok(0) | Ok(1) => {}
                Ok(2) => {
                    // pnputil 部分失败：通常是 PnP 仍持有设备实例 / 资源被占用
                    // 重启后 PnP 会在下次启动阶段释放并完成清理
                    pending_reboot = true;
                }
                Ok(n) => tracing::warn!("pnputil 卸载返回未知退出码 {n}"),
                Err(e) => tracing::warn!("pnputil 卸载兜底失败：{}", e),
            }
            // pnputil 卸载后 sys 仍在 → 视作"需要重启"由 PnP 完成
            if dd_hid_sys_installed() {
                pending_reboot = true;
            }
        }
        crate::commands::status::emit_status_changed(&app);

        if pending_reboot {
            return Ok(UninstallOutcome {
                message: "驱动卸载已发起，剩余清理需重启电脑由 PnP 完成。\n\
                    请重启电脑后再尝试安装驱动。"
                    .to_string(),
                pending_reboot: true,
            });
        }
        let sys_still_present = dd_hid_sys_installed();
        match judge_uninstall_result(sys_still_present, exe_result.clone()) {
            Ok(()) => Ok(UninstallOutcome {
                message: "究极HID 驱动已卸载，建议重启电脑后再尝试重新安装。".to_string(),
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
            .ok_or_else(|| format!("未知输入模式: {mode}"))?;

        let exe = std::env::current_exe().map_err(|e| format!("无法定位当前可执行文件: {e}"))?;
        let path_wide: Vec<u16> = exe
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let verb: Vec<u16> = "runas\0".encode_utf16().collect();
        let params: Vec<u16> = format!("--elevated --switch-mode={mode}\0")
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
                    Err(format!("启动管理员实例失败 (Win32 错误码 {err})"))
                };
            }
            Ok(())
        })
        .await
        .map_err(|e| format!("任务异常: {e}"))?;

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
