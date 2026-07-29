use super::{
    ModelRouteHint, OptimizationPolicyDecision, OptimizationPolicyInput, OptimizationPolicyStatus,
    ResourcePressure, DEFAULT_TEAM_REQUESTED_LANES, DEGRADED_CONTEXT_LIMIT_TOKENS,
    NORMAL_CONTEXT_BUDGET_TOKENS, OPTIMIZATION_HIGH_P95_LATENCY_MS,
    OPTIMIZATION_LOW_TOKENS_PER_SECOND, SMALL_MODEL_CONTEXT_SOFT_LIMIT_TOKENS,
};

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
            recommended_context_tokens: Some(NORMAL_CONTEXT_BUDGET_TOKENS),
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
        recommended_context_tokens: Some(NORMAL_CONTEXT_BUDGET_TOKENS),
        recommended_lanes: DEFAULT_TEAM_REQUESTED_LANES,
        fallback: "none",
        model_hint: ModelRouteHint::Keep,
        reason: "measured local metrics and benchmark evidence are within policy limits",
        hint: "current context budget and parallel lane default may proceed within approval and ownership policy",
    }
}
