//! [`crate::profile::Profile`] 的 schema 迁移入口。
//!
//! 调度通用 [`migrate::run_migrations`] 把 JSON 形式的旧版 Profile 升级到当前版本。
//! 每个版本对应的具体改写规则集中在 `migrate_step` 内。

use migrate::{run_migrations, MigrateError};
use serde_json::Value;

use crate::profile::CURRENT_SCHEMA_VERSION;

/// 把 `data`（JSON 形式的 [`crate::profile::Profile`]）从 `from` 版本迁移到当前版本。
///
/// 内部委托给通用 [`migrate::run_migrations`] 调度，每一步由模块内的 `migrate_step` 处理。
pub fn migrate_profile(data: Value, from: u32) -> Result<Value, MigrateError> {
    run_migrations(data, from, CURRENT_SCHEMA_VERSION, migrate_step)
}

fn migrate_step(_data: Value, from_version: u32) -> Result<Value, MigrateError> {
    // v1 is the initial version — no migrations needed yet
    Err(MigrateError::UnknownVersion(from_version))
}

#[cfg(test)]
#[path = "migrate_tests.rs"]
mod tests;
