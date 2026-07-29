#[path = "resource/chat.rs"]
mod chat;
#[path = "resource/context_model.rs"]
mod context_model;
#[path = "resource/lanes.rs"]
mod lanes;
#[path = "resource/optimization.rs"]
mod optimization;
#[path = "resource/pressure.rs"]
mod pressure;
#[path = "resource/types.rs"]
mod types;

pub use chat::chat_governor_decision;
pub use context_model::context_model_governor_decision;
pub use lanes::team_lane_decision;
pub use optimization::optimization_policy_decision;
pub use pressure::classify_pressure;
pub use types::{
    ContextGovernorAction, ContextModelGovernorDecision, ModelRouteHint, ModelTier,
    OptimizationPolicyDecision, OptimizationPolicyInput, OptimizationPolicyStatus,
    ResourceGovernorAdmission, ResourceGovernorDecision, ResourceGovernorTokenAction,
    ResourceLaneAdmission, ResourceLaneDecision, ResourcePressure,
};

pub const DEGRADED_CHAT_MAX_TOKENS: u32 = 128;
pub const DEFAULT_TEAM_REQUESTED_LANES: u32 = 2;
pub const NORMAL_CONTEXT_BUDGET_TOKENS: u32 = 4096;
pub const DEGRADED_CONTEXT_LIMIT_TOKENS: u32 = 2048;
pub const SMALL_MODEL_CONTEXT_SOFT_LIMIT_TOKENS: u32 = 3072;
pub const OPTIMIZATION_LOW_TOKENS_PER_SECOND: f64 = 5.0;
pub const OPTIMIZATION_HIGH_P95_LATENCY_MS: f64 = 30_000.0;

#[cfg(test)]
#[path = "resource/tests.rs"]
mod tests;
