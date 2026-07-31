use super::*;

#[test]
fn default_chat_budget_exceeds_legacy_512_with_observed_runtime_capacity() {
    let budget = bind_generation_budget(
        GenerationTokenRequest::Intent(GenerationIntent::InteractiveAnswer),
        GenerationRuntimeFacts {
            prompt: GenerationPromptEstimate::exact(975),
            context_window_tokens: Some(131_072),
            timeout_ms: 30_000,
            resource_pressure: ResourcePressure::Normal,
            observed_tokens_per_second: Some(24),
        },
    )
    .unwrap();

    assert_eq!(budget.requested_max_tokens, 672);
    assert!(budget.requested_max_tokens > 512);
    assert_eq!(
        budget.limiting_factors,
        [GenerationLimitingFactor::DeadlineThroughput]
    );
}

#[test]
fn context_window_is_remainder_capacity_not_a_completion_cap() {
    let budget = bind_generation_budget(
        GenerationTokenRequest::Intent(GenerationIntent::InteractiveAnswer),
        GenerationRuntimeFacts {
            prompt: GenerationPromptEstimate::exact(3),
            context_window_tokens: Some(131_072),
            timeout_ms: 30_000,
            resource_pressure: ResourcePressure::Normal,
            observed_tokens_per_second: Some(24),
        },
    )
    .unwrap();

    assert_eq!(budget.requested_max_tokens, 672);
    assert_ne!(budget.requested_max_tokens, 131_072);
}

#[test]
fn untrusted_throughput_is_not_promoted_to_a_quality_cap() {
    let budget = bind_default_backend_budget(
        GenerationTokenRequest::Intent(GenerationIntent::InteractiveAnswer),
        GenerationPromptEstimate::exact(3),
        Some(131_072),
        30_000,
        ResourcePressure::Normal,
    )
    .unwrap();

    assert!(budget.requested_max_tokens > 512);
    assert_eq!(
        budget.limiting_factors,
        [GenerationLimitingFactor::ContextCapacity]
    );
}

#[test]
fn explicit_bound_remains_available_for_governed_callers() {
    let budget = bind_generation_budget(
        GenerationTokenRequest::ExplicitBound(192),
        GenerationRuntimeFacts {
            prompt: GenerationPromptEstimate::exact(2),
            context_window_tokens: Some(4_096),
            timeout_ms: 5_000,
            resource_pressure: ResourcePressure::Normal,
            observed_tokens_per_second: None,
        },
    )
    .unwrap();

    assert_eq!(budget.requested_max_tokens, 192);
    assert_eq!(
        budget.limiting_factors,
        [GenerationLimitingFactor::ExplicitUserOverride]
    );
}

#[test]
fn explicit_bound_is_clamped_to_remaining_context_capacity() {
    let budget = bind_generation_budget(
        GenerationTokenRequest::ExplicitBound(4_096),
        GenerationRuntimeFacts {
            prompt: GenerationPromptEstimate::exact(900),
            context_window_tokens: Some(1_800),
            timeout_ms: 30_000,
            resource_pressure: ResourcePressure::Normal,
            observed_tokens_per_second: None,
        },
    )
    .unwrap();

    assert_eq!(budget.requested_max_tokens, 900);
    assert_eq!(
        budget.limiting_factors,
        [GenerationLimitingFactor::ContextCapacity]
    );
}

#[test]
fn exact_backend_input_tokens_reduce_available_completion_capacity() {
    let smaller_backend_prompt = bind_generation_budget(
        GenerationTokenRequest::ExplicitBound(4_096),
        GenerationRuntimeFacts {
            prompt: GenerationPromptEstimate::exact(250),
            context_window_tokens: Some(2_048),
            timeout_ms: 30_000,
            resource_pressure: ResourcePressure::Normal,
            observed_tokens_per_second: None,
        },
    )
    .unwrap();
    let multimodal_backend_prompt = bind_generation_budget(
        GenerationTokenRequest::ExplicitBound(4_096),
        GenerationRuntimeFacts {
            prompt: GenerationPromptEstimate::exact(1_250),
            context_window_tokens: Some(2_048),
            timeout_ms: 30_000,
            resource_pressure: ResourcePressure::Normal,
            observed_tokens_per_second: None,
        },
    )
    .unwrap();

    assert!(
        multimodal_backend_prompt.requested_max_tokens
            < smaller_backend_prompt.requested_max_tokens
    );
    assert!(multimodal_backend_prompt.requested_max_tokens < 1_000);
}

#[test]
fn explicit_zero_is_rejected_before_runtime_capacity_lookup() {
    let error = bind_generation_budget(
        GenerationTokenRequest::ExplicitBound(0),
        GenerationRuntimeFacts {
            prompt: GenerationPromptEstimate::exact(2),
            context_window_tokens: None,
            timeout_ms: 5_000,
            resource_pressure: ResourcePressure::Normal,
            observed_tokens_per_second: None,
        },
    )
    .unwrap_err();

    assert_eq!(error.code, 2);
    assert!(error.message.contains("1 이상"));
}
