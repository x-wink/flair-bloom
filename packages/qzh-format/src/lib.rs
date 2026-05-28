//! `.qzh` 配置文件容器格式：二进制文件头（[`header`]）与加解密 helper。
//!
//! 加密与解密由 [`crypto`] crate 负责，本 crate 定义"二进制布局如何拼装"，
//! 并提供 [`read_encrypted`] / [`write_encrypted`] 封装文件 I/O 五连段。
//!
//! 业务 Profile schema（`Profile` / `BurstRule` / `KeyId` 等）已迁至 `qzh-profile` crate。
#![deny(missing_docs)]

pub mod header;

use std::path::Path;

use header::{FileHeader, MAGIC, VERSION};
use serde::Serialize;
use thiserror::Error;

/// 读取或写入 `.qzh` 文件时可能发生的错误。
#[derive(Debug, Error)]
pub enum FormatError {
    /// 文件 I/O 错误。
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// 文件头无效（魔数/版本号不匹配，或长度不足）。
    #[error("invalid .qzh file header")]
    InvalidHeader,
    /// AES-GCM 加解密错误。
    #[error(transparent)]
    Crypto(#[from] crypto::CryptoError),
    /// JSON 序列化/反序列化错误。
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// 读取并解密一个 `.qzh` 文件，返回 JSON 载荷与 `schema_version`。
///
/// `schema_version` 字段缺失时返回 `1`（最低已知版本），调用方可安全运行迁移。
/// 上层（`qzh-profile`）负责 schema 迁移、反序列化与业务校验。
pub fn read_encrypted(path: &Path) -> Result<(serde_json::Value, u32), FormatError> {
    let data = std::fs::read(path)?;
    let header = FileHeader::from_bytes(&data).ok_or(FormatError::InvalidHeader)?;
    let aad = header.aad();
    let ciphertext = &data[FileHeader::SIZE..];
    let plaintext = crypto::aes::decrypt(ciphertext, &header.nonce, &aad)?;
    let value: serde_json::Value = serde_json::from_slice(&plaintext)?;
    let schema_version = value
        .get("schema_version")
        .and_then(|v| v.as_u64())
        .unwrap_or(1) as u32;
    Ok((value, schema_version))
}

/// 序列化并加密 `value` 写入 `.qzh` 文件（原子覆写）。
///
/// 先写入同目录下 `.tmp` 临时文件再 rename，确保断电不产生半写文件。
pub fn write_encrypted<T: Serialize>(path: &Path, value: &T) -> Result<(), FormatError> {
    let json = serde_json::to_vec(value)?;
    // AAD 不含 nonce，可在加密前确定
    let mut aad = Vec::with_capacity(7);
    aad.extend_from_slice(MAGIC);
    aad.push(VERSION);
    aad.extend_from_slice(&0u16.to_le_bytes()); // flags = 0
    let (ciphertext, nonce) = crypto::aes::encrypt(&json, &aad)?;
    let header = FileHeader::new(nonce);
    let mut data = header.to_bytes();
    data.extend_from_slice(&ciphertext);
    let tmp_path = path.with_extension("tmp");
    std::fs::write(&tmp_path, &data)?;
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn read_write_round_trip() {
        let dir = std::env::temp_dir();
        let path = dir.join("qzh_format_test_roundtrip.qzh");
        let value = json!({"schema_version": 2, "hello": "world"});
        write_encrypted(&path, &value).unwrap();
        let (loaded, version) = read_encrypted(&path).unwrap();
        assert_eq!(version, 2);
        assert_eq!(loaded["hello"], "world");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_encrypted_errors_on_missing_file() {
        let path = std::path::PathBuf::from("/tmp/qzh_format_nonexistent_test.qzh");
        let _ = std::fs::remove_file(&path); // 确保文件不存在
        assert!(matches!(read_encrypted(&path), Err(FormatError::Io(_))));
    }

    #[test]
    fn read_encrypted_defaults_schema_version_to_one_when_absent() {
        let dir = std::env::temp_dir();
        let path = dir.join("qzh_format_no_version.qzh");
        let value = json!({"data": "no schema_version field"});
        write_encrypted(&path, &value).unwrap();
        let (_, version) = read_encrypted(&path).unwrap();
        assert_eq!(version, 1);
        let _ = std::fs::remove_file(&path);
    }
}
