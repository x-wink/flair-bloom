use super::kid::key_id;
use super::*;
use ed25519_dalek::{Signer, SigningKey};

// 用 billing 同款 jose + computeKeyId 在 Node 里签出的夹具，钉住两边的编码与 kid 算法一致
const JOSE_X: &str = "J0NpYa9tMlidMrNU-zjs9rX6CMtKwf2FE-tNkff6blY";
const JOSE_KID: &str = "ENxkMTW3WsC9NuIr7J-2zE";
const JOSE_IAT: u64 = 1_793_000_000;
const JOSE_PERMANENT: &str = "eyJhbGciOiJFZERTQSIsImtpZCI6IkVOeGtNVFczV3NDOU51SXI3Si0yekUiLCJ0eXAiOiJ4d2luay1saWNlbnNlK2p3dCJ9.eyJlbnQiOlsiZmxhaXIuc2lnbmF0dXJlIl0sImlzcyI6ImJpbGxpbmciLCJhdWQiOiJmbGFpciIsImp0aSI6ImxpY18wMUpGSVhUVVJFIiwiaWF0IjoxNzkzMDAwMDAwfQ.B71GtOlUjBarI92suQMbLt6EwJbQgPTDKwHNH5iXHogAYou_-QqR7_5K93KiksCltY36upTLvFHe9EoTtkd9Cw";
const JOSE_EXPIRING: &str = "eyJhbGciOiJFZERTQSIsImtpZCI6IkVOeGtNVFczV3NDOU51SXI3Si0yekUiLCJ0eXAiOiJ4d2luay1saWNlbnNlK2p3dCJ9.eyJlbnQiOlsiZmxhaXIuc2lnbmF0dXJlIl0sImlzcyI6ImJpbGxpbmciLCJhdWQiOiJmbGFpciIsImp0aSI6ImxpY18wMUpGSVhUVVJFMiIsImlhdCI6MTc5MzAwMDAwMCwiZXhwIjoxNzkzMDg2NDAwfQ.jfTs0zpyUb8hoJ1rLoLyn8wYIO6U3qEcKOLCcsQHpnEUPMQgZLiY-2xxqkQcJW7cg1yME5yMxjw6X0zbyPuvBA";

const NOW: u64 = 1_793_100_000;

fn jose_key() -> [u8; 32] {
    URL_SAFE_NO_PAD.decode(JOSE_X).unwrap().try_into().unwrap()
}

fn signing_key() -> SigningKey {
    SigningKey::from_bytes(&[7u8; 32])
}

fn test_kid() -> String {
    key_id(&signing_key().verifying_key().to_bytes())
}

fn b64(json: &serde_json::Value) -> String {
    URL_SAFE_NO_PAD.encode(serde_json::to_vec(json).unwrap())
}

fn sign(header: serde_json::Value, claims: serde_json::Value) -> String {
    let input = format!("{}.{}", b64(&header), b64(&claims));
    let sig = signing_key().sign(input.as_bytes());
    format!("{input}.{}", URL_SAFE_NO_PAD.encode(sig.to_bytes()))
}

fn header() -> serde_json::Value {
    serde_json::json!({ "alg": "EdDSA", "typ": "xwink-license+jwt", "kid": test_kid() })
}

fn claims() -> serde_json::Value {
    serde_json::json!({
        "iss": "billing",
        "aud": "flair",
        "jti": "lic_1",
        "ent": ["flair.signature"],
        "iat": NOW - 60,
    })
}

fn with(mut base: serde_json::Value, key: &str, value: serde_json::Value) -> serde_json::Value {
    base[key] = value;
    base
}

fn verify(token: &str, now: u64) -> Result<License, LicenseError> {
    let kid = test_kid();
    let keys = [(kid.as_str(), signing_key().verifying_key().to_bytes())];
    verify_with_keys(token, now, &keys)
}

#[test]
fn kid_matches_billing_algorithm() {
    assert_eq!(key_id(&jose_key()), JOSE_KID);
}

#[test]
fn jose_permanent_license_verifies() {
    let keys = [(JOSE_KID, jose_key())];
    let license = verify_with_keys(JOSE_PERMANENT, NOW, &keys).unwrap();
    assert_eq!(
        license,
        License {
            id: "lic_01JFIXTURE".into(),
            entitlements: vec![ENT_SIGNATURE.into()],
            issued_at: JOSE_IAT,
            expires_at: None,
        }
    );
    assert!(license.grants(ENT_SIGNATURE));
    assert!(!license.grants("flair.other"));
}

#[test]
fn jose_expiring_license_honours_exp() {
    let keys = [(JOSE_KID, jose_key())];
    let exp = JOSE_IAT + 86_400;
    let license = verify_with_keys(JOSE_EXPIRING, exp - 1, &keys).unwrap();
    assert_eq!(license.expires_at, Some(exp));
    assert_eq!(
        verify_with_keys(JOSE_EXPIRING, exp, &keys),
        Err(LicenseError::Expired)
    );
}

#[test]
fn embedded_keys_reject_unknown_kid() {
    // 测试构建只编入开发公钥，jose 夹具的 kid 不在其中
    assert_eq!(
        verify_license(JOSE_PERMANENT, NOW),
        Err(LicenseError::UnknownKey)
    );
}

