const GIB_BYTES: u64 = 1024 * 1024 * 1024;
const DEGRADED_CPU_PERCENT: f64 = 80.0;
const CRITICAL_CPU_PERCENT: f64 = 95.0;
const DEGRADED_RSS_BYTES: u64 = 8 * GIB_BYTES;
const CRITICAL_RSS_BYTES: u64 = 12 * GIB_BYTES;
pub const DEGRADED_CHAT_MAX_TOKENS: u32 = 128;
pub const DEFAULT_TEAM_REQUESTED_LANES: u32 = 2;
pub const DEFAULT_CONTEXT_LIMIT_TOKENS: u32 = 4096;
pub const DEGRADED_CONTEXT_LIMIT_TOKENS: u32 = 2048;
pub const SMALL_MODEL_CONTEXT_SOFT_LIMIT_TOKENS: u32 = 3072;
pub const OPTIMIZATION_LOW_TOKENS_PER_SECOND: f64 = 5.0;
pub const OPTIMIZATION_HIGH_P95_LATENCY_MS: f64 = 30_000.0;

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

pub fn classify_pressure(
    process_cpu_percent: Option<f64>,
    average_rss_bytes: Option<u64>,
    peak_rss_bytes: Option<u64>,
) -> ResourcePressure {
    let cpu = process_cpu_percent.filter(|value| value.is_finite());
    let memory = peak_rss_bytes.or(average_rss_bytes);

    if cpu.is_none() && memory.is_none() {
        return ResourcePressure::Unknown;
    }
    if cpu.is_some_and(|value| value >= CRITICAL_CPU_PERCENT)
        || memory.is_some_and(|value| value >= CRITICAL_RSS_BYTES)
    {
        return ResourcePressure::Critical;
    }
    if cpu.is_some_and(|value| value >= DEGRADED_CPU_PERCENT)
        || memory.is_some_and(|value| value >= DEGRADED_RSS_BYTES)
    {
        return ResourcePressure::Degraded;
    }
    ResourcePressure::Normal
}

pub fn chat_governor_decision(
    pressure: ResourcePressure,
    requested_max_tokens: u32,
) -> ResourceGovernorDecision {
    match pressure {
        ResourcePressure::Critical => ResourceGovernorDecision {
            pressure,
            requested_max_tokens,
            effective_max_tokens: None,
            admission: ResourceGovernorAdmission::Block,
            token_action: ResourceGovernorTokenAction::Blocked,
            reason: "critical resource pressure",
            hint: "run backend status, stop the sidecar, or lower host load before retrying",
        },
        ResourcePressure::Degraded => {
            let effective_max_tokens = requested_max_tokens.min(DEGRADED_CHAT_MAX_TOKENS);
            ResourceGovernorDecision {
                pressure,
                requested_max_tokens,
                effective_max_tokens: Some(effective_max_tokens),
                admission: ResourceGovernorAdmission::Allow,
                token_action: if effective_max_tokens < requested_max_tokens {
                    ResourceGovernorTokenAction::Clamped
                } else {
                    ResourceGovernorTokenAction::Unchanged
                },
                reason: "degraded resource pressure",
                hint: "use a smaller --max-tokens value or restart with a smaller --ctx-size if pressure persists",
            }
        }
        ResourcePressure::Unknown => ResourceGovernorDecision {
            pressure,
            requested_max_tokens,
            effective_max_tokens: Some(requested_max_tokens),
            admission: ResourceGovernorAdmission::Allow,
            token_action: ResourceGovernorTokenAction::Unchanged,
            reason: "resource pressure unknown",
            hint: "resource sample is incomplete, so the requested token limit is preserved",
        },
        ResourcePressure::Normal => ResourceGovernorDecision {
            pressure,
            requested_max_tokens,
            effective_max_tokens: Some(requested_max_tokens),
            admission: ResourceGovernorAdmission::Allow,
            token_action: ResourceGovernorTokenAction::Unchanged,
            reason: "resource pressure normal",
            hint: "no runtime clamp applied",
        },
    }
}

