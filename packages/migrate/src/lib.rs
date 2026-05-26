//! 通用 schema 迁移运行器。
//!
//! 把"对单一版本号执行单步升级"的 closure 串联成"从 `from` 升到 `to`"的链式调用。
//! `qzh-format` 的 `.qzh` 配置迁移与 `tauri-plugin-store` 的 settings 迁移共用此基础设施。
#![deny(missing_docs)]

use serde_json::Value;
use thiserror::Error;

/// 迁移过程中的错误。
#[derive(Debug, Error)]
pub enum MigrateError {
    /// 遇到了未知或不支持的源版本号。
    #[error("unknown schema version {0}")]
    UnknownVersion(u32),
    /// 迁移过程中产生的 JSON 解析/序列化错误。
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Execute migration functions from `from` to `to` (exclusive).
/// `migrator` receives (data, current_version) and returns the upgraded data.
pub fn run_migrations<F>(
    data: Value,
    from: u32,
    to: u32,
    migrator: F,
) -> Result<Value, MigrateError>
where
    F: Fn(Value, u32) -> Result<Value, MigrateError>,
{
    let mut current = data;
    for version in from..to {
        current = migrator(current, version)?;
    }
    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn no_op_when_from_equals_to() {
        // from == to 应直接返回原值,不调用 migrator
        let called = std::cell::Cell::new(0u32);
        let out = run_migrations(json!({"v": 0}), 3, 3, |v, _| {
            called.set(called.get() + 1);
            Ok(v)
        })
        .unwrap();
        assert_eq!(called.get(), 0);
        assert_eq!(out, json!({"v": 0}));
    }

    #[test]
    fn invokes_migrator_for_each_version_in_order() {
        let calls = std::cell::RefCell::new(Vec::<u32>::new());
        let _ = run_migrations(json!(null), 1, 4, |v, version| {
            calls.borrow_mut().push(version);
            Ok(v)
        })
        .unwrap();
        // from=1, to=4 -> 调用 v=1, v=2, v=3
        assert_eq!(*calls.borrow(), vec![1, 2, 3]);
    }

    #[test]
    fn passes_data_through_each_step() {
        // migrator 累加一个 step 计数,验证 data 被正确链式传递
        let out = run_migrations(json!({"step": 0}), 0, 3, |mut v, _| {
            let cur = v["step"].as_u64().unwrap();
            v["step"] = json!(cur + 1);
            Ok(v)
        })
        .unwrap();
        assert_eq!(out, json!({"step": 3}));
    }

    #[test]
    fn propagates_migrator_error_and_stops() {
        let calls = std::cell::RefCell::new(0u32);
        let result = run_migrations(json!(null), 0, 5, |_, version| {
            *calls.borrow_mut() += 1;
            if version == 2 {
                Err(MigrateError::UnknownVersion(version))
            } else {
                Ok(json!(null))
            }
        });
        assert!(matches!(result, Err(MigrateError::UnknownVersion(2))));
        // 在 v=2 时失败,共调用了 0/1/2 三次
        assert_eq!(*calls.borrow(), 3);
    }
}
