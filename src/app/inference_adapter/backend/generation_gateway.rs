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
use crate::runtime_core::inference::resource::ResourcePressure;
const PROMPT_ESTIMATOR_ID: &str = "llama.cpp-chat-input-tokens";
const PROMPT_ESTIMATOR_VERSION: &str = "b9982-input-tokens-v1";
const RUNTIME_SNAPSHOT_VERSION: &str = "backend-sidecar-record-v1";
const CHAT_TIMEOUT_CONTRACT_VERSION: &str = "backend-chat-timeout-v1";
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
pub(super) struct GenerationRuntimeFacts {
    pub(super) prompt: GenerationPromptEstimate,
    pub(super) context_window_tokens: Option<u32>,
    pub(super) timeout_ms: u32,
    pub(super) resource_pressure: ResourcePressure,
    pub(super) observed_tokens_per_second: Option<u32>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct GenerationPromptEstimate {
    pub(super) exact_input_tokens: u32,
}

impl GenerationPromptEstimate {
    pub(super) fn exact(exact_input_tokens: u32) -> Self {
        Self { exact_input_tokens }
    }
}

pub(super) fn bind_generation_budget(
    request: GenerationTokenRequest,
    facts: GenerationRuntimeFacts,
) -> Result<BoundGenerationBudget, AppError> {
    if matches!(request, GenerationTokenRequest::ExplicitBound(0)) {
        return Err(AppError::usage("max tokens는 1 이상이어야 합니다."));
    }
    bind_budget(request, facts)
}

pub(super) fn bind_default_backend_budget(
    request: GenerationTokenRequest,
    prompt: GenerationPromptEstimate,
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

fn bind_budget(
    request: GenerationTokenRequest,
    facts: GenerationRuntimeFacts,
) -> Result<BoundGenerationBudget, AppError> {
    let intent = match request {
        GenerationTokenRequest::Intent(intent) => intent,
        GenerationTokenRequest::ExplicitBound(_) => GenerationIntent::InteractiveAnswer,
    };
    let context_window_tokens = facts.context_window_tokens.ok_or_else(|| {
        AppError::blocked(
            "모델 인지형 생성 예산을 계산할 수 없습니다.\n- 이유: 활성 backend의 context size가 기록되지 않았습니다.\n- 다음: 모델을 명시적인 context size로 다시 준비하세요.",
        )
    })?;
    let estimated_prompt_tokens = facts.prompt.exact_input_tokens;
    let mut capacities = capacities(
        context_window_tokens,
        facts.timeout_ms,
        facts.resource_pressure,
        facts.observed_tokens_per_second,
    );
    if let GenerationTokenRequest::ExplicitBound(tokens) = request {
        capacities.explicit_user_override = Some(ActiveTokenCapacity::new(
            tokens,
            source(
                PolicyValueSourceKind::IntentContract,
                "explicit-generation-bound-v1",
            ),
        ));
    }
    let uncertainty = estimator_uncertainty();
    let profile = exact_prompt_policy_profile();
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

    let limiting_factors = if matches!(request, GenerationTokenRequest::ExplicitBound(_))
        && final_budget.limiting_factors == [GenerationLimitingFactor::ProvisionalReservation]
    {
        provisional.limiting_factors
    } else {
        final_budget.limiting_factors
    };
    Ok(BoundGenerationBudget {
        requested_max_tokens: final_budget.final_max_tokens,
        limiting_factors,
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
    // Runtime resource admission and clamping are owned by the resource
    // governor. Keeping that decision out of the generation policy preserves
    // the policy-requested value so diagnostics can show the exact
    // policy-requested -> transport-effective transition.
    let _ = resource_pressure;

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
        resource_capacity: None,
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

fn exact_prompt_policy_profile() -> GenerationPolicyProfileV1 {
    GenerationPolicyProfileV1 {
        bootstrap_unseen_framing_tokens: 0,
        fallback_estimator_error_bps: 0,
        minimum_estimator_uncertainty_tokens: 0,
        ..GenerationPolicyProfileV1::default()
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
#[path = "generation_gateway/tests.rs"]
mod tests;
