//! Windows 路径工具：去掉 verbatim 路径前缀。

/// 去掉 Windows verbatim 路径前缀（`\\?\<drive>:\...` → `<drive>:\...`），
/// 保留真正的 UNC 路径（`\\?\UNC\server\share\...` → `\\server\share\...`）。
pub fn strip_verbatim(path: std::path::PathBuf) -> std::path::PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        return std::path::PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = s.strip_prefix(r"\\?\") {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_verbatim_removes_drive_prefix() {
        let p = std::path::PathBuf::from(r"\\?\C:\Windows\System32");
        assert_eq!(
            strip_verbatim(p),
            std::path::PathBuf::from(r"C:\Windows\System32")
        );
    }

    #[test]
    fn strip_verbatim_handles_lowercase_drive() {
        let p = std::path::PathBuf::from(r"\\?\d:\foo\bar");
        assert_eq!(strip_verbatim(p), std::path::PathBuf::from(r"d:\foo\bar"));
    }

    #[test]
    fn strip_verbatim_converts_unc_back_to_double_slash() {
        let p = std::path::PathBuf::from(r"\\?\UNC\server\share\dir");
        assert_eq!(
            strip_verbatim(p),
            std::path::PathBuf::from(r"\\server\share\dir")
        );
    }
}
