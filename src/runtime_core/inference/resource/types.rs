#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourcePressure {
    Unknown,
    Normal,
    Degraded,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceGovernorAdmission {
    Allow,
    Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceGovernorTokenAction {
    Unchanged,
    Clamped,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceLaneAdmission {
    AllowParallel,
    SequentialFallback,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextGovernorAction {
    Unchanged,
    Clamped,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelRouteHint {
    Keep,
    Downgrade,
    Escalate,
    Defer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelTier {
    Small,
    Standard,
    Large,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizationPolicyStatus {
    Recommend,
    InsufficientEvidence,
    Constrained,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceGovernorDecision {
    pub pressure: ResourcePressure,
    pub requested_max_tokens: u32,
    pub effective_max_tokens: Option<u32>,
    pub admission: ResourceGovernorAdmission,
    pub token_action: ResourceGovernorTokenAction,
    pub reason: &'static str,
    pub hint: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceLaneDecision {
    pub pressure: ResourcePressure,
    pub requested_lanes: u32,
    pub admitted_lanes: u32,
    pub admission: ResourceLaneAdmission,
    pub fallback: &'static str,
    pub reason: &'static str,
    pub hint: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextModelGovernorDecision {
    pub pressure: ResourcePressure,
    pub requested_context_tokens: u32,
    pub context_limit_tokens: u32,
    pub effective_context_tokens: Option<u32>,
    pub context_action: ContextGovernorAction,
    pub model_tier: ModelTier,
    pub model_hint: ModelRouteHint,
    pub admission: ResourceGovernorAdmission,
    pub reason: &'static str,
    pub hint: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OptimizationPolicyInput {
    pub pressure: ResourcePressure,
    pub model_runs: usize,
    pub measured_benchmark_runs: usize,
    pub failed_benchmark_runs: usize,
    pub context_clamp_count: i64,
    pub p95_latency_ms: Option<f64>,
    pub avg_tokens_per_second: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptimizationPolicyDecision {
    pub status: OptimizationPolicyStatus,
    pub recommended_context_tokens: Option<u32>,
    pub recommended_lanes: u32,
    pub fallback: &'static str,
    pub model_hint: ModelRouteHint,
    pub reason: &'static str,
    pub hint: &'static str,
}

impl ResourcePressure {
    pub fn as_str(self) -> &'static str {
        match self {
            ResourcePressure::Unknown => "unknown",
            ResourcePressure::Normal => "normal",
            ResourcePressure::Degraded => "degraded",
            ResourcePressure::Critical => "critical",
        }
    }
}

impl ResourceGovernorAdmission {
    pub fn as_str(self) -> &'static str {
        match self {
            ResourceGovernorAdmission::Allow => "allow",
            ResourceGovernorAdmission::Block => "block",
        }
    }
}

impl ResourceGovernorTokenAction {
    pub fn as_str(self) -> &'static str {
        match self {
            ResourceGovernorTokenAction::Unchanged => "unchanged",
            ResourceGovernorTokenAction::Clamped => "clamped",
            ResourceGovernorTokenAction::Blocked => "blocked",
        }
    }
}

impl ResourceLaneAdmission {
    pub fn as_str(self) -> &'static str {
        match self {
            ResourceLaneAdmission::AllowParallel => "allow-parallel",
            ResourceLaneAdmission::SequentialFallback => "sequential-fallback",
            ResourceLaneAdmission::Blocked => "blocked",
        }
    }
}

impl ContextGovernorAction {
    pub fn as_str(self) -> &'static str {
        match self {
            ContextGovernorAction::Unchanged => "unchanged",
            ContextGovernorAction::Clamped => "clamped",
            ContextGovernorAction::Blocked => "blocked",
        }
    }
}

impl ModelRouteHint {
    pub fn as_str(self) -> &'static str {
        match self {
            ModelRouteHint::Keep => "keep",
            ModelRouteHint::Downgrade => "downgrade",
            ModelRouteHint::Escalate => "escalate",
            ModelRouteHint::Defer => "defer",
        }
    }
}

impl ModelTier {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "small" => Some(Self::Small),
            "standard" => Some(Self::Standard),
            "large" => Some(Self::Large),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ModelTier::Small => "small",
            ModelTier::Standard => "standard",
            ModelTier::Large => "large",
        }
    }
}

impl OptimizationPolicyStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            OptimizationPolicyStatus::Recommend => "recommend",
            OptimizationPolicyStatus::InsufficientEvidence => "insufficient-evidence",
            OptimizationPolicyStatus::Constrained => "constrained",
            OptimizationPolicyStatus::Blocked => "blocked",
        }
    }
}

impl ResourceGovernorDecision {
    pub fn is_blocked(&self) -> bool {
        self.admission == ResourceGovernorAdmission::Block
    }
}

impl ResourceLaneDecision {
    pub fn is_blocked(&self) -> bool {
        self.admission == ResourceLaneAdmission::Blocked
    }
}

impl ContextModelGovernorDecision {
    pub fn is_blocked(&self) -> bool {
        self.admission == ResourceGovernorAdmission::Block
    }
}
