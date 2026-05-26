//! Profile 数据结构：连发规则、元信息、热键与高级选项。
//!
//! 这是 `.qzh` 文件解密后的明文 JSON 形态。落盘时由 [`crate::header::FileHeader`]
//! 与 [`crypto`] 包装为加密二进制。

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 当前 [`Profile`] 的 schema 版本号。
///
/// 升级配置数据结构时必须递增并同步 `migrate::migrate_step`，否则旧文件无法升级。
pub const CURRENT_SCHEMA_VERSION: u32 = 1;
/// 单个 [`Profile`] 允许的最大规则数量，超出会在 [`Profile::validate`] 阶段被拒绝。
pub const MAX_RULES: usize = 64;

/// 一份完整的连发配置文件。落盘时序列化为 JSON，再由 [`crate::header::FileHeader`] 与
/// `crypto` 包装为 `.qzh` 二进制文件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    /// 配置 schema 版本号，用于迁移决策。
    pub schema_version: u32,
    /// 元信息（名称、时间戳、生成版本）。
    pub meta: ProfileMeta,
    /// 连发规则列表，长度不超过 [`MAX_RULES`]。
    pub rules: Vec<BurstRule>,
    /// 全局热键设置。
    #[serde(default)]
    pub hotkeys: Hotkeys,
    /// 高级选项（日志级别等）。
    #[serde(default)]
    pub advanced: Advanced,
}

/// 配置文件的元信息：名称、创建/更新时间、生成应用版本。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileMeta {
    /// 用户可见的配置名称，同时决定文件名（经过 sanitize）。
    pub name: String,
    /// 首次创建时间（Unix 秒）。
    pub created_at: u64,
    /// 最近一次保存时间（Unix 秒）。
    pub updated_at: u64,
    /// 写入此文件时使用的应用版本号（CARGO_PKG_VERSION）。
    pub app_version: String,
}

/// 单条连发规则：把 `trigger_key` 触发的事件转换为针对 `target_key` 的连发输出。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurstRule {
    /// 规则在配置文件内的稳定唯一 ID。
    pub id: String,
    /// 是否启用此规则。被禁用的规则不会被引擎装载。
    pub enabled: bool,
    /// 触发键（物理按下时启动连发）的虚拟键码。
    pub trigger_key: u32,
    /// 连发实际输出到游戏的目标键虚拟键码。
    pub target_key: u32,
    /// 触发模式：[`BurstMode::Hold`] 或 [`BurstMode::Toggle`]。
    pub mode: BurstMode,
    /// Toggle mode only: separate stop hotkey. Defaults to trigger_key when None.
    #[serde(default)]
    pub stop_key: Option<u32>,
    /// Milliseconds between simulated keypresses. Clamped to [10, 10000].
    pub interval_ms: u32,
}

/// 连发触发模式。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BurstMode {
    /// Fire while key is held.
    Hold,
    /// Toggle on/off with hotkey.
    Toggle,
}

/// 全局热键映射。当前仅支持「全局开关」一个槽位。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Hotkeys {
    /// 全局连发引擎开关的虚拟键码。`None` 表示未绑定热键。
    pub global_toggle: Option<u32>,
}

/// 高级选项。当前仅含日志级别，未来可扩展。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Advanced {
    /// 应用日志级别（与 `tracing_subscriber::EnvFilter` 一致），如 `info` / `debug`。
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_log_level() -> String {
    "info".to_string()
}

