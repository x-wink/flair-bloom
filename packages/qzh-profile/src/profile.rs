//! Profile 数据结构：连发规则、元信息、热键与高级选项。
//!
//! 这是 `.qzh` 文件解密后的明文 JSON 形态。落盘时由 [`crate::header::FileHeader`]
//! 与 [`crypto`] 包装为加密二进制。

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::key_id::KeyId;

/// 当前 [`Profile`] 的 schema 版本号。
///
/// 升级配置数据结构时必须递增并同步 `migrate::migrate_step`，否则旧文件无法升级。
///
/// - v1：所有按键字段为裸 `u32` VK 码。
/// - v2：按键字段改为 [`KeyId`]，支持键盘 + 鼠标 5 键。
/// - v3：MouseButton 新增 WheelUp / WheelDown，旧文件向后兼容（新字段有默认值不涉及结构变更）。
/// - v4：BurstRule 新增可选 `group` 字段，用于 Toggle 规则互斥分组；旧文件向后兼容。
pub const CURRENT_SCHEMA_VERSION: u32 = 4;
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
    /// 触发键（物理按下时启动连发）。
    pub trigger_key: KeyId,
    /// 连发实际输出到游戏的目标键。
    pub target_key: KeyId,
    /// 触发模式：[`BurstMode::Hold`] 或 [`BurstMode::Toggle`]。
    pub mode: BurstMode,
    /// Toggle mode only: separate stop hotkey. Defaults to trigger_key when None.
    #[serde(default)]
    pub stop_key: Option<KeyId>,
    /// Milliseconds between simulated keypresses. Clamped to [10, 10000].
    pub interval_ms: u32,
    /// Toggle 规则互斥分组名。同组内激活一条规则时，其他活跃的同组 Toggle 规则自动停止。
    /// Hold 规则忽略此字段。`None` 表示不属于任何分组。
    #[serde(default)]
    pub group: Option<String>,
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

/// 全局热键映射。
///
/// 当 `global_stop` 为 `None` 时，`global_toggle` 兼作切换键（按一下开，再按一下关）。
/// 当两者不同时，`global_toggle` 只负责开启，`global_stop` 只负责关闭。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Hotkeys {
    /// 全局开启热键（`None` 表示未绑定）。
    pub global_toggle: Option<KeyId>,
    /// 全局停止热键；`None` 时与 `global_toggle` 共用（切换模式）。
    #[serde(default)]
    pub global_stop: Option<KeyId>,
    /// 面板显示/隐藏热键。
    #[serde(default)]
    pub panel_toggle: Option<KeyId>,
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
    /// DD 后端无法在 dwExtraInfo 中写入过滤标记，Toggle 模式的 sim KEYDOWN 会被 hook
    /// 处理，无法过滤自身，故要求：
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
            if !distinct_target || !rule.enabled {
                continue;
            }
            if rule.mode == BurstMode::Toggle {
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
#[path = "profile_tests.rs"]
mod tests;
