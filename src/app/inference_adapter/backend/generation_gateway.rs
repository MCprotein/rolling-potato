//! Bind observed backend facts to the pure model-aware generation policy.
//!
//! The active context window contributes only remaining capacity. A model
//! completion cap is activated only when source-backed metadata exists.

use crate::foundation::error::AppError;
use crate::runtime_core::inference::generation_policy::{
    ActiveTokenCapacity, AssembledPromptEstimate, BootstrapPromptEstimate, DeadlineCapacityInput,
    EstimatorUncertaintyInput, FinalBudgetInput, GenerationCapacityInputs, GenerationIntent,
    GenerationLimitingFactor, GenerationPolicyError, GenerationPolicyProfileV1,
    ManagedThroughputEvidence, PolicyValueSourceKind, ProvisionalBudgetInput, VersionedValueSource,
};
use crate::runtime_core::inference::resource::{ResourcePressure, DEGRADED_CHAT_MAX_TOKENS};
use crate::runtime_core::knowledge::compaction::estimate_tokens;

const PROMPT_ESTIMATOR_ID: &str = "runtime-core-conservative-text-estimator";
const PROMPT_ESTIMATOR_VERSION: &str = "v1";
const RUNTIME_SNAPSHOT_VERSION: &str = "backend-sidecar-record-v1";
const CHAT_TIMEOUT_CONTRACT_VERSION: &str = "backend-chat-timeout-v1";
const RESOURCE_GOVERNOR_VERSION: &str = "chat-resource-governor-v1";
const EXACT_THROUGHPUT_EVIDENCE_VERSION: &str = "artifact-backend-conservative-tps-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GenerationTokenRequest {
    Intent(GenerationIntent),
    ExplicitBound(u32),
}

