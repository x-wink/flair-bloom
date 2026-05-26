//! 加密原语：AES-256-GCM 对称加密（[`aes`]）与 Ed25519 离线许可证校验（[`license`]）。
//!
//! 主程序所有需要落盘加密或许可证校验的代码都从这里取，不要直接依赖底层的
//! `aes_gcm` / `ed25519_dalek` crate，否则参数与错误形态会漂移。
#![deny(missing_docs)]

pub mod aes;
pub mod license;

pub use aes::{decrypt, encrypt, CryptoError};
pub use license::{verify_license, LicenseError, LicensePayload};
