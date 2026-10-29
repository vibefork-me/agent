//! Models offered to the user.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Cost class of a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CostClass {
    /// Included in a subscription the user already pays for.
    Subscription,
    /// Free tier.
    Free,
    /// Paid per token.
    Paid,
}

/// A model available on this machine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ModelInfo {
    /// Provider: `claude`, `codex`, `copilot`, `openrouter`.
    pub provider: String,
    /// Model id; empty = provider default.
    pub model: String,
    /// 1 light … 3 strong.
    pub tier: u8,
    /// Cost class.
    pub cost: CostClass,
    /// Label for the UI.
    pub label: String,
}
