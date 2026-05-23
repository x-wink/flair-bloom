use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Encoded in the license payload's `features` bitmask.
pub mod feature_bits {
    pub const MACRO_RECORD: u32 = 1 << 0;
    pub const MOUSE_BURST: u32 = 1 << 1;
    pub const RANDOM_JITTER: u32 = 1 << 2;
    pub const CONDITIONAL_PROFILE: u32 = 1 << 3;
    pub const PET_ANIMATION_PACK: u32 = 1 << 4;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicensePayload {
    pub version: u8,
    /// Unix timestamp — lower bound for clock-rollback detection
    pub issue_time: u64,
    /// Unix timestamp — expiry
    pub expiry: u64,
    pub features: u32,
}

#[derive(Debug, Error)]
pub enum LicenseError {
    #[error("invalid code format")]
    InvalidFormat,
    #[error("signature verification failed")]
    InvalidSignature,
    #[error("license has expired")]
    Expired,
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
