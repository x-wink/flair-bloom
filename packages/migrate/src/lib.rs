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
#[path = "lib_tests.rs"]
mod tests;
