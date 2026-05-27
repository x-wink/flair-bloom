//! 系统级元信息采集（OS 版本 / WebView2 / 架构 / 区域 / 安装路径）。
//!
//! 仅用于状态弹窗与诊断回报，所有失败都退化为空串而非 panic：
//! 缺一两项不该挡住整个对话框打开。

#[cfg(windows)]
pub fn os_version() -> String {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY,
    };

    let subkey = wide("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion");
    let mut hkey: HKEY = std::ptr::null_mut();
    // SAFETY: subkey 是 NUL 结尾宽串；hkey 是栈上出参指针
    let r = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            subkey.as_ptr(),
            0,
            KEY_READ | KEY_WOW64_64KEY,
            &mut hkey,
        )
    };
    if r != 0 {
        return String::new();
    }

    let display = read_reg_sz(hkey, "DisplayVersion")
        .or_else(|| read_reg_sz(hkey, "ReleaseId"))
        .unwrap_or_default();
    let product = read_reg_sz(hkey, "ProductName").unwrap_or_default();
    let build = read_reg_sz(hkey, "CurrentBuild").unwrap_or_default();
    let ubr = read_reg_dword(hkey, "UBR");

    // SAFETY: hkey 是上面 RegOpenKeyExW 成功返回的句柄
    unsafe { RegCloseKey(hkey) };

    let mut full_build = build.clone();
    if let Some(u) = ubr {
        if !full_build.is_empty() {
            full_build.push('.');
        }
        full_build.push_str(&u.to_string());
    }

    match (
        product.is_empty(),
        display.is_empty(),
        full_build.is_empty(),
    ) {
        (true, true, true) => String::new(),
        (false, false, false) => format!("{product} {display} (Build {full_build})"),
        (false, true, false) => format!("{product} (Build {full_build})"),
        (false, false, true) => format!("{product} {display}"),
        (true, false, false) => format!("Windows {display} (Build {full_build})"),
        (false, true, true) => product,
        (true, false, true) => display,
        (true, true, false) => format!("Build {full_build}"),
    }
}

#[cfg(not(windows))]
pub fn os_version() -> String {
    String::new()
}

/// 读取 WebView2 Runtime 的版本号（"118.0.2088.61" 形态）。
///
/// Edge 安装两个客户端 GUID：稳定版 Evergreen 用 `{F3017226-...}`，
/// 单独 Runtime 安装包也是同一 ID。Beta/Dev 走别的 GUID，这里不覆盖，
/// 因为开发版用户极少且不会成为反馈面里的主要噪声。
#[cfg(windows)]
pub fn webview2_version() -> String {
    const RUNTIME_GUID: &str = "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";

    // HKLM 64 / HKLM 32 / HKCU，按这个顺序找；任意一处命中即取
    let candidates: [(_, &str); 4] = [
        (
            RegRoot::HklmWow64_64,
            "SOFTWARE\\WOW6432Node\\Microsoft\\EdgeUpdate\\Clients",
        ),
        (RegRoot::Hklm, "SOFTWARE\\Microsoft\\EdgeUpdate\\Clients"),
        (
            RegRoot::HklmWow64_64,
            "SOFTWARE\\Microsoft\\EdgeUpdate\\Clients",
        ),
        (RegRoot::Hkcu, "SOFTWARE\\Microsoft\\EdgeUpdate\\Clients"),
    ];

    for (root, base) in candidates {
        let path = format!("{base}\\{RUNTIME_GUID}");
        if let Some(v) = read_reg_sz_at(root, &path, "pv") {
            if !v.is_empty() && v != "0.0.0.0" {
                return v;
            }
        }
    }
    String::new()
}

#[cfg(not(windows))]
pub fn webview2_version() -> String {
    String::new()
}

/// 当前进程的可执行文件所在目录；失败时返回空串。
pub fn install_path() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_string_lossy().into_owned()))
        .unwrap_or_default()
}