impl GenerationTokenRequest {
    pub(super) fn interactive_or_explicit(max_tokens: Option<u32>) -> Self {
        max_tokens.map_or(
            Self::Intent(GenerationIntent::InteractiveAnswer),
            Self::ExplicitBound,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BoundGenerationBudget {
    pub(super) requested_max_tokens: u32,
    pub(super) limiting_factors: Vec<GenerationLimitingFactor>,
}

impl BoundGenerationBudget {
    pub(super) fn limiting_factor_label(&self) -> String {
        format!("{:?}", self.limiting_factors).replace(' ', "")
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct GenerationRuntimeFacts<'a> {
    pub(super) prompt: &'a str,
    pub(super) context_window_tokens: Option<u32>,
    pub(super) timeout_ms: u32,
    pub(super) resource_pressure: ResourcePressure,
    pub(super) observed_tokens_per_second: Option<u32>,
}

pub(super) fn bind_generation_budget(
    request: GenerationTokenRequest,
    facts: GenerationRuntimeFacts<'_>,
) -> Result<BoundGenerationBudget, AppError> {
    match request {
        GenerationTokenRequest::ExplicitBound(tokens) => {
            if tokens == 0 {
                return Err(AppError::usage("max tokens는 1 이상이어야 합니다."));
            }
            Ok(BoundGenerationBudget {
                requested_max_tokens: tokens,
                limiting_factors: vec![GenerationLimitingFactor::ExplicitUserOverride],
            })
        }
        GenerationTokenRequest::Intent(intent) => bind_intent_budget(intent, facts),
    }
}

pub(super) fn bind_default_backend_budget(
    request: GenerationTokenRequest,
    prompt: &str,
    context_window_tokens: Option<u32>,
    timeout_ms: u32,
    resource_pressure: ResourcePressure,
) -> Result<BoundGenerationBudget, AppError> {
    // A model-id average can mix artifacts and backend versions. Until exact
    // conservative evidence exists, transport timeout remains authoritative
    // instead of becoming an invented token cap.
    bind_generation_budget(
        request,
        GenerationRuntimeFacts {
            prompt,
            context_window_tokens,
            timeout_ms,
            resource_pressure,
            observed_tokens_per_second: None,
        },
    )
}

fn bind_intent_budget(
    intent: GenerationIntent,
    facts: GenerationRuntimeFacts<'_>,
) -> Result<BoundGenerationBudget, AppError> {
    let context_window_tokens = facts.context_window_tokens.ok_or_else(|| {
        AppError::blocked(
            "모델 인지형 생성 예산을 계산할 수 없습니다.\n- 이유: 활성 backend의 context size가 기록되지 않았습니다.\n- 다음: 모델을 명시적인 context size로 다시 준비하세요.",
        )
    })?;
    let estimated_prompt_tokens = u32::try_from(estimate_tokens(facts.prompt)).unwrap_or(u32::MAX);
    let capacities = capacities(
        context_window_tokens,
        facts.timeout_ms,
        facts.resource_pressure,
        facts.observed_tokens_per_second,
    );
    let uncertainty = estimator_uncertainty();
    let profile = GenerationPolicyProfileV1::default();
    let provisional = profile
        .provisional_budget(&ProvisionalBudgetInput {
            intent,
            prompt: BootstrapPromptEstimate {
                system_prompt_tokens: 0,
                current_user_input_tokens: estimated_prompt_tokens,
                required_attachment_tokens: 0,
                required_response_schema_tokens: 0,
                known_serializer_framing_tokens: 0,
                uncertainty: uncertainty.clone(),
            },
            capacities: capacities.clone(),
        })
        .map_err(policy_error)?;
    let final_budget = profile
        .final_budget(
            &provisional,
            &FinalBudgetInput {
                intent,
                prompt: AssembledPromptEstimate {
                    prompt_tokens: estimated_prompt_tokens,
                    serializer_system_schema_framing_tokens: 0,
                    uncertainty,
                },
                capacities,
            },
        )
        .map_err(policy_error)?;

    Ok(BoundGenerationBudget {
        requested_max_tokens: final_budget.final_max_tokens,
        limiting_factors: final_budget.limiting_factors,
    })
}

fn capacities(
    context_window_tokens: u32,
    timeout_ms: u32,
    resource_pressure: ResourcePressure,
    observed_tokens_per_second: Option<u32>,
) -> GenerationCapacityInputs {
    let throughput = observed_tokens_per_second
        .filter(|tokens_per_second| *tokens_per_second > 0)
        .map(|tokens_per_second| ManagedThroughputEvidence {
            tokens_per_second,
            source: source(
                PolicyValueSourceKind::ManagedObservation,
                EXACT_THROUGHPUT_EVIDENCE_VERSION,
            ),
        });
    let resource_capacity = (resource_pressure == ResourcePressure::Degraded).then(|| {
        ActiveTokenCapacity::new(
            DEGRADED_CHAT_MAX_TOKENS,
            source(
                PolicyValueSourceKind::ResourceGovernor,
                RESOURCE_GOVERNOR_VERSION,
            ),
        )
    });

    GenerationCapacityInputs {
        context_window_tokens,
        context_source: source(
            PolicyValueSourceKind::RuntimeSnapshot,
            RUNTIME_SNAPSHOT_VERSION,
        ),
        model_completion_cap: None,
        protocol_capacity: None,
        sink_capacity: None,
        deadline: throughput.map(|managed_conservative_observation| DeadlineCapacityInput {
            timeout_ms,
            timeout_source: source(
                PolicyValueSourceKind::IntentContract,
                CHAT_TIMEOUT_CONTRACT_VERSION,
            ),
            throughput: managed_conservative_observation,
        }),
        semantic_capacity: None,
        governance_capacity: None,
        resource_capacity,
        explicit_user_override: None,
    }
}

fn estimator_uncertainty() -> EstimatorUncertaintyInput {
    EstimatorUncertaintyInput {
        estimator_identity: PROMPT_ESTIMATOR_ID.to_string(),
        estimator_version: PROMPT_ESTIMATOR_VERSION.to_string(),
        managed_high_water_error: None,
    }
}

fn source(kind: PolicyValueSourceKind, version: &str) -> VersionedValueSource {
    VersionedValueSource::new(kind, version)
}

fn policy_error(error: GenerationPolicyError) -> AppError {
    match error {
        GenerationPolicyError::InsufficientCapacity { factor } => AppError::blocked(format!(
            "모델 인지형 생성 예산이 부족합니다.\n- limiting factor: {factor:?}\n- 동작: backend 요청을 보내지 않았습니다."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_chat_budget_exceeds_legacy_512_with_observed_runtime_capacity() {
        let budget = bind_generation_budget(
            GenerationTokenRequest::Intent(GenerationIntent::InteractiveAnswer),
            GenerationRuntimeFacts {
                prompt: &"가".repeat(3_900),
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
                prompt: "짧은 질문",
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
            "짧은 질문",
            Some(131_072),
            30_000,
            ResourcePressure::Normal,
        )
        .unwrap();

        assert!(budget.requested_max_tokens > 512);
        assert_eq!(
            budget.limiting_factors,
            [GenerationLimitingFactor::ProvisionalReservation]
        );
    }

    #[test]
    fn explicit_bound_remains_available_for_governed_callers() {
        let budget = bind_generation_budget(
            GenerationTokenRequest::ExplicitBound(192),
            GenerationRuntimeFacts {
                prompt: "benchmark",
                context_window_tokens: None,
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
}
