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
mod tests {
    use super::*;

    #[test]
    fn to_bytes_emits_19_bytes_with_correct_layout() {
        let nonce = [9u8; 12];
        let bytes = FileHeader::new(nonce).to_bytes();
        assert_eq!(bytes.len(), FileHeader::SIZE);
        assert_eq!(&bytes[0..4], MAGIC);
        assert_eq!(bytes[4], VERSION);
        assert_eq!(&bytes[5..7], &0u16.to_le_bytes());
        assert_eq!(&bytes[7..19], &nonce);
    }

    #[test]
    fn from_bytes_round_trip_preserves_nonce() {
        let nonce: [u8; 12] = std::array::from_fn(|i| i as u8 + 1);
        let original = FileHeader::new(nonce);
        let bytes = original.to_bytes();
        let parsed = FileHeader::from_bytes(&bytes).expect("valid header");
        assert_eq!(parsed.magic, original.magic);
        assert_eq!(parsed.version, original.version);
        assert_eq!(parsed.flags, original.flags);
        assert_eq!(parsed.nonce, original.nonce);
    }

    #[test]
    fn from_bytes_returns_none_when_too_short() {
        assert!(FileHeader::from_bytes(&[]).is_none());
        assert!(FileHeader::from_bytes(&[0u8; FileHeader::SIZE - 1]).is_none());
    }

    #[test]
    fn from_bytes_rejects_bad_magic() {
        let mut bytes = FileHeader::new([0u8; 12]).to_bytes();
        bytes[0] = b'X';
        assert!(FileHeader::from_bytes(&bytes).is_none());
    }

    #[test]
    fn from_bytes_rejects_bad_version() {
        let mut bytes = FileHeader::new([0u8; 12]).to_bytes();
        bytes[4] = VERSION.wrapping_add(1);
        assert!(FileHeader::from_bytes(&bytes).is_none());
    }

    #[test]
    fn from_bytes_accepts_trailing_bytes() {
        // 真实使用场景：header 后面紧跟密文,from_bytes 不应因尾部数据报错
        let mut bytes = FileHeader::new([0u8; 12]).to_bytes();
        bytes.extend_from_slice(&[0xAA; 64]);
        assert!(FileHeader::from_bytes(&bytes).is_some());
    }

    #[test]
    fn aad_covers_magic_version_and_flags_but_not_nonce() {
        let h = FileHeader::new([7u8; 12]);
        let aad = h.aad();
        assert_eq!(aad.len(), 7);
        assert_eq!(&aad[0..4], MAGIC);
        assert_eq!(aad[4], VERSION);
        assert_eq!(&aad[5..7], &0u16.to_le_bytes());
        // 改 nonce 不应改变 aad,这是 AAD 设计的核心保证
        let h2 = FileHeader::new([8u8; 12]);
        assert_eq!(aad, h2.aad());
    }
}
