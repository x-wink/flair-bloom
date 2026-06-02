use super::*;
use crate::key_id::{KeyId, MouseButton};
use serde_json::json;

fn make_profile(rules: Vec<BurstRule>) -> Profile {
    Profile {
        schema_version: CURRENT_SCHEMA_VERSION,
        meta: ProfileMeta {
            name: "t".into(),
            created_at: 0,
            updated_at: 0,
            app_version: "0".into(),
        },
        rules,
        hotkeys: Hotkeys::default(),
        advanced: Advanced::default(),
    }
}

fn rule(id: &str, mode: BurstMode, trigger: KeyId, target: KeyId, interval: u32) -> BurstRule {
    BurstRule {
        id: id.into(),
        enabled: true,
        trigger_key: trigger,
        target_key: target,
        mode,
        stop_key: None,
        interval_ms: interval,
    }
}

fn kbd(vk: u32) -> KeyId {
    KeyId::Keyboard(vk)
}

#[test]
fn validate_accepts_empty_profile() {
    assert!(make_profile(vec![]).validate().is_ok());
}

#[test]
fn validate_accepts_interval_at_lower_bound() {
    let p = make_profile(vec![rule("r", BurstMode::Hold, kbd(0x41), kbd(0x42), 10)]);
    assert!(p.validate().is_ok());
}

#[test]
fn validate_accepts_interval_at_upper_bound() {
    let p = make_profile(vec![rule(
        "r",
        BurstMode::Hold,
        kbd(0x41),
        kbd(0x42),
        10000,
    )]);
    assert!(p.validate().is_ok());
}

#[test]
fn validate_rejects_interval_below_minimum() {
    let p = make_profile(vec![rule("r", BurstMode::Hold, kbd(0x41), kbd(0x42), 9)]);
    assert!(matches!(
        p.validate(),
        Err(ProfileError::InvalidInterval(9))
    ));
}

#[test]
fn validate_rejects_interval_above_maximum() {
    let p = make_profile(vec![rule(
        "r",
        BurstMode::Hold,
        kbd(0x41),
        kbd(0x42),
        10001,
    )]);
    assert!(matches!(
        p.validate(),
        Err(ProfileError::InvalidInterval(10001))
    ));
}

#[test]
fn validate_rejects_too_many_rules() {
    let rules = (0..=MAX_RULES)
        .map(|i| rule(&format!("r{i}"), BurstMode::Hold, kbd(0x41), kbd(0x42), 10))
        .collect();
    assert!(matches!(
        make_profile(rules).validate(),
        Err(ProfileError::TooManyRules)
    ));
}

#[test]
fn validate_accepts_max_rules() {
    let rules = (0..MAX_RULES)
        .map(|i| rule(&format!("r{i}"), BurstMode::Hold, kbd(0x41), kbd(0x42), 10))
        .collect();
    assert!(make_profile(rules).validate().is_ok());
}

#[test]
fn validate_for_mode_dd_allows_hold_with_same_key() {
    // Hold 模式 trigger == target 在 DD 模式合法
    let p = make_profile(vec![rule("h", BurstMode::Hold, kbd(0x51), kbd(0x51), 10)]);
    assert!(p.validate_for_mode(true).is_ok());
}

#[test]
fn validate_for_mode_dd_rejects_toggle_target_equals_trigger() {
    let p = make_profile(vec![rule("t", BurstMode::Toggle, kbd(0x46), kbd(0x46), 10)]);
    let err = p.validate_for_mode(true).unwrap_err();
    assert!(matches!(err, ProfileError::DdTargetEqualsTrigger(ref id) if id == "t"));
}

#[test]
fn validate_for_mode_dd_rejects_toggle_target_equals_stop_key() {
    let mut r = rule("t", BurstMode::Toggle, kbd(0x46), kbd(0x47), 10);
    r.stop_key = Some(kbd(0x47));
    let p = make_profile(vec![r]);
    let err = p.validate_for_mode(true).unwrap_err();
    assert!(matches!(err, ProfileError::DdTargetEqualsStop(ref id) if id == "t"));
}

#[test]
fn validate_for_mode_dd_skips_disabled_rules() {
    let mut r = rule("t", BurstMode::Toggle, kbd(0x46), kbd(0x46), 10);
    r.enabled = false;
    let p = make_profile(vec![r]);
    assert!(p.validate_for_mode(true).is_ok());
}

#[test]
fn validate_default_mode_allows_toggle_target_equals_trigger() {
    // 非 DD 模式（distinct_target = false）允许 toggle target == trigger
    let p = make_profile(vec![rule("t", BurstMode::Toggle, kbd(0x46), kbd(0x46), 10)]);
    assert!(p.validate().is_ok());
}

#[test]
fn validate_for_mode_dd_allows_mouse_x1_target() {
    let p = make_profile(vec![rule(
        "m",
        BurstMode::Hold,
        kbd(0x51),
        KeyId::Mouse(MouseButton::X1),
        10,
    )]);
    assert!(p.validate_for_mode(true).is_ok());
}

#[test]
fn validate_for_mode_dd_allows_mouse_x2_target() {
    let p = make_profile(vec![rule(
        "m",
        BurstMode::Hold,
        kbd(0x51),
        KeyId::Mouse(MouseButton::X2),
        10,
    )]);
    assert!(p.validate_for_mode(true).is_ok());
}

#[test]
fn validate_for_mode_dd_allows_mouse_left_target() {
    let p = make_profile(vec![rule(
        "m",
        BurstMode::Hold,
        kbd(0x51),
        KeyId::Mouse(MouseButton::Left),
        10,
    )]);
    assert!(p.validate_for_mode(true).is_ok());
}

#[test]
fn validate_for_mode_dd_allows_wheel_targets() {
    let p = make_profile(vec![
        rule(
            "wheel-up",
            BurstMode::Hold,
            kbd(0x51),
            KeyId::Mouse(MouseButton::WheelUp),
            10,
        ),
        rule(
            "wheel-down",
            BurstMode::Toggle,
            kbd(0x52),
            KeyId::Mouse(MouseButton::WheelDown),
            10,
        ),
    ]);
    assert!(p.validate_for_mode(true).is_ok());
}

#[test]
fn validate_for_mode_dd_allows_mouse_x1_as_trigger() {
    // X1 作为触发键合法（hook 端识别），仅 target 受限
    let p = make_profile(vec![rule(
        "m",
        BurstMode::Hold,
        KeyId::Mouse(MouseButton::X1),
        kbd(0x51),
        10,
    )]);
    assert!(p.validate_for_mode(true).is_ok());
}

#[test]
fn burst_rule_serializes_keyid_shape() {
    let r = rule(
        "x",
        BurstMode::Hold,
        kbd(0x51),
        KeyId::Mouse(MouseButton::Left),
        10,
    );
    let v = serde_json::to_value(&r).unwrap();
    assert_eq!(v["trigger_key"], json!({"kind":"keyboard","code":81}));
    assert_eq!(v["target_key"], json!({"kind":"mouse","code":"left"}));
}
