//! AES-256-GCM 对称加解密。
//!
//! 主密钥由编译期常量注入（发布前需替换为 build-script 注入的真实密钥），
//! 经 HKDF-SHA256 派生为 32 字节会话密钥；nonce 每次随机生成、随密文一并落盘。

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use hkdf::Hkdf;
use rand::RngCore;
use sha2::Sha256;
use thiserror::Error;

const APP_SALT: &[u8] = b"FlairBloom-AES256-GCM-v1";
// TODO(security): replace with build-script-injected key before release
const MASTER_KEY: &[u8; 32] = b"FlairBloom_MasterKey_Placeholder";

/// AES-256-GCM 加解密失败。错误信息刻意不暴露细节，避免给攻击者提供 oracle。
#[derive(Debug, Error)]
pub enum CryptoError {
    /// 加密阶段失败（极罕见，通常意味着 nonce 生成或底层密码学库出错）。
    #[error("encryption failed")]
    Encrypt,
    /// 解密失败：密文/Nonce/AAD 不匹配，或文件被篡改/损坏。
    #[error("decryption failed: data may be corrupted or tampered")]
    Decrypt,
}

fn derive_key() -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(Some(APP_SALT), MASTER_KEY);
    let mut key = [0u8; 32];
    hk.expand(b"aes-gcm-key", &mut key)
        .expect("HKDF expand failed");
    key
}

/// Encrypt `plaintext` with AES-256-GCM. Returns (ciphertext_with_tag, nonce).
pub fn encrypt(plaintext: &[u8], aad: &[u8]) -> Result<(Vec<u8>, [u8; 12]), CryptoError> {
    let key = derive_key();
    let cipher = Aes256Gcm::new(key.as_ref().into());
    let mut nonce_bytes = [0u8; 12];
    rand::rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(
            nonce,
            aes_gcm::aead::Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| CryptoError::Encrypt)?;
    Ok((ciphertext, nonce_bytes))
}

/// Decrypt AES-256-GCM `ciphertext` (includes auth tag).
pub fn decrypt(ciphertext: &[u8], nonce: &[u8; 12], aad: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let key = derive_key();
    let cipher = Aes256Gcm::new(key.as_ref().into());
    let nonce = Nonce::from_slice(nonce);
    cipher
        .decrypt(
            nonce,
            aes_gcm::aead::Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| CryptoError::Decrypt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_recovers_plaintext() {
        let plaintext = b"hello flair bloom";
        let aad = b"unit-test-aad";
        let (ciphertext, nonce) = encrypt(plaintext, aad).expect("encrypt ok");
        let recovered = decrypt(&ciphertext, &nonce, aad).expect("decrypt ok");
        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn round_trip_handles_empty_plaintext() {
        let aad = b"aad";
        let (ciphertext, nonce) = encrypt(b"", aad).expect("encrypt ok");
        // GCM 即使空消息也会产出 16 字节 tag
        assert_eq!(ciphertext.len(), 16);
        let recovered = decrypt(&ciphertext, &nonce, aad).expect("decrypt ok");
        assert!(recovered.is_empty());
    }

    #[test]
    fn each_encryption_uses_random_nonce() {
        // 同一明文加密两次应产生不同 nonce(随机) 与不同密文
        let (c1, n1) = encrypt(b"same", b"aad").unwrap();
        let (c2, n2) = encrypt(b"same", b"aad").unwrap();
        assert_ne!(n1, n2);
        assert_ne!(c1, c2);
    }

    #[test]
    fn decrypt_fails_when_aad_tampered() {
        let (ciphertext, nonce) = encrypt(b"payload", b"original-aad").unwrap();
        let err = decrypt(&ciphertext, &nonce, b"tampered-aad").unwrap_err();
        assert!(matches!(err, CryptoError::Decrypt));
    }

    #[test]
    fn decrypt_fails_when_nonce_tampered() {
        let (ciphertext, mut nonce) = encrypt(b"payload", b"aad").unwrap();
        nonce[0] ^= 0xFF;
        assert!(matches!(
            decrypt(&ciphertext, &nonce, b"aad"),
            Err(CryptoError::Decrypt)
        ));
    }

    #[test]
    fn decrypt_fails_when_ciphertext_tampered() {
        let (mut ciphertext, nonce) = encrypt(b"payload", b"aad").unwrap();
        ciphertext[0] ^= 0xFF;
        assert!(matches!(
            decrypt(&ciphertext, &nonce, b"aad"),
            Err(CryptoError::Decrypt)
        ));
    }

    #[test]
    fn decrypt_fails_on_truncated_ciphertext() {
        // 截掉认证 tag 必失败
        let (mut ciphertext, nonce) = encrypt(b"payload", b"aad").unwrap();
        ciphertext.truncate(ciphertext.len() - 16);
        assert!(decrypt(&ciphertext, &nonce, b"aad").is_err());
    }
}