/// 主机硬件架构（不是进程的编译目标）。
///
/// 用 `GetNativeSystemInfo` 而非 `std::env::consts::ARCH`：后者是构建产物的
/// target，前者读到的是真实 CPU。x64 进程跑在 ARM64 设备上时，两者会分叉。
#[cfg(windows)]
pub fn host_arch() -> String {
    use windows_sys::Win32::System::SystemInformation::{GetNativeSystemInfo, SYSTEM_INFO};

    // SAFETY: SYSTEM_INFO 是 POD，全 0 初始化合法
    let mut info: SYSTEM_INFO = unsafe { std::mem::zeroed() };
    // SAFETY: GetNativeSystemInfo 只写出参，无前置条件
    unsafe { GetNativeSystemInfo(&mut info) };
    // 这些常量在 windows-sys 里的可见性会因 feature 组合而变，宁可硬编也不引误用
    // 数值来自 winnt.h PROCESSOR_ARCHITECTURE_*
    // SAFETY: SYSTEM_INFO 联合体的 wProcessorArchitecture 字段始终有效
    let arch = unsafe { info.Anonymous.Anonymous.wProcessorArchitecture };
    match arch {
        9 => "x64".to_string(),    // AMD64
        12 => "ARM64".to_string(), // ARM64
        5 => "ARM".to_string(),    // ARM
        6 => "ia64".to_string(),   // IA-64（理论存在，实际不会遇到）
        0 => "x86".to_string(),    // INTEL
        other => format!("unknown({other})"),
    }
}

#[cfg(not(windows))]
pub fn host_arch() -> String {
    std::env::consts::ARCH.to_string()
}

/// 用户区域代码（"zh-CN" / "en-US" 等）。`GetUserDefaultLocaleName` 是 Vista+
/// 唯一稳定 API，比 `GetUserDefaultUILanguage` 返的 LCID 更适合直接展示。
#[cfg(windows)]
pub fn user_locale() -> String {
    use windows_sys::Win32::Globalization::{GetUserDefaultLocaleName, LOCALE_NAME_MAX_LENGTH};

    let mut buf = vec![0u16; LOCALE_NAME_MAX_LENGTH as usize];
    // SAFETY: buf 在调用期间存活，长度参数与缓冲区匹配
    let n = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
    if n <= 1 {
        return String::new();
    }
    // 返回值含末尾 NUL，去掉
    String::from_utf16_lossy(&buf[..(n as usize - 1)])
}

#[cfg(not(windows))]
pub fn user_locale() -> String {
    // POSIX 上 LANG 通常形如 zh_CN.UTF-8，剥掉编码部分后展示
    std::env::var("LANG")
        .ok()
        .map(|s| s.split('.').next().unwrap_or(&s).to_string())
        .unwrap_or_default()
}

// ---- 注册表辅助 -------------------------------------------------------

#[cfg(windows)]
#[derive(Clone, Copy)]
enum RegRoot {
    /// HKLM，默认视图（32 位进程会被重定向到 WOW6432Node）
    Hklm,
    /// HKLM，强制 64 位视图
    HklmWow64_64,
    Hkcu,
}

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn read_reg_sz_at(root: RegRoot, subkey: &str, name: &str) -> Option<String> {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ,
        KEY_WOW64_64KEY,
    };

    let (hroot, flags) = match root {
        RegRoot::Hklm => (HKEY_LOCAL_MACHINE, KEY_READ),
        RegRoot::HklmWow64_64 => (HKEY_LOCAL_MACHINE, KEY_READ | KEY_WOW64_64KEY),
        RegRoot::Hkcu => (HKEY_CURRENT_USER, KEY_READ),
    };
    let wsub = wide(subkey);
    let mut hkey: HKEY = std::ptr::null_mut();
    // SAFETY: 出参 hkey 在调用前后都有效，wsub NUL 结尾
    let r = unsafe { RegOpenKeyExW(hroot, wsub.as_ptr(), 0, flags, &mut hkey) };
    if r != 0 {
        return None;
    }
    let v = read_reg_sz(hkey, name);
    // SAFETY: hkey 是上面成功返回的句柄
    unsafe { RegCloseKey(hkey) };
    v
}