pub fn team_lane_decision(
    pressure: ResourcePressure,
    requested_lanes: u32,
) -> ResourceLaneDecision {
    let requested_lanes = requested_lanes.max(1);
    match pressure {
        ResourcePressure::Normal => ResourceLaneDecision {
            pressure,
            requested_lanes,
            admitted_lanes: requested_lanes,
            admission: ResourceLaneAdmission::AllowParallel,
            fallback: "none",
            reason: "resource pressure normal",
            hint: "parallel team lanes may proceed within file ownership, tool risk, and approval limits",
        },
        ResourcePressure::Unknown => ResourceLaneDecision {
            pressure,
            requested_lanes,
            admitted_lanes: 1,
            admission: ResourceLaneAdmission::SequentialFallback,
            fallback: "sequential",
            reason: "resource pressure unknown",
            hint: "resource sample is missing or incomplete, so dispatch should stay sequential until telemetry exists",
        },
        ResourcePressure::Degraded => ResourceLaneDecision {
            pressure,
            requested_lanes,
            admitted_lanes: 1,
            admission: ResourceLaneAdmission::SequentialFallback,
            fallback: "sequential",
            reason: "degraded resource pressure",
            hint: "run subagents sequentially or reduce backend/model/context pressure before parallel dispatch",
        },
        ResourcePressure::Critical => ResourceLaneDecision {
            pressure,
            requested_lanes,
            admitted_lanes: 0,
            admission: ResourceLaneAdmission::Blocked,
            fallback: "wait",
            reason: "critical resource pressure",
            hint: "do not dispatch new team lanes until backend status recovers or host load is reduced",
        },
    }
}

pub fn context_model_governor_decision(
    pressure: ResourcePressure,
    requested_context_tokens: u32,
    context_limit_tokens: u32,
    model_tier: ModelTier,
) -> ContextModelGovernorDecision {
    let requested_context_tokens = requested_context_tokens.max(1);
    let context_limit_tokens = context_limit_tokens.max(1);
    if pressure == ResourcePressure::Critical {
        return ContextModelGovernorDecision {
            pressure,
            requested_context_tokens,
            context_limit_tokens,
            effective_context_tokens: None,
            context_action: ContextGovernorAction::Blocked,
            model_tier,
            model_hint: ModelRouteHint::Defer,
            admission: ResourceGovernorAdmission::Block,
            reason: "critical resource pressure",
            hint:
                "defer model selection and context packing until backend or host pressure recovers",
        };
    }

    let pressure_limit = if pressure == ResourcePressure::Degraded {
        context_limit_tokens.min(DEGRADED_CONTEXT_LIMIT_TOKENS)
    } else {
        context_limit_tokens
    };
    let tier_limit = match model_tier {
        ModelTier::Small => pressure_limit.min(SMALL_MODEL_CONTEXT_SOFT_LIMIT_TOKENS),
        ModelTier::Standard | ModelTier::Large => pressure_limit,
    };
    let effective_context_tokens = requested_context_tokens.min(tier_limit);
    let context_action = if effective_context_tokens < requested_context_tokens {
        ContextGovernorAction::Clamped
    } else {
        ContextGovernorAction::Unchanged
    };
    let model_hint = if pressure == ResourcePressure::Degraded && model_tier != ModelTier::Small {
        ModelRouteHint::Downgrade
    } else if requested_context_tokens > tier_limit {
        ModelRouteHint::Escalate
    } else {
        ModelRouteHint::Keep
    };
    let reason = match (pressure, context_action, model_hint) {
        (ResourcePressure::Degraded, _, ModelRouteHint::Downgrade) => "degraded resource pressure",
        (_, ContextGovernorAction::Clamped, ModelRouteHint::Escalate) => {
            "requested context exceeds current model/context budget"
        }
        (_, ContextGovernorAction::Clamped, _) => "requested context was clamped",
        (ResourcePressure::Unknown, _, _) => "resource pressure unknown",
        _ => "resource pressure normal",
    };
    let hint = match model_hint {
        ModelRouteHint::Downgrade => {
            "prefer a smaller model tier or sequential lanes while resource pressure is degraded"
        }
        ModelRouteHint::Escalate => {
            "use a larger-context model/backend profile, split the task, or reduce retrieved context"
        }
        ModelRouteHint::Keep if pressure == ResourcePressure::Unknown => {
            "keep the current model tier but avoid parallel context growth until telemetry exists"
        }
        ModelRouteHint::Keep => "keep the current model tier and context budget",
        ModelRouteHint::Defer => {
            "do not dispatch model work until critical pressure is cleared"
        }
    };

    ContextModelGovernorDecision {
        pressure,
        requested_context_tokens,
        context_limit_tokens,
        effective_context_tokens: Some(effective_context_tokens),
        context_action,
        model_tier,
        model_hint,
        admission: ResourceGovernorAdmission::Allow,
        reason,
        hint,
    }
}

