use super::*;

// 本地开发密钥签的永久许可证，只在 debug 构建里验得过（crypto 的 license-dev-keys.json）
const DEV_SAMPLE: &str = include_str!("../../../../../packages/crypto/license-dev-sample.jws");

fn state(token: Option<&str>) -> LicenseState {
    LicenseState(Mutex::new(token.map(str::to_string)))
}

#[test]
fn dev_sample_grants_signature() {
    let s = state(Some(DEV_SAMPLE));
    assert!(s.grants(crypto::ENT_SIGNATURE));
    assert!(!s.grants("flair.other"));
    assert_eq!(
        s.current().map(LicenseStatus::from),
        Some(LicenseStatus {
            id: "lic_flair_dev_sample".into(),
            entitlements: vec![crypto::ENT_SIGNATURE.into()],
            issued_at: 1_793_000_000,
            expires_at: None,
        })
    );
}

#[test]
fn trailing_newline_in_file_is_tolerated() {
    let s = state(Some(&format!("{DEV_SAMPLE}\n")));
    assert!(s.grants(crypto::ENT_SIGNATURE));
}

#[test]
fn no_license_grants_nothing() {
    assert!(state(None).current().is_none());
    assert!(!state(None).grants(crypto::ENT_SIGNATURE));
}

#[test]
fn invalid_license_grants_nothing() {
    let s = state(Some("not.a.license"));
    assert!(s.current().is_none());
    assert!(!s.grants(crypto::ENT_SIGNATURE));
}
