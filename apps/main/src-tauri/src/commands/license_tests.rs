use super::*;

// 本地开发密钥签的永久许可证，只在 debug 构建里验得过（crypto 的 license-dev-keys.json）
const DEV_SAMPLE: &str = include_str!("../../../../../packages/crypto/license-dev-sample.jws");

fn state(tokens: &[&str]) -> LicenseState {
    LicenseState(Mutex::new(tokens.iter().map(|t| t.to_string()).collect()))
}

fn license(id: &str, ents: &[&str], issued_at: u64, expires_at: Option<u64>) -> License {
    License {
        id: id.into(),
        entitlements: ents.iter().map(|e| e.to_string()).collect(),
        issued_at,
        expires_at,
    }
}

#[test]
fn dev_sample_grants_signature() {
    let s = state(&[DEV_SAMPLE]);
    assert!(s.grants(crypto::ENT_SIGNATURE));
    assert!(!s.grants("flair.other"));
    assert_eq!(
        summarize(s.current()).licenses,
        vec![LicenseStatus {
            id: "lic_flair_dev_sample".into(),
            entitlements: vec![crypto::ENT_SIGNATURE.into()],
            issued_at: 1_793_000_000,
            expires_at: None,
        }]
    );
}

#[test]
fn trailing_newline_in_file_is_tolerated() {
    assert!(state(&[&format!("{DEV_SAMPLE}\n")]).grants(crypto::ENT_SIGNATURE));
}

#[test]
fn no_license_grants_nothing() {
    assert!(state(&[]).current().is_empty());
    assert!(!state(&[]).grants(crypto::ENT_SIGNATURE));
}

#[test]
fn invalid_license_is_skipped_but_valid_one_still_counts() {
    assert!(!state(&["not.a.license"]).grants(crypto::ENT_SIGNATURE));
    let s = state(&["not.a.license", DEV_SAMPLE]);
    assert_eq!(s.current().len(), 1);
    assert!(s.grants(crypto::ENT_SIGNATURE));
}

#[test]
fn summarize_unions_entitlements_and_keeps_latest_expiry() {
    let summary = summarize(vec![
        license("b", &["flair.signature", "flair.x"], 20, Some(500)),
        license("a", &["flair.signature"], 10, Some(300)),
        license("c", &["flair.x"], 30, None),
    ]);
    assert_eq!(
        summary.entitlements,
        vec![
            EntitlementGrant {
                key: "flair.signature".into(),
                expires_at: Some(500)
            },
            EntitlementGrant {
                key: "flair.x".into(),
                expires_at: None
            },
        ]
    );
    let ids: Vec<&str> = summary.licenses.iter().map(|l| l.id.as_str()).collect();
    assert_eq!(ids, ["a", "b", "c"]);
}

#[test]
fn summarize_permanent_wins_regardless_of_order() {
    let summary = summarize(vec![
        license("a", &["flair.signature"], 10, None),
        license("b", &["flair.signature"], 20, Some(500)),
    ]);
    assert_eq!(summary.entitlements[0].expires_at, None);
}

#[test]
fn code_file_name_normalizes_case_dashes_and_spaces() {
    let a = code_file_name("UR5Q-XMHD-Z38F-PDAB").unwrap();
    assert_eq!(code_file_name(" ur5qxmhdz38fpdab\n").unwrap(), a);
    assert_ne!(code_file_name("62L6F7HZMBX8CWZG").unwrap(), a);
    assert!(a.ends_with(".jws") && a.len() == 16 + 4);
    assert_eq!(code_file_name(" - "), None);
}

#[test]
fn read_tokens_reads_only_license_files_in_name_order() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("b.jws"), "second").unwrap();
    std::fs::write(dir.path().join("a.jws"), "first").unwrap();
    std::fs::write(dir.path().join("a.jws.tmp"), "half").unwrap();
    std::fs::write(dir.path().join("notes.txt"), "x").unwrap();
    assert_eq!(read_tokens(dir.path()), ["first", "second"]);
    assert!(read_tokens(&dir.path().join("missing")).is_empty());
}

