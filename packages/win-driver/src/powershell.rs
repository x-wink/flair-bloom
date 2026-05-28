//! PowerShell 脚本提权执行 + 字符串辅助。

#[cfg(windows)]
use crate::elevation::run_elevated_exe_capture;

/// 把字符串包成 PowerShell 单引号字面量，内部单引号转义为两个单引号。
pub fn ps_single_quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    out.push_str(&s.replace('\'', "''"));
    out.push('\'');
    out
}

/// 把 `[String]` 编为 PowerShell 字面量数组：`@('a','b')`。
pub fn ps_string_array(items: &[String]) -> String {
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

/// 极简 Base64 标准编码（PowerShell -EncodedCommand 用）。
pub fn base64_std_encode(input: &[u8]) -> String {
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

/// 把脚本编码成 `-EncodedCommand` 形式并以管理员权限执行，返回真实退出码。
#[cfg(windows)]
pub async fn run_script_elevated(script: &str) -> Result<u32, String> {
    let utf16: Vec<u16> = script.encode_utf16().collect();
    let bytes: Vec<u8> = utf16.iter().flat_map(|c| c.to_le_bytes()).collect();
    let encoded = base64_std_encode(&bytes);
    let arg =
        format!("-NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand {encoded}");
    run_elevated_exe_capture(
        std::path::PathBuf::from(
            "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe",
        ),
        Some(&arg),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ps_string_array_escapes_quotes() {
        assert_eq!(ps_string_array(&[]), "@()");
        assert_eq!(
            ps_string_array(&["oem15.inf".to_string(), "oem99.inf".to_string()]),
            "@('oem15.inf','oem99.inf')"
        );
        assert_eq!(ps_string_array(&["a'b".to_string()]), "@('a''b')");
    }

    #[test]
    fn ps_single_quoted_escapes_inner_quotes() {
        assert_eq!(ps_single_quoted("plain"), "'plain'");
        assert_eq!(ps_single_quoted(""), "''");
        assert_eq!(
            ps_single_quoted("C:\\Users\\O'Brien\\Local"),
            "'C:\\Users\\O''Brien\\Local'"
        );
        assert_eq!(ps_single_quoted("a''b"), "'a''''b'");
    }
}
