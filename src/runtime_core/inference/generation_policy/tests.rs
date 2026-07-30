use super::*;

fn source(kind: PolicyValueSourceKind, version: &str) -> VersionedValueSource {
    VersionedValueSource::new(kind, version)
}

fn uncertainty() -> EstimatorUncertaintyInput {
    EstimatorUncertaintyInput {
        estimator_identity: "fixture-estimator".to_string(),
        estimator_version: "1".to_string(),
        managed_high_water_error: None,
    }
}

fn cap(tokens: u32, kind: PolicyValueSourceKind) -> ActiveTokenCapacity {
    ActiveTokenCapacity::new(tokens, source(kind, "fixture-v1"))
}

fn capacities(context_window_tokens: u32) -> GenerationCapacityInputs {
    GenerationCapacityInputs {
        context_window_tokens,
        context_source: source(PolicyValueSourceKind::RuntimeSnapshot, "snapshot-v1"),
        model_completion_cap: None,
        protocol_capacity: None,
        sink_capacity: None,
        deadline: None,
        semantic_capacity: None,
        governance_capacity: None,
        resource_capacity: None,
        explicit_user_override: None,
    }
}

fn bootstrap(tokens: u32) -> BootstrapPromptEstimate {
    BootstrapPromptEstimate {
        system_prompt_tokens: tokens,
        current_user_input_tokens: 0,
        required_attachment_tokens: 0,
        required_response_schema_tokens: 0,
        known_serializer_framing_tokens: 0,
        uncertainty: uncertainty(),
    }
}

fn assembled(tokens: u32) -> AssembledPromptEstimate {
    AssembledPromptEstimate {
        prompt_tokens: tokens,
        serializer_system_schema_framing_tokens: 0,
        uncertainty: uncertainty(),
    }
}

#[test]
fn incident_fixture_uses_runtime_capacity_instead_of_512() {
    let profile = GenerationPolicyProfileV1::default();
    let mut active = capacities(131_072);
    active.protocol_capacity = Some(cap(65_536, PolicyValueSourceKind::ProtocolContract));
    active.sink_capacity = Some(cap(65_536, PolicyValueSourceKind::SinkContract));
    active.deadline = Some(DeadlineCapacityInput {
        timeout_ms: 30_000,
        timeout_source: source(PolicyValueSourceKind::IntentContract, "chat-timeout-v1"),
        throughput: ThroughputInput {
            managed_conservative_observation: Some(ManagedThroughputEvidence {
                tokens_per_second: 24,
                source: source(
                    PolicyValueSourceKind::ManagedObservation,
                    "model-hash/backend-p10-v1",
                ),
            }),
        },
    });

    let provisional = profile
        .provisional_budget(&ProvisionalBudgetInput {
            intent: GenerationIntent::InteractiveAnswer,
            prompt: bootstrap(1_024),
            capacities: active.clone(),
        })
        .unwrap();
    assert_eq!(provisional.provisional_max_tokens, 672);
    assert_eq!(
        provisional.limiting_factors,
        [GenerationLimitingFactor::DeadlineThroughput]
    );

    let final_budget = profile
        .final_budget(
            &provisional,
            &FinalBudgetInput {
                intent: GenerationIntent::InteractiveAnswer,
                prompt: AssembledPromptEstimate {
                    prompt_tokens: 1_300,
                    serializer_system_schema_framing_tokens: 58,
                    uncertainty: uncertainty(),
                },
                capacities: active,
            },
        )
        .unwrap();

    assert_eq!(final_budget.final_max_tokens, 672);
    assert!(final_budget.final_max_tokens > 512);
    assert_eq!(final_budget.remaining_context_tokens, 129_374);
    assert_eq!(
        final_budget.limiting_factors,
        [GenerationLimitingFactor::DeadlineThroughput]
    );
    assert_eq!(
        final_budget
            .diagnostics
            .selected_throughput
            .as_ref()
            .unwrap()
            .source
            .kind,
        PolicyValueSourceKind::ManagedObservation
    );
}