fn billing_error(status: u16, code: &str, extra: serde_json::Value) -> ActivationError {
    let mut details = serde_json::json!({ "code": code });
    if let Some(map) = extra.as_object() {
        for (k, v) in map {
            details[k] = v.clone();
        }
    }
    let body = serde_json::json!({
        "success": false,
        "error": { "code": "CONFLICT", "message": "说明", "statusCode": status, "traceId": "t-1", "details": details },
    });
    map_activation_error(status, None, &serde_json::to_vec(&body).unwrap())
}

#[test]
fn business_codes_map_to_kinds() {
    let none = serde_json::Value::Null;
    assert_eq!(
        billing_error(404, "REDEMPTION_CODE_NOT_FOUND", none.clone()),
        ActivationError::InvalidCode
    );
    assert_eq!(
        billing_error(409, "LICENSE_CODE_TYPE_MISMATCH", none.clone()),
        ActivationError::InvalidCode
    );
    assert_eq!(
        billing_error(410, "REDEMPTION_CODE_VOID", none.clone()),
        ActivationError::Void
    );
    assert_eq!(
        billing_error(410, "REDEMPTION_CODE_EXPIRED", none.clone()),
        ActivationError::Expired
    );
    assert_eq!(
        billing_error(409, "LICENSE_REISSUE_LIMIT", none.clone()),
        ActivationError::ReissueLimit
    );
    assert_eq!(
        billing_error(410, "LICENSE_EXPIRED", none.clone()),
        ActivationError::LicenseExpired
    );
    assert_eq!(
        billing_error(503, "LICENSE_APP_UNAVAILABLE", none.clone()),
        ActivationError::Unavailable
    );
    assert_eq!(
        billing_error(503, "AGREEMENT_UNAVAILABLE", none),
        ActivationError::Unavailable
    );
}

#[test]
fn agreement_errors_carry_current_version() {
    for code in ["AGREEMENT_VERSION_REQUIRED", "AGREEMENT_VERSION_OUTDATED"] {
        assert_eq!(
            billing_error(409, code, serde_json::json!({ "currentVersion": "1.0.0" })),
            ActivationError::AgreementChanged {
                current_version: Some("1.0.0".into())
            }
        );
    }
}

#[test]
fn unknown_code_keeps_message_and_trace_id() {
    assert_eq!(
        billing_error(409, "LICENSE_MISMATCH", serde_json::Value::Null),
        ActivationError::Failed {
            message: "说明".into(),
            trace_id: Some("t-1".into())
        }
    );
}

#[test]
fn rate_limit_reads_retry_after() {
    assert_eq!(
        map_activation_error(429, Some("60"), b""),
        ActivationError::RateLimited {
            retry_after_secs: Some(60)
        }
    );
    assert_eq!(
        map_activation_error(429, Some("Wed, 21 Oct 2026 07:28:00 GMT"), b""),
        ActivationError::RateLimited {
            retry_after_secs: None
        }
    );
}

#[test]
fn gateway_errors_without_body_are_unavailable() {
    assert_eq!(
        map_activation_error(502, None, b"<html>"),
        ActivationError::Unavailable
    );
    assert_eq!(
        map_activation_error(500, None, b""),
        ActivationError::Failed {
            message: "HTTP 500".into(),
            trace_id: None
        }
    );
}

#[test]
fn activation_error_serializes_with_kind_tag() {
    assert_eq!(
        serde_json::to_value(ActivationError::RateLimited {
            retry_after_secs: Some(60)
        })
        .unwrap(),
        serde_json::json!({ "kind": "rate_limited", "retry_after_secs": 60 })
    );
    assert_eq!(
        serde_json::to_value(ActivationError::InvalidCode).unwrap(),
        serde_json::json!({ "kind": "invalid_code" })
    );
}

#[test]
fn parse_activation_takes_license() {
    let body = serde_json::json!({
        "licenseId": "lic_1", "license": "a.b.c", "reissueCount": 0, "reissueLimit": 3,
    });
    assert_eq!(
        parse_activation(&serde_json::to_vec(&body).unwrap()).unwrap(),
        "a.b.c"
    );
    assert!(matches!(
        parse_activation(b"{}"),
        Err(ActivationError::Failed { .. })
    ));
}

