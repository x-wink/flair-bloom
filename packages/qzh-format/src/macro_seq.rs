use serde::{Deserialize, Serialize};

pub const MAX_STEPS: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroSequence {
    pub schema_version: u32,
    pub name: String,
    pub steps: Vec<MacroStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroStep {
    pub key: u32,
    pub action: KeyAction,
    /// Milliseconds delay after this step.
    pub delay_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyAction {
    Press,
    Release,
}