/// [`Profile::validate`] / 解析 `.qzh` 时返回的错误集合。
#[derive(Debug, Error)]
pub enum ProfileError {
    /// 规则数量超过 [`MAX_RULES`]。
    #[error("rule count exceeds maximum of {MAX_RULES}")]
    TooManyRules,
    /// 连发间隔不在 `[10, 10000]` ms 范围内。
    #[error("rule interval {0}ms is out of range [10, 10000]")]
    InvalidInterval(u32),
    /// DD-HID 模式下 Toggle 规则的 `target_key == trigger_key`，会导致自循环。
    #[error("rule {0}: target_key must differ from trigger_key in DD mode")]
    DdTargetEqualsTrigger(String),
    /// DD-HID 模式下 Toggle 规则的 `target_key == stop_key`，会导致按下停止键即又触发。
    #[error("rule {0}: target_key must differ from stop_key in DD mode")]
    DdTargetEqualsStop(String),
    /// 解析或序列化 JSON 时出错。
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// 调用 [`crypto`] 加解密时出错。
    #[error(transparent)]
    Crypto(#[from] crypto::CryptoError),
    /// 文件读写出错。
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// 文件头不合法（魔数/版本号不匹配，或长度不足）。
    #[error("invalid file format")]
    InvalidFormat,
    /// 配置 schema 版本高于当前应用支持的 [`CURRENT_SCHEMA_VERSION`]，需要升级应用。
    #[error("schema version {0} is newer than supported {CURRENT_SCHEMA_VERSION}")]
    TooNew(u32),
    /// 迁移过程中出错。
    #[error(transparent)]
    Migrate(#[from] migrate::MigrateError),
}

impl Profile {
    /// 通用校验：仅检查规则数与连发间隔范围，不做后端模式相关约束。
    pub fn validate(&self) -> Result<(), ProfileError> {
        self.validate_for_mode(false)
    }

    /// `distinct_target = true` 时启用 DD-HID 模式专属约束：
    /// DD 后端无法在 dwExtraInfo 中写入过滤标记，但 Hold 模式靠应用层注入事件队列识别
    /// 自身注入，已允许「触发键 == 目标键」（连发 CD 类典型用法）。Toggle 模式因 sim
    /// KEYDOWN 必须被 hook 处理（toggle 的本意），无法过滤自身，故仍要求：
    /// - `target_key != trigger_key`
    /// - `target_key != stop_key`（默认 `stop_key = trigger_key`）
    pub fn validate_for_mode(&self, distinct_target: bool) -> Result<(), ProfileError> {
        if self.rules.len() > MAX_RULES {
            return Err(ProfileError::TooManyRules);
        }
        for rule in &self.rules {
            let i = rule.interval_ms;
            if !(10..=10000).contains(&i) {
                return Err(ProfileError::InvalidInterval(i));
            }
            if distinct_target && rule.enabled && rule.mode == BurstMode::Toggle {
                if rule.target_key == rule.trigger_key {
                    return Err(ProfileError::DdTargetEqualsTrigger(rule.id.clone()));
                }
                let stop = rule.stop_key.unwrap_or(rule.trigger_key);
                if rule.target_key == stop {
                    return Err(ProfileError::DdTargetEqualsStop(rule.id.clone()));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn rule(id: &str, mode: BurstMode, trigger: u32, target: u32, interval: u32) -> BurstRule {
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

    #[test]
    fn validate_accepts_empty_profile() {
        assert!(make_profile(vec![]).validate().is_ok());
    }

    #[test]
    fn validate_accepts_interval_at_lower_bound() {
        let p = make_profile(vec![rule("r", BurstMode::Hold, 0x41, 0x42, 10)]);
        assert!(p.validate().is_ok());
    }

    #[test]
    fn validate_accepts_interval_at_upper_bound() {
        let p = make_profile(vec![rule("r", BurstMode::Hold, 0x41, 0x42, 10000)]);
        assert!(p.validate().is_ok());
    }

    #[test]
    fn validate_rejects_interval_below_minimum() {
        let p = make_profile(vec![rule("r", BurstMode::Hold, 0x41, 0x42, 9)]);
        assert!(matches!(
            p.validate(),
            Err(ProfileError::InvalidInterval(9))
        ));
    }

    #[test]
    fn validate_rejects_interval_above_maximum() {
        let p = make_profile(vec![rule("r", BurstMode::Hold, 0x41, 0x42, 10001)]);
        assert!(matches!(
            p.validate(),
            Err(ProfileError::InvalidInterval(10001))
        ));
    }

    #[test]
    fn validate_rejects_too_many_rules() {
        let rules = (0..=MAX_RULES)
            .map(|i| rule(&format!("r{i}"), BurstMode::Hold, 0x41, 0x42, 10))
            .collect();
        assert!(matches!(
            make_profile(rules).validate(),
            Err(ProfileError::TooManyRules)
        ));
    }

    #[test]
    fn validate_accepts_max_rules() {
        let rules = (0..MAX_RULES)
            .map(|i| rule(&format!("r{i}"), BurstMode::Hold, 0x41, 0x42, 10))
            .collect();
        assert!(make_profile(rules).validate().is_ok());
    }

    #[test]
    fn validate_for_mode_dd_allows_hold_with_same_key() {
        // Hold 模式 trigger == target 在 DD 模式合法
        let p = make_profile(vec![rule("h", BurstMode::Hold, 0x51, 0x51, 10)]);
        assert!(p.validate_for_mode(true).is_ok());
    }

    #[test]
    fn validate_for_mode_dd_rejects_toggle_target_equals_trigger() {
        let p = make_profile(vec![rule("t", BurstMode::Toggle, 0x46, 0x46, 10)]);
        let err = p.validate_for_mode(true).unwrap_err();
        assert!(matches!(err, ProfileError::DdTargetEqualsTrigger(ref id) if id == "t"));
    }

    #[test]
    fn validate_for_mode_dd_rejects_toggle_target_equals_stop_key() {
        let mut r = rule("t", BurstMode::Toggle, 0x46, 0x47, 10);
        r.stop_key = Some(0x47);
        let p = make_profile(vec![r]);
        let err = p.validate_for_mode(true).unwrap_err();
        assert!(matches!(err, ProfileError::DdTargetEqualsStop(ref id) if id == "t"));
    }

    #[test]
    fn validate_for_mode_dd_skips_disabled_rules() {
        let mut r = rule("t", BurstMode::Toggle, 0x46, 0x46, 10);
        r.enabled = false;
        let p = make_profile(vec![r]);
        assert!(p.validate_for_mode(true).is_ok());
    }

    #[test]
    fn validate_default_mode_allows_toggle_target_equals_trigger() {
        // 非 DD 模式（distinct_target = false）允许 toggle target == trigger
        let p = make_profile(vec![rule("t", BurstMode::Toggle, 0x46, 0x46, 10)]);
        assert!(p.validate().is_ok());
    }
}
