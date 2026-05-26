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
