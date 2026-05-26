//! `.qzh` 文件二进制头部布局：4 字节魔数 + 1 字节版本 + 2 字节 flags + 12 字节 nonce。
//!
//! 头部的 `magic + version + flags`（共 7 字节）作为 AAD 喂给 AES-GCM，确保密文无法
//! 被「换头」攻击：任何头部位被改动都会导致解密失败。

/// `.qzh` 文件头部 4 字节魔数（ASCII "QZHU"）。
pub const MAGIC: &[u8; 4] = b"QZHU";
/// 当前文件头版本号。升级头部布局时必须递增并配合迁移逻辑。
pub const VERSION: u8 = 0x01;

/// Serialized at the start of every .qzh file.
#[derive(Debug, Clone)]
pub struct FileHeader {
    /// 4 字节魔数，恒为 [`MAGIC`]。
    pub magic: [u8; 4],
    /// 头部版本，恒为 [`VERSION`]。
    pub version: u8,
    /// 预留位标志，当前固定为 0。改动后必须同步迁移逻辑。
    pub flags: u16,
    /// AES-GCM 12 字节随机 nonce。每次写入都重新生成。
    pub nonce: [u8; 12],
}

impl FileHeader {
    /// 序列化后的字节大小（4 magic + 1 version + 2 flags + 12 nonce = 19）。
    pub const SIZE: usize = 4 + 1 + 2 + 12; // 19 bytes

    /// 用给定 nonce 构造一个当前版本的文件头，flags 置 0。
    pub fn new(nonce: [u8; 12]) -> Self {
        Self {
            magic: *MAGIC,
            version: VERSION,
            flags: 0,
            nonce,
        }
    }

    /// 把文件头按线缆字节序（little-endian flags）序列化为定长字节串。
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::SIZE);
        buf.extend_from_slice(&self.magic);
        buf.push(self.version);
        buf.extend_from_slice(&self.flags.to_le_bytes());
        buf.extend_from_slice(&self.nonce);
        buf
    }

    /// Returns None if magic or version mismatch.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::SIZE {
            return None;
        }
        if &bytes[0..4] != MAGIC {
            return None;
        }
        if bytes[4] != VERSION {
            return None;
        }
        let flags = u16::from_le_bytes([bytes[5], bytes[6]]);
        let mut nonce = [0u8; 12];
        nonce.copy_from_slice(&bytes[7..19]);
        Some(Self {
            magic: *MAGIC,
            version: VERSION,
            flags,
            nonce,
        })
    }

    /// The AAD passed to AES-GCM (covers magic + version + flags).
    pub fn aad(&self) -> Vec<u8> {
        let mut aad = Vec::with_capacity(7);
        aad.extend_from_slice(&self.magic);
        aad.push(self.version);
        aad.extend_from_slice(&self.flags.to_le_bytes());
        aad
    }
}

#[cfg(test)]
#[path = "header_tests.rs"]
mod tests;