#[test]
fn surrounding_whitespace_is_ignored() {
    let token = format!("  {}\n", sign(header(), claims()));
    assert!(verify(&token, NOW).is_ok());
}

#[test]
fn aud_array_containing_flair_is_accepted() {
    let token = sign(
        header(),
        with(claims(), "aud", serde_json::json!(["chat", "flair"])),
    );
    assert!(verify(&token, NOW).is_ok());
}

#[test]
fn missing_ent_means_no_entitlements() {
    let mut c = claims();
    c.as_object_mut().unwrap().remove("ent");
    let license = verify(&sign(header(), c), NOW).unwrap();
    assert!(!license.grants(ENT_SIGNATURE));
}

#[test]
fn wrong_audience_is_rejected() {
    for aud in [serde_json::json!("chat"), serde_json::json!(["chat"])] {
        let token = sign(header(), with(claims(), "aud", aud));
        assert_eq!(verify(&token, NOW), Err(LicenseError::WrongAudience));
    }
}

#[test]
fn wrong_issuer_is_rejected() {
    let token = sign(header(), with(claims(), "iss", serde_json::json!("uc")));
    assert_eq!(verify(&token, NOW), Err(LicenseError::WrongIssuer));
}

#[test]
fn entitlement_credential_type_is_rejected() {
    let token = sign(
        with(header(), "typ", serde_json::json!("xwink-entitlement+jwt")),
        claims(),
    );
    assert_eq!(verify(&token, NOW), Err(LicenseError::WrongType));
}

#[test]
fn missing_type_is_rejected() {
    let mut h = header();
    h.as_object_mut().unwrap().remove("typ");
    assert_eq!(
        verify(&sign(h, claims()), NOW),
        Err(LicenseError::WrongType)
    );
}

#[test]
fn other_algorithm_is_rejected() {
    let token = sign(with(header(), "alg", serde_json::json!("ES256")), claims());
    assert_eq!(verify(&token, NOW), Err(LicenseError::UnsupportedAlgorithm));
}

#[test]
fn unknown_kid_is_rejected() {
    let token = sign(with(header(), "kid", serde_json::json!("nope")), claims());
    assert_eq!(verify(&token, NOW), Err(LicenseError::UnknownKey));
}

#[test]
fn tampered_claims_fail_signature() {
    let token = sign(header(), claims());
    let mut parts: Vec<&str> = token.split('.').collect();
    let forged = b64(&with(claims(), "jti", serde_json::json!("lic_forged")));
    parts[1] = &forged;
    assert_eq!(
        verify(&parts.join("."), NOW),
        Err(LicenseError::InvalidSignature)
    );
}

#[test]
fn signature_from_another_key_fails() {
    let keys = [(JOSE_KID, jose_key())];
    let token = sign(with(header(), "kid", serde_json::json!(JOSE_KID)), claims());
    assert_eq!(
        verify_with_keys(&token, NOW, &keys),
        Err(LicenseError::InvalidSignature)
    );
}

#[test]
fn iat_within_leeway_is_accepted() {
    let token = sign(header(), with(claims(), "iat", (NOW + 3600).into()));
    assert!(verify(&token, NOW).is_ok());
}

#[test]
fn iat_beyond_leeway_is_clock_rollback() {
    let token = sign(
        header(),
        with(claims(), "iat", (NOW + IAT_LEEWAY_SECS + 1).into()),
    );
    assert_eq!(verify(&token, NOW), Err(LicenseError::ClockRollback));
}

#[test]
fn expired_license_is_rejected() {
    let token = sign(header(), with(claims(), "exp", NOW.into()));
    assert_eq!(verify(&token, NOW), Err(LicenseError::Expired));
    assert!(verify(&token, NOW - 1).is_ok());
}

#[test]
fn missing_required_claim_is_invalid_format() {
    let mut c = claims();
    c.as_object_mut().unwrap().remove("jti");
    assert_eq!(
        verify(&sign(header(), c), NOW),
        Err(LicenseError::InvalidFormat)
    );
}

#[test]
fn malformed_tokens_are_invalid_format() {
    let valid = sign(header(), claims());
    let (head, rest) = valid.split_once('.').unwrap();
    let (body, _) = rest.split_once('.').unwrap();
    let cases = [
        String::new(),
        "a.b".into(),
        format!("{valid}.extra"),
        format!("!!!.{body}.sig"),
        format!("{}.{body}.sig", URL_SAFE_NO_PAD.encode(b"not json")),
        format!("{head}.{body}.{}", URL_SAFE_NO_PAD.encode([0u8; 10])),
        format!("{head}.{body}.***"),
    ];
    for token in cases {
        assert_eq!(
            verify(&token, NOW),
            Err(LicenseError::InvalidFormat),
            "{token}"
        );
    }
}

#[test]
fn dev_sample_verifies_with_embedded_dev_keys() {
    let license = verify_license(include_str!("../license-dev-sample.jws"), NOW).unwrap();
    assert!(license.grants(ENT_SIGNATURE));
}
