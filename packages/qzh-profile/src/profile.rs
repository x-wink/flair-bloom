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
/// 数据结构上的绝对下限。`validate` 据此判定结构合法性，不收紧到 [`MIN_EFFECTIVE_INTERVAL_MS`]，
/// 旧配置 <10ms 在加载时由 [`Profile::clamp_intervals`] 钳位而非拒绝。
pub const MIN_INTERVAL_MS: u32 = 1;
/// 注入周期「基础」下限（ms）：全局 LL 钩子链 + RIT 对每个注入事件有固定「过路税」，管线
/// 可持续的「总」注入速率有上限。单条规则用此基础下限（10ms ≈ 100 taps/s 仍可稳定收住）；
/// 多条规则同时连发时，连发调度器（burst-engine `process_due`）把每条规则的有效下限放大为
/// 「此值 × 活跃规则数」，使总速率与规则数无关、不叠加超发导致停止后「收不住」（实测早期
/// 多规则齐发即收不住，根因是总速率而非单规则）。UI 默认/下限、加载钳位
/// （[`Profile::clamp_intervals`]）都以此为准。
pub const MIN_EFFECTIVE_INTERVAL_MS: u32 = 10;
pub const DEFAULT_INTERVAL_MS: u32 = 10;
pub const MAX_INTERVAL_MS: u32 = 10000;

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
    /// 面板水印署名，空串即未设置；只在持有 `flair.signature` 权益时显示，没有权益也原样保留。
    /// 反序列化即经 [`sanitize_signature`] 净化，导入、列表、保存等所有读路径都不用各自再记得调。
    #[serde(default, deserialize_with = "deserialize_signature")]
    pub signature: String,
}

/// 署名的最大字符数（按 Unicode 标量计，不按字节）。竖版面板宽度下 16 个汉字刚好一行。
pub const MAX_SIGNATURE_CHARS: usize = 16;

/// 去掉控制字符与首尾空白，截到 [`MAX_SIGNATURE_CHARS`] 个字符。
pub fn sanitize_signature(raw: &str) -> String {
    let cleaned: String = raw.chars().filter(|c| !c.is_control()).collect();
    let truncated: String = cleaned.trim().chars().take(MAX_SIGNATURE_CHARS).collect();
    truncated.trim_end().to_string()
}

fn deserialize_signature<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    String::deserialize(d).map(|s| sanitize_signature(&s))
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
    /// 仅 Toggle 模式：独立停止热键。为 `None` 时回退为 `trigger_key`。
    #[serde(default)]
    pub stop_key: Option<KeyId>,
    /// 每次模拟按键之间的间隔毫秒数。钳进 \[[`MIN_INTERVAL_MS`], [`MAX_INTERVAL_MS`]\]。
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
    /// 按住触发键期间持续连发。
    Hold,
    /// 按一下热键开启、再按一下关闭。
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
    /// 连发间隔不在允许范围内。
    #[error("rule interval {0}ms is out of range")]
    InvalidInterval(u32),
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
    /// 把所有规则的连发间隔钳进 \[[`MIN_EFFECTIVE_INTERVAL_MS`], [`MAX_INTERVAL_MS`]\]。
    /// 加载旧配置时调用：低于有效操作下限的间隔静默抬到下限（管线可持续注入上限，
    /// 详见 [`MIN_EFFECTIVE_INTERVAL_MS`]），而非拒绝加载。
    pub fn clamp_intervals(&mut self) {
        for rule in &mut self.rules {
            rule.interval_ms = rule
                .interval_ms
                .clamp(MIN_EFFECTIVE_INTERVAL_MS, MAX_INTERVAL_MS);
        }
    }

    /// 文件结构完整性校验：规则数与连发间隔范围。**不检查按键策略**。
    ///
    /// 按键是否被所在槽位接受由 [`crate::key_policy`] 判定，且刻意不放在这里：加载一份
    /// 含不支持按键的旧配置时要能净化后照常打开，而不是整个文件打不开。策略的强制点在
    /// 命令层（保存规则、切换输入模式、设置热键），加载期则走
    /// [`crate::key_policy::sanitize_profile`]。
    pub fn validate(&self) -> Result<(), ProfileError> {
        if self.rules.len() > MAX_RULES {
            return Err(ProfileError::TooManyRules);
        }
        for rule in &self.rules {
            let i = rule.interval_ms;
            if !(MIN_INTERVAL_MS..=MAX_INTERVAL_MS).contains(&i) {
                return Err(ProfileError::InvalidInterval(i));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "profile_tests.rs"]
mod tests;
