//! `.qzh` 配置文件格式：文件头（[`header`]）、Profile/规则数据结构（[`profile`]）、
//! 宏序列（[`macro_seq`]）以及 schema 迁移入口（[`migrate`]）。
//!
//! 加密与解密由 [`crypto`] crate 负责，本 crate 仅定义"明文 JSON 长什么样"以及
//! "二进制布局如何拼装"，二者通过 AAD 绑定（见 `header::FileHeader::aad`）。
#![deny(missing_docs)]

pub mod header;
pub mod key_id;
pub mod macro_seq;
pub mod migrate;
pub mod profile;

pub use key_id::{KeyId, MouseButton};
pub use profile::{Profile, ProfileError};
