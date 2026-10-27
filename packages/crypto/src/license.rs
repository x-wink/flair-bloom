//! 离线许可证校验：billing 签发的 EdDSA（Ed25519）JWS 紧凑串。
//!
//! 格式与校验口径和 billing 约定（`docs/roadmaps/license.md` D11–D12）：头部 `alg: EdDSA`、
//! `typ: xwink-license+jwt`、`kid`；载荷 `iss: billing`、`aud: flair`、`jti`、`ent`、`iat`，
//! 永久许可证不带 `exp`。公钥由 build script 按 `kid` 编入，私钥只在 billing。

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{de::DeserializeOwned, Deserialize};
use thiserror::Error;

include!(concat!(env!("OUT_DIR"), "/license_keys.rs"));

/// 自定义水印署名的权益键。
pub const ENT_SIGNATURE: &str = "flair.signature";

const ALGORITHM: &str = "EdDSA";
const TOKEN_TYPE: &str = "xwink-license+jwt";
const ISSUER: &str = "billing";
const AUDIENCE: &str = "flair";
/// `iat` 允许领先本机时钟的秒数。Windows 上时区设错导致系统 UTC 偏几小时很常见，
/// 不放宽的话刚激活就会被判成时钟回拨；回拨本来就只是顺手的一道（D9）。
const IAT_LEEWAY_SECS: u64 = 24 * 60 * 60;

/// 验签通过的许可证。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct License {
    /// 许可证 id（`jti`），重装时随激活请求带回 billing 以重下发同一张。
    pub id: String,
    /// 已授权的权益键（`ent`）。
    pub entitlements: Vec<String>,
    /// 签发时间（Unix 秒）。
    pub issued_at: u64,
    /// 到期时间（Unix 秒）；`None` 即永久。
    pub expires_at: Option<u64>,
}

impl License {
    /// 是否授权了指定权益键。
    pub fn grants(&self, entitlement: &str) -> bool {
        self.entitlements.iter().any(|e| e == entitlement)
    }
}

/// [`verify_license`] 校验失败的原因。
#[derive(Debug, Error, PartialEq, Eq)]
pub enum LicenseError {
    /// 不是三段式 JWS、base64url 或 JSON 不合法、缺必填字段。
    #[error("invalid license format")]
    InvalidFormat,
    /// `alg` 不是 EdDSA。
    #[error("unsupported signature algorithm")]
    UnsupportedAlgorithm,
    /// `typ` 不是许可证类型（例如拿 xwink 权益凭证来验）。
    #[error("not a license token")]
    WrongType,
    /// `kid` 不在内嵌公钥列表里：签发密钥已被移除，或是别的环境签的。
    #[error("unknown signing key")]
    UnknownKey,
    /// 签名校验失败：内容被篡改。
    #[error("signature verification failed")]
    InvalidSignature,
    /// `iss` 不是 billing。
    #[error("unexpected issuer")]
    WrongIssuer,
    /// `aud` 不含 flair，是签给别的应用的。
    #[error("license issued for another app")]
    WrongAudience,
    /// `iat` 明显晚于本机时间，怀疑系统时钟被回拨。
    #[error("clock rollback detected")]
    ClockRollback,
    /// 已过 `exp`。
    #[error("license has expired")]
    Expired,
}

#[derive(Deserialize)]
struct Header {
    alg: String,
    typ: Option<String>,
    kid: String,
}

#[derive(Deserialize)]
struct Claims {
    iss: String,
    aud: Audience,
    jti: String,
    #[serde(default)]
    ent: Vec<String>,
    iat: u64,
    exp: Option<u64>,
}

/// JWT 的 `aud` 可以是单个字符串或数组，billing 现在发字符串，两种都认。
#[derive(Deserialize)]
#[serde(untagged)]
enum Audience {
    One(String),
    Many(Vec<String>),
}

impl Audience {
    fn contains(&self, aud: &str) -> bool {
        match self {
            Self::One(a) => a == aud,
            Self::Many(list) => list.iter().any(|a| a == aud),
        }
    }
}

/// 用内嵌公钥校验许可证 JWS，`now` 为当前 Unix 秒。
pub fn verify_license(token: &str, now: u64) -> Result<License, LicenseError> {
    verify_with_keys(token, now, EMBEDDED_KEYS)
}

fn verify_with_keys(
    token: &str,
    now: u64,
    keys: &[(&str, [u8; 32])],
) -> Result<License, LicenseError> {
    let token = token.trim();
    let mut parts = token.split('.');
    let (Some(header_b64), Some(claims_b64), Some(sig_b64), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(LicenseError::InvalidFormat);
    };

    let header: Header = decode_json(header_b64)?;
    if header.alg != ALGORITHM {
        return Err(LicenseError::UnsupportedAlgorithm);
    }
    if header.typ.as_deref() != Some(TOKEN_TYPE) {
        return Err(LicenseError::WrongType);
    }
    let (_, key_bytes) = keys
        .iter()
        .find(|(kid, _)| *kid == header.kid)
        .ok_or(LicenseError::UnknownKey)?;

    let sig_bytes = URL_SAFE_NO_PAD
        .decode(sig_b64)
        .map_err(|_| LicenseError::InvalidFormat)?;
    let sig = Signature::from_slice(&sig_bytes).map_err(|_| LicenseError::InvalidFormat)?;
    let key = VerifyingKey::from_bytes(key_bytes).map_err(|_| LicenseError::UnknownKey)?;
    // 签名覆盖的是两段 base64url 原文，不是解码后的 JSON
    let signing_input = &token[..header_b64.len() + 1 + claims_b64.len()];
    key.verify_strict(signing_input.as_bytes(), &sig)
        .map_err(|_| LicenseError::InvalidSignature)?;

    let claims: Claims = decode_json(claims_b64)?;
    if claims.iss != ISSUER {
        return Err(LicenseError::WrongIssuer);
    }
    if !claims.aud.contains(AUDIENCE) {
        return Err(LicenseError::WrongAudience);
    }
    if claims.iat > now.saturating_add(IAT_LEEWAY_SECS) {
        return Err(LicenseError::ClockRollback);
    }
    if claims.exp.is_some_and(|exp| now >= exp) {
        return Err(LicenseError::Expired);
    }

    Ok(License {
        id: claims.jti,
        entitlements: claims.ent,
        issued_at: claims.iat,
        expires_at: claims.exp,
    })
}

fn decode_json<T: DeserializeOwned>(segment: &str) -> Result<T, LicenseError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(segment)
        .map_err(|_| LicenseError::InvalidFormat)?;
    serde_json::from_slice(&bytes).map_err(|_| LicenseError::InvalidFormat)
}

#[cfg(test)]
#[path = "kid.rs"]
mod kid;

#[cfg(test)]
#[path = "license_tests.rs"]
mod tests;