#[test]
fn absent_optional_factor_is_inactive_but_present_zero_blocks() {
    let profile = GenerationPolicyProfileV1::default();
    let base = ProvisionalBudgetInput {
        intent: GenerationIntent::InteractiveAnswer,
        prompt: bootstrap(100),
        capacities: capacities(4_096),
    };
    let without_cap = profile.provisional_budget(&base).unwrap();

    let mut with_large_cap = base.clone();
    with_large_cap.capacities.model_completion_cap =
        Some(cap(4_096, PolicyValueSourceKind::SourceBackedCapability));
    assert_eq!(
        profile
            .provisional_budget(&with_large_cap)
            .unwrap()
            .provisional_max_tokens,
        without_cap.provisional_max_tokens
    );

    let zero_factors = [
        GenerationLimitingFactor::ModelCompletionCap,
        GenerationLimitingFactor::ProtocolCapacity,
        GenerationLimitingFactor::SinkCapacity,
        GenerationLimitingFactor::SemanticCapacity,
        GenerationLimitingFactor::GovernanceCapacity,
        GenerationLimitingFactor::ResourceCapacity,
        GenerationLimitingFactor::ExplicitUserOverride,
    ];
    for expected in zero_factors {
        let mut input = base.clone();
        let zero = Some(cap(0, PolicyValueSourceKind::IntentContract));
        match expected {
            GenerationLimitingFactor::ModelCompletionCap => {
                input.capacities.model_completion_cap = zero
            }
            GenerationLimitingFactor::ProtocolCapacity => input.capacities.protocol_capacity = zero,
            GenerationLimitingFactor::SinkCapacity => input.capacities.sink_capacity = zero,
            GenerationLimitingFactor::SemanticCapacity => input.capacities.semantic_capacity = zero,
            GenerationLimitingFactor::GovernanceCapacity => {
                input.capacities.governance_capacity = zero
            }
            GenerationLimitingFactor::ResourceCapacity => input.capacities.resource_capacity = zero,
            GenerationLimitingFactor::ExplicitUserOverride => {
                input.capacities.explicit_user_override = zero
            }
            _ => unreachable!(),
        }
        assert_eq!(
            profile.provisional_budget(&input),
            Err(GenerationPolicyError::InsufficientCapacity { factor: expected })
        );
    }
}

#[test]
fn zero_context_and_zero_deadline_block() {
    let profile = GenerationPolicyProfileV1::default();
    let context_error = profile.provisional_budget(&ProvisionalBudgetInput {
        intent: GenerationIntent::InteractiveAnswer,
        prompt: bootstrap(0),
        capacities: capacities(0),
    });
    assert_eq!(
        context_error,
        Err(GenerationPolicyError::InsufficientCapacity {
            factor: GenerationLimitingFactor::ContextCapacity
        })
    );

    for timeout_ms in [0, 1_999, 2_000] {
        let mut active = capacities(4_096);
        active.deadline = Some(DeadlineCapacityInput {
            timeout_ms,
            timeout_source: source(PolicyValueSourceKind::IntentContract, "timeout-v1"),
            throughput: ThroughputInput::default(),
        });
        assert_eq!(
            profile.provisional_budget(&ProvisionalBudgetInput {
                intent: GenerationIntent::InteractiveAnswer,
                prompt: bootstrap(100),
                capacities: active,
            }),
            Err(GenerationPolicyError::InsufficientCapacity {
                factor: GenerationLimitingFactor::DeadlineThroughput
            })
        );
    }
}

