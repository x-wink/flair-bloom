//! 许可证公钥的 `kid`：SPKI DER 的 SHA-256 做 base64url 后取前 22 字符。
//!
//! 必须与 billing（xwink-console `packages/identity/auth/src/verify/keys.ts::computeKeyId`）
//! 同一算法，否则按 `kid` 选钥永远落空。build script 也经 `#[path]` 引入本文件核对注入的公钥。

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use sha2::{Digest, Sha256};

/// Ed25519 公钥的 SPKI DER 固定前缀（`SEQUENCE { AlgorithmIdentifier { id-Ed25519 }, BIT STRING }`）。
const ED25519_SPKI_PREFIX: [u8; 12] = [
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
];

/// 计算 32 字节 Ed25519 原始公钥的 `kid`。
pub fn key_id(public_key: &[u8; 32]) -> String {
    let mut der = Vec::with_capacity(ED25519_SPKI_PREFIX.len() + public_key.len());
    der.extend_from_slice(&ED25519_SPKI_PREFIX);
    der.extend_from_slice(public_key);
    let digest = Sha256::digest(&der);
    let mut kid = URL_SAFE_NO_PAD.encode(digest);
    kid.truncate(22);
    kid
}
