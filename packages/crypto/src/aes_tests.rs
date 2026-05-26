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
