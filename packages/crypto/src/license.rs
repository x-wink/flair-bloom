//! Ed25519 离线许可证校验。
//!
//! 兑换码格式 `QZHUA-XXXXX-XXXXX-XXXXX-XXXXX`：Base32 解码后为 64 字节 Ed25519
//! 签名 + 一段 JSON payload（[`LicensePayload`]）。公钥内嵌在二进制中，私钥仅在
//! `apps/keygen` CLI 持有。

use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 许可证 payload 中 `features` 位掩码的位定义。每位代表一个亲友专属功能。
pub mod feature_bits {
    /// 宏录制与回放。
    pub const MACRO_RECORD: u32 = 1 << 0;
    /// 鼠标连点。
    pub const MOUSE_BURST: u32 = 1 << 1;
    /// 随机抖动（反检测）。
    pub const RANDOM_JITTER: u32 = 1 << 2;
    /// 条件配置集（按窗口/进程切换）。
    pub const CONDITIONAL_PROFILE: u32 = 1 << 3;
    /// 桌宠扩展动画包。
    pub const PET_ANIMATION_PACK: u32 = 1 << 4;
}

/// 已签名的许可证 payload，由 `apps/keygen` 用 Ed25519 私钥签发。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicensePayload {
    /// payload schema 版本号。
    pub version: u8,
    /// Unix timestamp — lower bound for clock-rollback detection
    pub issue_time: u64,
    /// Unix timestamp — expiry
    pub expiry: u64,
    /// 已开通的功能位掩码，参见 [`feature_bits`]。
    pub features: u32,
}

/// [`verify_license`] 校验失败时返回的错误。
#[derive(Debug, Error)]
pub enum LicenseError {
    /// 兑换码格式错误：长度/Base32 字符集不合法，或 payload 不是合法 JSON。
    #[error("invalid code format")]
    InvalidFormat,
    /// 签名校验失败：兑换码被篡改或公钥不匹配。
    #[error("signature verification failed")]
    InvalidSignature,
    /// 当前时间晚于 `expiry`，许可证已过期。
    #[error("license has expired")]
    Expired,
    /// 当前时间早于 `issue_time`，怀疑系统时钟被回拨。
    #[error("clock rollback detected")]
    ClockRollback,
}

// TODO: replace with actual public key before release
const PUBLIC_KEY_BYTES: [u8; 32] = [0u8; 32];

/// Verify a license code and return the decoded payload on success.
/// Code format: QZHUA-XXXXX-XXXXX-XXXXX-XXXXX (Base32)
pub fn verify_license(code: &str, now: u64) -> Result<LicensePayload, LicenseError> {
    let normalized = code.replace('-', "").to_uppercase();
    let bytes = base32::decode(base32::Alphabet::Rfc4648 { padding: false }, &normalized)
        .ok_or(LicenseError::InvalidFormat)?;

    if bytes.len() < 64 {
        return Err(LicenseError::InvalidFormat);
    }

    let (sig_bytes, payload_bytes) = bytes.split_at(64);
    let sig = Signature::from_slice(sig_bytes).map_err(|_| LicenseError::InvalidSignature)?;
    let vk =
        VerifyingKey::from_bytes(&PUBLIC_KEY_BYTES).map_err(|_| LicenseError::InvalidSignature)?;
    vk.verify_strict(payload_bytes, &sig)
        .map_err(|_| LicenseError::InvalidSignature)?;

    let payload: LicensePayload =
        serde_json::from_slice(payload_bytes).map_err(|_| LicenseError::InvalidFormat)?;

    if now < payload.issue_time {
        return Err(LicenseError::ClockRollback);
    }
    if now > payload.expiry {
        return Err(LicenseError::Expired);
    }

    Ok(payload)
}

#[cfg(test)]
#[path = "license_tests.rs"]
mod tests;