#[test]
fn final_pass_is_monotone_for_prompt_and_capacity_changes() {
    let profile = GenerationPolicyProfileV1::default();
    let provisional = profile
        .provisional_budget(&ProvisionalBudgetInput {
            intent: GenerationIntent::InteractiveAnswer,
            prompt: bootstrap(128),
            capacities: capacities(4_096),
        })
        .unwrap();

    let small_prompt = profile
        .final_budget(
            &provisional,
            &FinalBudgetInput {
                intent: GenerationIntent::InteractiveAnswer,
                prompt: assembled(256),
                capacities: capacities(4_096),
            },
        )
        .unwrap();
    let large_prompt = profile
        .final_budget(
            &provisional,
            &FinalBudgetInput {
                intent: GenerationIntent::InteractiveAnswer,
                prompt: assembled(2_048),
                capacities: capacities(4_096),
            },
        )
        .unwrap();

    assert!(small_prompt.final_max_tokens <= provisional.provisional_max_tokens);
    assert!(large_prompt.final_max_tokens <= small_prompt.final_max_tokens);

    let mut tighter = capacities(4_096);
    tighter.sink_capacity = Some(cap(300, PolicyValueSourceKind::SinkContract));
    let tighter_budget = profile
        .final_budget(
            &provisional,
            &FinalBudgetInput {
                intent: GenerationIntent::InteractiveAnswer,
                prompt: assembled(256),
                capacities: tighter,
            },
        )
        .unwrap();
    assert_eq!(tighter_budget.final_max_tokens, 300);
    assert_eq!(
        tighter_budget.limiting_factors,
        [GenerationLimitingFactor::SinkCapacity]
    );
}

#[test]
fn context_prompt_and_cap_matrix_is_monotone() {
    let profile = GenerationPolicyProfileV1::default();
    let contexts = [1_024, 4_096, 131_072, 262_144];
    let prompts = [0, 128, 512, 768];

    for context in contexts {
        let mut previous_for_larger_prompt = u32::MAX;
        for prompt in prompts {
            let result = profile.provisional_budget(&ProvisionalBudgetInput {
                intent: GenerationIntent::InteractiveAnswer,
                prompt: bootstrap(prompt),
                capacities: capacities(context),
            });
            let budget = result
                .map(|budget| budget.provisional_max_tokens)
                .unwrap_or(0);
            assert!(
                budget <= previous_for_larger_prompt,
                "a larger prompt raised the budget for context {context}"
            );
            previous_for_larger_prompt = budget;
        }
    }

    let prompt = bootstrap(128);
    let budgets = contexts.map(|context| {
        profile
            .provisional_budget(&ProvisionalBudgetInput {
                intent: GenerationIntent::InteractiveAnswer,
                prompt: prompt.clone(),
                capacities: capacities(context),
            })
            .unwrap()
            .provisional_max_tokens
    });
    assert!(budgets.windows(2).all(|pair| pair[0] <= pair[1]));

    let caps = [8_192, 4_096, 1_024, 1];
    let capped_budgets = caps.map(|tokens| {
        let mut active = capacities(131_072);
        active.model_completion_cap =
            Some(cap(tokens, PolicyValueSourceKind::SourceBackedCapability));
        profile
            .provisional_budget(&ProvisionalBudgetInput {
                intent: GenerationIntent::InteractiveAnswer,
                prompt: prompt.clone(),
                capacities: active,
            })
            .unwrap()
            .provisional_max_tokens
    });
    assert!(capped_budgets.windows(2).all(|pair| pair[1] <= pair[0]));
}

#[test]
fn final_pass_rejects_new_zero_capacity_before_dispatch() {
    let profile = GenerationPolicyProfileV1::default();
    let provisional = profile
        .provisional_budget(&ProvisionalBudgetInput {
            intent: GenerationIntent::InteractiveAnswer,
            prompt: bootstrap(128),
            capacities: capacities(4_096),
        })
        .unwrap();
    let mut active = capacities(4_096);
    active.protocol_capacity = Some(cap(0, PolicyValueSourceKind::ProtocolContract));

    assert_eq!(
        profile.final_budget(
            &provisional,
            &FinalBudgetInput {
                intent: GenerationIntent::InteractiveAnswer,
                prompt: assembled(256),
                capacities: active,
            }
        ),
        Err(GenerationPolicyError::InsufficientCapacity {
            factor: GenerationLimitingFactor::ProtocolCapacity,
        })
    );
}

