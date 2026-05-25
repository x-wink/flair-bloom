use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const CURRENT_SCHEMA_VERSION: u32 = 1;
pub const MAX_RULES: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub schema_version: u32,
    pub meta: ProfileMeta,
    pub rules: Vec<BurstRule>,
    #[serde(default)]
    pub hotkeys: Hotkeys,
    #[serde(default)]
    pub advanced: Advanced,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileMeta {
    pub name: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub app_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurstRule {
    pub id: String,
    pub enabled: bool,
    pub trigger_key: u32,
    pub target_key: u32,
    pub mode: BurstMode,
    /// Toggle mode only: separate stop hotkey. Defaults to trigger_key when None.
    #[serde(default)]
    pub stop_key: Option<u32>,
    /// Milliseconds between simulated keypresses. Clamped to [10, 10000].
    pub interval_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BurstMode {
    /// Fire while key is held.
    #[default]
    Hold,
    /// Toggle on/off with hotkey.
    Toggle,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Hotkeys {
    pub global_toggle: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Advanced {
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_log_level() -> String {
    "info".to_string()
}

#[derive(Debug, Error)]
pub enum ProfileError {
    #[error("rule count exceeds maximum of {MAX_RULES}")]
    TooManyRules,
    #[error("rule interval {0}ms is out of range [10, 10000]")]
    InvalidInterval(u32),
    #[error("rule {0}: target_key must differ from trigger_key in DD mode")]
    DdTargetEqualsTrigger(String),
    #[error("rule {0}: target_key must differ from stop_key in DD mode")]
    DdTargetEqualsStop(String),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Crypto(#[from] crypto::CryptoError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("invalid file format")]
    InvalidFormat,
    #[error("schema version {0} is newer than supported {CURRENT_SCHEMA_VERSION}")]
    TooNew(u32),
    #[error(transparent)]
    Migrate(#[from] migrate::MigrateError),
}

impl Profile {
    pub fn validate(&self) -> Result<(), ProfileError> {
        self.validate_for_mode(false)
    }

    /// `distinct_target = true` 时启用 DD 模式专属约束：
    /// 因为 DD 后端无法在 dwExtraInfo 中写入过滤标记，注入事件会被自身 LL 钩子
    /// 误判为物理按键。规则：
    /// - 任何模式：`target_key != trigger_key`
    /// - Toggle 模式：`target_key != stop_key`（默认 `stop_key = trigger_key`）
    pub fn validate_for_mode(&self, distinct_target: bool) -> Result<(), ProfileError> {
        if self.rules.len() > MAX_RULES {
            return Err(ProfileError::TooManyRules);
        }
        for rule in &self.rules {
            let i = rule.interval_ms;
            if !(10..=10000).contains(&i) {
                return Err(ProfileError::InvalidInterval(i));
            }
            if distinct_target && rule.enabled {
                if rule.target_key == rule.trigger_key {
                    return Err(ProfileError::DdTargetEqualsTrigger(rule.id.clone()));
                }
                if rule.mode == BurstMode::Toggle {
                    let stop = rule.stop_key.unwrap_or(rule.trigger_key);
                    if rule.target_key == stop {
                        return Err(ProfileError::DdTargetEqualsStop(rule.id.clone()));
                    }
                }
            }
        }
        Ok(())
    }
}
