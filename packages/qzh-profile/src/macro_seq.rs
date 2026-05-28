//! 宏序列数据结构（亲友专属功能：宏录制与回放）。
//!
//! 序列在配置文件中以独立条目落盘，使用 [`crate::migrate`] 共用迁移基础设施做版本演进。

use serde::{Deserialize, Serialize};

/// 单个宏序列允许的最大步骤数。超出会在 schema 校验阶段被拒绝。
pub const MAX_STEPS: usize = 256;

/// 一个完整的宏序列：一组按顺序回放的按键步骤。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroSequence {
    /// 宏序列的 schema 版本号，用于跨版本迁移。
    pub schema_version: u32,
    /// 用户可见的宏名称。
    pub name: String,
    /// 宏的步骤列表，按下标顺序执行。
    pub steps: Vec<MacroStep>,
}

/// 宏序列中的一步：对某个虚拟键执行 press 或 release，并在之后等待 `delay_ms`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroStep {
    /// 目标按键的虚拟键码（VK code）。
    pub key: u32,
    /// 该步要执行的动作（按下 / 抬起）。
    pub action: KeyAction,
    /// Milliseconds delay after this step.
    pub delay_ms: u32,
}

/// 宏序列每一步可执行的按键动作。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyAction {
    /// 按下目标键（KEYDOWN）。
    Press,
    /// 抬起目标键（KEYUP）。
    Release,
}