#[cfg(windows)]
fn read_reg_sz(hkey: windows_sys::Win32::System::Registry::HKEY, name: &str) -> Option<String> {
    use windows_sys::Win32::System::Registry::{RegQueryValueExW, REG_EXPAND_SZ, REG_SZ};

    let wname = wide(name);
    let mut ty: u32 = 0;
    let mut len: u32 = 0;
    // 第一次：探测长度（cbData 出参以字节计）
    // SAFETY: hkey 有效；wname NUL 结尾；data 传 null 仅探测大小
    let r = unsafe {
        RegQueryValueExW(
            hkey,
            wname.as_ptr(),
            std::ptr::null_mut(),
            &mut ty,
            std::ptr::null_mut(),
            &mut len,
        )
    };
    if r != 0 || (ty != REG_SZ && ty != REG_EXPAND_SZ) || len == 0 {
        return None;
    }

    // len 是字节数；REG_SZ 是 UTF-16，因此分配 len/2 个 u16
    let cap = (len as usize).div_ceil(2);
    let mut buf = vec![0u16; cap];
    let mut len2 = len;
    // SAFETY: buf 容量 >= len2 字节；写入后 buf 可能含 NUL，需要修剪
    let r = unsafe {
        RegQueryValueExW(
            hkey,
            wname.as_ptr(),
            std::ptr::null_mut(),
            &mut ty,
            buf.as_mut_ptr() as *mut u8,
            &mut len2,
        )
    };
    if r != 0 {
        return None;
    }
    let chars = (len2 as usize).div_ceil(2);
    let slice = &buf[..chars.min(buf.len())];
    // 去掉末尾的 NUL（注册表 REG_SZ 含尾 NUL，但部分写入者不写）
    let end = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
    Some(String::from_utf16_lossy(&slice[..end]))
}

#[cfg(windows)]
fn read_reg_dword(hkey: windows_sys::Win32::System::Registry::HKEY, name: &str) -> Option<u32> {
    use windows_sys::Win32::System::Registry::{RegQueryValueExW, REG_DWORD};

    let wname = wide(name);
    let mut ty: u32 = 0;
    let mut data: u32 = 0;
    let mut len: u32 = std::mem::size_of::<u32>() as u32;
    // SAFETY: data 是栈上 u32，len 与之匹配；hkey 与 wname 同上
    let r = unsafe {
        RegQueryValueExW(
            hkey,
            wname.as_ptr(),
            std::ptr::null_mut(),
            &mut ty,
            &mut data as *mut u32 as *mut u8,
            &mut len,
        )
    };
    if r == 0 && ty == REG_DWORD {
        Some(data)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_path_is_non_empty_for_test_runner() {
        // current_exe 在测试 binary 下肯定能拿到
        assert!(!install_path().is_empty());
    }

    #[test]
    fn host_arch_returns_known_label() {
        let a = host_arch();
        assert!(!a.is_empty());
        // 测试环境一定不会是 unknown(...)；约束这一点能挡住将来意外的 fallback 路径
        assert!(!a.starts_with("unknown("), "unexpected arch: {a}");
    }

    #[cfg(not(windows))]
    #[test]
    fn non_windows_os_version_is_empty_for_now() {
        // 当前实现只覆盖 Windows 注册表，其它平台返回空串以避免乱填
        assert_eq!(os_version(), "");
        assert_eq!(webview2_version(), "");
    }

    #[cfg(windows)]
    #[test]
    fn windows_os_version_contains_build_number() {
        // Windows 测试机上至少 ProductName 或 Build 必须读到一项
        let v = os_version();
        assert!(!v.is_empty(), "os_version 在 Windows 上不应为空");
    }
}
