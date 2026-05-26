use super::*;
use serde_json::json;

#[test]
fn migrate_from_current_is_noop() {
    // from == CURRENT_SCHEMA_VERSION 时不应调用 migrate_step，原值原样返回
    let v = json!({"hello": "world"});
    let out = migrate_profile(v.clone(), CURRENT_SCHEMA_VERSION).unwrap();
    assert_eq!(out, v);
}

#[test]
fn migrate_from_unknown_version_errors() {
    // 当前还没有任何迁移规则，from < CURRENT 必然落到 UnknownVersion 分支
    let err = migrate_profile(json!({}), 0).unwrap_err();
    assert!(matches!(err, MigrateError::UnknownVersion(0)));
}
