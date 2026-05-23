use migrate::{run_migrations, MigrateError};
use serde_json::Value;

use crate::profile::CURRENT_SCHEMA_VERSION;

pub fn migrate_profile(data: Value, from: u32) -> Result<Value, MigrateError> {
    run_migrations(data, from, CURRENT_SCHEMA_VERSION, migrate_step)
}

fn migrate_step(_data: Value, from_version: u32) -> Result<Value, MigrateError> {
    // v1 is the initial version — no migrations needed yet
    Err(MigrateError::UnknownVersion(from_version))
}
