use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MigrateError {
    #[error("unknown schema version {0}")]
    UnknownVersion(u32),
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
