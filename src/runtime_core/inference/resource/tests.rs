use super::*;

#[test]
fn classify_pressure_handles_unknown_normal_and_thresholds() {
    assert_eq!(
        classify_pressure(None, None, None),
        ResourcePressure::Unknown
    );
    assert_eq!(
        classify_pressure(Some(12.5), Some(512 * 1024 * 1024), None),
        ResourcePressure::Normal
    );
    assert_eq!(
        classify_pressure(Some(80.0), Some(512 * 1024 * 1024), None),
        ResourcePressure::Degraded
    );
    assert_eq!(
        classify_pressure(Some(20.0), None, Some(12 * GIB_BYTES)),
        ResourcePressure::Critical
    );
}

#[test]
fn chat_governor_allows_clamps_and_blocks_by_pressure() {
    let normal = chat_governor_decision(ResourcePressure::Normal, 512);
    assert_eq!(normal.admission, ResourceGovernorAdmission::Allow);
    assert_eq!(normal.token_action, ResourceGovernorTokenAction::Unchanged);
    assert_eq!(normal.effective_max_tokens, Some(512));

    let degraded = chat_governor_decision(ResourcePressure::Degraded, 512);
    assert_eq!(degraded.admission, ResourceGovernorAdmission::Allow);
    assert_eq!(degraded.token_action, ResourceGovernorTokenAction::Clamped);
    assert_eq!(
        degraded.effective_max_tokens,
        Some(DEGRADED_CHAT_MAX_TOKENS)
    );

    let small_degraded = chat_governor_decision(ResourcePressure::Degraded, 64);
    assert_eq!(
        small_degraded.token_action,
        ResourceGovernorTokenAction::Unchanged
    );
    assert_eq!(small_degraded.effective_max_tokens, Some(64));

    let critical = chat_governor_decision(ResourcePressure::Critical, 512);
    assert!(critical.is_blocked());
    assert_eq!(critical.token_action, ResourceGovernorTokenAction::Blocked);
    assert_eq!(critical.effective_max_tokens, None);
}

#[test]
fn team_lane_decision_allows_sequential_fallback_and_blocks() {
    let normal = team_lane_decision(ResourcePressure::Normal, 3);
    assert_eq!(normal.admission, ResourceLaneAdmission::AllowParallel);
    assert_eq!(normal.admitted_lanes, 3);

    let unknown = team_lane_decision(ResourcePressure::Unknown, 3);
    assert_eq!(unknown.admission, ResourceLaneAdmission::SequentialFallback);
    assert_eq!(unknown.admitted_lanes, 1);
    assert_eq!(unknown.fallback, "sequential");

    let degraded = team_lane_decision(ResourcePressure::Degraded, 3);
    assert_eq!(
        degraded.admission,
        ResourceLaneAdmission::SequentialFallback
    );
    assert_eq!(degraded.admitted_lanes, 1);

    let critical = team_lane_decision(ResourcePressure::Critical, 3);
    assert!(critical.is_blocked());
    assert_eq!(critical.admitted_lanes, 0);
    assert_eq!(critical.fallback, "wait");
}

#[test]
fn context_model_governor_clamps_and_hints_without_model_claims() {
    let normal_small =
        context_model_governor_decision(ResourcePressure::Normal, 6000, 8192, ModelTier::Small);
    assert_eq!(normal_small.context_action, ContextGovernorAction::Clamped);
    assert_eq!(
        normal_small.effective_context_tokens,
        Some(SMALL_MODEL_CONTEXT_SOFT_LIMIT_TOKENS)
    );
    assert_eq!(normal_small.model_hint, ModelRouteHint::Escalate);

    let degraded_large =
        context_model_governor_decision(ResourcePressure::Degraded, 8000, 8192, ModelTier::Large);
    assert_eq!(
        degraded_large.context_action,
        ContextGovernorAction::Clamped
    );
    assert_eq!(
        degraded_large.effective_context_tokens,
        Some(DEGRADED_CONTEXT_LIMIT_TOKENS)
    );
    assert_eq!(degraded_large.model_hint, ModelRouteHint::Downgrade);

    let critical = context_model_governor_decision(
        ResourcePressure::Critical,
        1024,
        4096,
        ModelTier::Standard,
    );
    assert!(critical.is_blocked());
    assert_eq!(critical.context_action, ContextGovernorAction::Blocked);
    assert_eq!(critical.model_hint, ModelRouteHint::Defer);
    assert_eq!(critical.effective_context_tokens, None);
}

#[test]
fn optimization_policy_uses_local_metrics_and_benchmark_evidence() {
    let healthy = optimization_policy_decision(OptimizationPolicyInput {
        pressure: ResourcePressure::Normal,
        model_runs: 2,
        measured_benchmark_runs: 1,
        failed_benchmark_runs: 0,
        context_clamp_count: 0,
        p95_latency_ms: Some(200.0),
        avg_tokens_per_second: Some(25.0),
    });
    assert_eq!(healthy.status, OptimizationPolicyStatus::Recommend);
    assert_eq!(
        healthy.recommended_context_tokens,
        Some(DEFAULT_CONTEXT_LIMIT_TOKENS)
    );
    assert_eq!(healthy.recommended_lanes, DEFAULT_TEAM_REQUESTED_LANES);
    assert_eq!(healthy.model_hint, ModelRouteHint::Keep);

    let failed_benchmark = optimization_policy_decision(OptimizationPolicyInput {
        pressure: ResourcePressure::Normal,
        model_runs: 2,
        measured_benchmark_runs: 2,
        failed_benchmark_runs: 1,
        context_clamp_count: 0,
        p95_latency_ms: Some(200.0),
        avg_tokens_per_second: Some(25.0),
    });
    assert_eq!(
        failed_benchmark.status,
        OptimizationPolicyStatus::Constrained
    );
    assert_eq!(failed_benchmark.recommended_lanes, 1);
    assert_eq!(failed_benchmark.model_hint, ModelRouteHint::Escalate);

    let no_evidence = optimization_policy_decision(OptimizationPolicyInput {
        pressure: ResourcePressure::Normal,
        model_runs: 0,
        measured_benchmark_runs: 0,
        failed_benchmark_runs: 0,
        context_clamp_count: 0,
        p95_latency_ms: None,
        avg_tokens_per_second: None,
    });
    assert_eq!(
        no_evidence.status,
        OptimizationPolicyStatus::InsufficientEvidence
    );
    assert_eq!(no_evidence.recommended_lanes, 1);

    let critical = optimization_policy_decision(OptimizationPolicyInput {
        pressure: ResourcePressure::Critical,
        model_runs: 2,
        measured_benchmark_runs: 1,
        failed_benchmark_runs: 0,
        context_clamp_count: 0,
        p95_latency_ms: Some(200.0),
        avg_tokens_per_second: Some(25.0),
    });
    assert_eq!(critical.status, OptimizationPolicyStatus::Blocked);
    assert_eq!(critical.recommended_context_tokens, None);
    assert_eq!(critical.model_hint, ModelRouteHint::Defer);
}