#[test]
fn uncertainty_and_deadline_sources_are_versioned() {
    let profile = GenerationPolicyProfileV1::default();
    let mut active = capacities(131_072);
    active.deadline = Some(DeadlineCapacityInput {
        timeout_ms: 30_000,
        timeout_source: source(PolicyValueSourceKind::IntentContract, "chat-timeout-v7"),
        throughput: ThroughputInput::default(),
    });
    let fallback = profile
        .provisional_budget(&ProvisionalBudgetInput {
            intent: GenerationIntent::InteractiveAnswer,
            prompt: bootstrap(1_024),
            capacities: active.clone(),
        })
        .unwrap();
    assert_eq!(fallback.provisional_max_tokens, 224);
    assert_eq!(
        fallback
            .diagnostics
            .selected_throughput
            .as_ref()
            .unwrap()
            .source,
        source(PolicyValueSourceKind::PolicyProfile, "generation-policy-v1")
    );
    assert_eq!(
        fallback.diagnostics.deadline_source,
        Some(source(
            PolicyValueSourceKind::IntentContract,
            "chat-timeout-v7"
        ))
    );

    let mut observed_uncertainty = uncertainty();
    observed_uncertainty.managed_high_water_error = Some(ManagedTokenEvidence {
        tokens: 700,
        source: source(
            PolicyValueSourceKind::ManagedObservation,
            "model-hash/backend-estimator-v3",
        ),
    });
    let observed = profile
        .provisional_budget(&ProvisionalBudgetInput {
            intent: GenerationIntent::InteractiveAnswer,
            prompt: BootstrapPromptEstimate {
                uncertainty: observed_uncertainty,
                ..bootstrap(1_024)
            },
            capacities: active,
        })
        .unwrap();
    assert_eq!(observed.diagnostics.estimator_uncertainty.tokens, 700);
    assert_eq!(
        observed.diagnostics.estimator_uncertainty.source.kind,
        PolicyValueSourceKind::ManagedObservation
    );
}

#[test]
fn arithmetic_saturates_at_u32_boundaries() {
    let profile = GenerationPolicyProfileV1::default();
    let budget = profile
        .provisional_budget(&ProvisionalBudgetInput {
            intent: GenerationIntent::Benchmark,
            prompt: BootstrapPromptEstimate {
                system_prompt_tokens: u32::MAX,
                current_user_input_tokens: u32::MAX,
                required_attachment_tokens: u32::MAX,
                required_response_schema_tokens: u32::MAX,
                known_serializer_framing_tokens: u32::MAX,
                uncertainty: uncertainty(),
            },
            capacities: capacities(u32::MAX),
        })
        .unwrap_err();
    assert_eq!(
        budget,
        GenerationPolicyError::InsufficientCapacity {
            factor: GenerationLimitingFactor::ContextCapacity
        }
    );
}

#[test]
fn profile_defaults_are_explicit_and_versioned() {
    let profile = GenerationPolicyProfileV1::default();
    assert_eq!(profile.policy_profile_version, "generation-policy-v1");
    assert_eq!(profile.bootstrap_unseen_framing_tokens, 128);
    assert_eq!(profile.fallback_estimator_error_bps, 2_500);
    assert_eq!(profile.minimum_estimator_uncertainty_tokens, 128);
    assert_eq!(profile.uncalibrated_throughput_tokens_per_second, 8);
    assert_eq!(profile.deadline_terminal_reserve_ms, 2_000);
}

#[test]
fn every_intent_uses_the_same_capability_driven_calculator() {
    let profile = GenerationPolicyProfileV1::default();
    let intents = [
        GenerationIntent::InteractiveAnswer,
        GenerationIntent::VisionAnswer,
        GenerationIntent::StructuredRouteAndAnswer,
        GenerationIntent::GroundedWebAnswer,
        GenerationIntent::Repair,
        GenerationIntent::AgentAction,
        GenerationIntent::CompactionSummary,
        GenerationIntent::Benchmark,
        GenerationIntent::Collaboration,
        GenerationIntent::ExplicitUserOverride,
    ];
    for intent in intents {
        let budget = profile
            .provisional_budget(&ProvisionalBudgetInput {
                intent,
                prompt: bootstrap(100),
                capacities: capacities(4_096),
            })
            .unwrap();
        assert_eq!(budget.intent, intent);
        assert!(budget.provisional_max_tokens > 512);
    }
}