pub fn optimization_policy_decision(input: OptimizationPolicyInput) -> OptimizationPolicyDecision {
    if input.pressure == ResourcePressure::Critical {
        return OptimizationPolicyDecision {
            status: OptimizationPolicyStatus::Blocked,
            recommended_context_tokens: None,
            recommended_lanes: 0,
            fallback: "wait",
            model_hint: ModelRouteHint::Defer,
            reason: "critical resource pressure",
            hint: "do not dispatch model/team work until backend or host pressure recovers",
        };
    }

    let has_local_metrics = input.model_runs > 0 || input.measured_benchmark_runs > 0;
    let has_benchmark_evidence = input.measured_benchmark_runs > 0;
    if !has_local_metrics {
        return OptimizationPolicyDecision {
            status: OptimizationPolicyStatus::InsufficientEvidence,
            recommended_context_tokens: Some(DEGRADED_CONTEXT_LIMIT_TOKENS),
            recommended_lanes: 1,
            fallback: "sequential",
            model_hint: ModelRouteHint::Keep,
            reason: "no local runtime metrics or measured benchmark evidence",
            hint: "run monitor baseline and executable benchmarks before increasing context or parallel lanes",
        };
    }

    if input.pressure == ResourcePressure::Degraded {
        return OptimizationPolicyDecision {
            status: OptimizationPolicyStatus::Constrained,
            recommended_context_tokens: Some(DEGRADED_CONTEXT_LIMIT_TOKENS),
            recommended_lanes: 1,
            fallback: "sequential",
            model_hint: ModelRouteHint::Downgrade,
            reason: "degraded resource pressure",
            hint:
                "prefer a smaller model/context profile and sequential lanes until pressure clears",
        };
    }

    if input.pressure == ResourcePressure::Unknown {
        return OptimizationPolicyDecision {
            status: if has_benchmark_evidence {
                OptimizationPolicyStatus::Recommend
            } else {
                OptimizationPolicyStatus::InsufficientEvidence
            },
            recommended_context_tokens: Some(SMALL_MODEL_CONTEXT_SOFT_LIMIT_TOKENS),
            recommended_lanes: 1,
            fallback: "sequential",
            model_hint: if input.failed_benchmark_runs > 0 {
                ModelRouteHint::Escalate
            } else {
                ModelRouteHint::Keep
            },
            reason: "resource pressure unknown",
            hint: "keep dispatch sequential until a fresh resource sample exists",
        };
    }

    if input.failed_benchmark_runs > 0 {
        return OptimizationPolicyDecision {
            status: OptimizationPolicyStatus::Constrained,
            recommended_context_tokens: Some(DEFAULT_CONTEXT_LIMIT_TOKENS),
            recommended_lanes: 1,
            fallback: "review-before-parallel",
            model_hint: ModelRouteHint::Escalate,
            reason: "measured benchmark failure exists",
            hint: "review failed local benchmark rows before widening team lanes or accepting the current model route",
        };
    }

    if input.context_clamp_count > 0 {
        return OptimizationPolicyDecision {
            status: OptimizationPolicyStatus::Constrained,
            recommended_context_tokens: Some(SMALL_MODEL_CONTEXT_SOFT_LIMIT_TOKENS),
            recommended_lanes: 1,
            fallback: "sequential",
            model_hint: ModelRouteHint::Keep,
            reason: "context clamp observed in local metrics",
            hint: "lower retrieval/context packing budget before increasing parallelism",
        };
    }

    let slow_latency = input
        .p95_latency_ms
        .is_some_and(|value| value.is_finite() && value >= OPTIMIZATION_HIGH_P95_LATENCY_MS);
    let low_throughput = input.avg_tokens_per_second.is_some_and(|value| {
        value.is_finite() && value > 0.0 && value <= OPTIMIZATION_LOW_TOKENS_PER_SECOND
    });
    if slow_latency || low_throughput {
        return OptimizationPolicyDecision {
            status: OptimizationPolicyStatus::Constrained,
            recommended_context_tokens: Some(DEGRADED_CONTEXT_LIMIT_TOKENS),
            recommended_lanes: 1,
            fallback: "sequential",
            model_hint: ModelRouteHint::Downgrade,
            reason: "slow local latency or token throughput observed",
            hint: "reduce context or route to a lighter model profile before enabling parallel team lanes",
        };
    }

    if !has_benchmark_evidence {
        return OptimizationPolicyDecision {
            status: OptimizationPolicyStatus::InsufficientEvidence,
            recommended_context_tokens: Some(SMALL_MODEL_CONTEXT_SOFT_LIMIT_TOKENS),
            recommended_lanes: 1,
            fallback: "sequential",
            model_hint: ModelRouteHint::Keep,
            reason: "local model metrics exist but measured benchmark evidence is missing",
            hint: "record at least one measured benchmark row before widening team lanes",
        };
    }

    OptimizationPolicyDecision {
        status: OptimizationPolicyStatus::Recommend,
        recommended_context_tokens: Some(DEFAULT_CONTEXT_LIMIT_TOKENS),
        recommended_lanes: DEFAULT_TEAM_REQUESTED_LANES,
        fallback: "none",
        model_hint: ModelRouteHint::Keep,
        reason: "measured local metrics and benchmark evidence are within policy limits",
        hint: "current context budget and parallel lane default may proceed within approval and ownership policy",
    }
}

#[cfg(test)]
#[path = "resource/tests.rs"]
mod tests;
