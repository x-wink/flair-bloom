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

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("encryption failed")]
    Encrypt,
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
