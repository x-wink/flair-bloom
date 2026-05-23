pub const MAGIC: &[u8; 4] = b"QZHU";
pub const VERSION: u8 = 0x01;

/// Serialized at the start of every .qzh file.
#[derive(Debug, Clone)]
pub struct FileHeader {
    pub magic: [u8; 4],
    pub version: u8,
    pub flags: u16,
    pub nonce: [u8; 12],
}

impl FileHeader {
    pub const SIZE: usize = 4 + 1 + 2 + 12; // 19 bytes

    pub fn new(nonce: [u8; 12]) -> Self {
        Self {
            magic: *MAGIC,
            version: VERSION,
            flags: 0,
            nonce,
        }
    }

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
