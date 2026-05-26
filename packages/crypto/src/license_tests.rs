use super::*;

#[test]
fn empty_code_is_invalid_format() {
    assert!(matches!(
        verify_license("", 0),
        Err(LicenseError::InvalidFormat)
    ));
}

#[test]
fn non_base32_chars_are_invalid_format() {
    // Base32 RFC4648 字母表不含 '0' '1' '8' '9',含会解码失败
    assert!(matches!(
        verify_license("QZHUA-00000-11111-88888-99999", 0),
        Err(LicenseError::InvalidFormat)
    ));
}

#[test]
fn too_short_payload_is_invalid_format() {
    // 仅几字节 Base32,解码后 < 64 字节
    assert!(matches!(
        verify_license("AB", 0),
        Err(LicenseError::InvalidFormat)
    ));
}

#[test]
fn random_64_byte_payload_fails_signature() {
    // 64 个有效 Base32 字符 → 解码 40 字节,仍 < 64,InvalidFormat;
    // 但若给足够长的合法 base32 内容,签名校验必失败(占位公钥)
    let s: String = std::iter::repeat_n('A', 120).collect();
    let err = verify_license(&s, 0).unwrap_err();
    assert!(matches!(
        err,
        LicenseError::InvalidSignature | LicenseError::InvalidFormat
    ));
}