#[test]
fn pick_agreement_finds_membership() {
    let body = serde_json::json!([
        { "code": "privacy", "title": "隐私政策", "version": "2.0" },
        { "code": "membership", "title": "会员服务协议", "version": "0.1.0-draft", "minor": false, "content": "…" },
    ]);
    assert_eq!(
        pick_agreement(&serde_json::to_vec(&body).unwrap()),
        Some(Some(MembershipAgreement {
            title: "会员服务协议".into(),
            version: "0.1.0-draft".into()
        }))
    );
    assert_eq!(pick_agreement(b"[]"), Some(None));
    // 解析不了与「没有已发布的协议」必须分开，前者不能静默当成后者
    assert_eq!(pick_agreement(b"not json"), None);
    assert_eq!(pick_agreement(br#"{"data":[]}"#), None);
}

#[test]
fn write_license_file_replaces_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("licenses");
    write_license_file(&nested, "k.jws", "first").unwrap();
    write_license_file(&nested, "k.jws", DEV_SAMPLE).unwrap();
    assert_eq!(
        std::fs::read_to_string(nested.join("k.jws")).unwrap(),
        DEV_SAMPLE
    );
    assert!(!nested.join("k.jws.tmp").exists());
}

#[test]
fn set_switches_current_licenses() {
    let s = state(&[]);
    s.set(vec![DEV_SAMPLE.to_string()]);
    assert!(s.grants(crypto::ENT_SIGNATURE));
}

#[test]
fn clock_skew_serializes_as_its_own_kind() {
    assert_eq!(
        serde_json::to_value(ActivationError::ClockSkew).unwrap(),
        serde_json::json!({ "kind": "clock_skew" })
    );
}

#[test]
fn already_activated_serializes_with_license() {
    let err = ActivationError::AlreadyActivated { license: None };
    assert_eq!(
        serde_json::to_value(err).unwrap(),
        serde_json::json!({ "kind": "already_activated", "license": null })
    );
}

#[test]
fn already_activated_detects_existing_code_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("k.jws");
    assert_eq!(already_activated(&path), None);

    std::fs::write(&path, DEV_SAMPLE).unwrap();
    match already_activated(&path) {
        Some(ActivationError::AlreadyActivated { license: Some(l) }) => {
            assert_eq!(l.id, "lic_flair_dev_sample")
        }
        other => panic!("{other:?}"),
    }

    // 文件坏了也算激活过：照样先问，覆盖正好修好它
    std::fs::write(&path, "broken").unwrap();
    assert_eq!(
        already_activated(&path),
        Some(ActivationError::AlreadyActivated { license: None })
    );
}

/// 打 xwink-console 本地起的 uc 与 billing 走一遍真实契约（`pnpm db:up && pnpm service:all`）。
/// 会消耗一张本地测试码，码从环境变量给、不入库：
/// `FLAIR_TEST_LICENSE_CODE=<码> cargo test -p flair-bloom live_activation -- --ignored`
#[test]
#[ignore = "需要本地 uc / billing 与一张未用过的测试码"]
fn live_activation_against_local_billing() {
    let code = std::env::var("FLAIR_TEST_LICENSE_CODE").expect("缺 FLAIR_TEST_LICENSE_CODE");
    tauri::async_runtime::block_on(async {
        let agreement = get_membership_agreement()
            .await
            .unwrap()
            .expect("本地 uc 应发布着协议");
        assert!(!agreement.version.is_empty());

        assert_eq!(
            request_license("AAAAAAAAAAAAAAAA", Some(agreement.version.clone()))
                .await
                .unwrap_err(),
            ActivationError::InvalidCode
        );
        assert_eq!(
            request_license(&code, None).await.unwrap_err(),
            ActivationError::AgreementChanged {
                current_version: Some(agreement.version.clone())
            }
        );

        let (token, first) = request_license(&code, Some(agreement.version.clone()))
            .await
            .unwrap();
        let first = first.unwrap();
        assert!(first.grants(crypto::ENT_SIGNATURE));
        assert_eq!(first.expires_at, None);
        assert_eq!(crypto::verify_license(&token, now_secs()).unwrap(), first);

        // 同码再激活是重下发：同一张许可证 id、重新签发
        let (_, again) = request_license(&code, Some(agreement.version))
            .await
            .unwrap();
        let again = again.unwrap();
        assert_eq!(again.id, first.id);
    });
}
