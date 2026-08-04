//! Pure, model-aware generation budget policy.
//!
//! This module owns calculation only. Runtime binding, prompt assembly, backend
//! dispatch, persistence, and presentation remain application/adaptor concerns.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GenerationIntent {
    InteractiveAnswer,
    VisionAnswer,
    StructuredRouteAndAnswer,
    StructuredToolRoute,
    GroundedWebAnswer,
    Repair,
    AgentAction,
    CompactionSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GenerationLimitingFactor {
    ContextCapacity,
    ModelCompletionCap,
    ProtocolCapacity,
    SinkCapacity,
    DeadlineThroughput,
    SemanticCapacity,
    GovernanceCapacity,
    ResourceCapacity,
    ExplicitUserOverride,
    ProvisionalReservation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PolicyValueSourceKind {
    PolicyProfile,
    RuntimeSnapshot,
    #[allow(dead_code)]
    SourceBackedCapability,
    #[allow(dead_code)]
    ProtocolContract,
    #[allow(dead_code)]
    SinkContract,
    IntentContract,
    ManagedObservation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VersionedValueSource {
    pub kind: PolicyValueSourceKind,
    pub version: String,
}

impl VersionedValueSource {
    pub(crate) fn new(kind: PolicyValueSourceKind, version: impl Into<String>) -> Self {
        Self {
            kind,
            version: version.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ActiveTokenCapacity {
    pub tokens: u32,
    pub source: VersionedValueSource,
}

impl ActiveTokenCapacity {
    pub(crate) fn new(tokens: u32, source: VersionedValueSource) -> Self {
        Self { tokens, source }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManagedTokenEvidence {
    pub tokens: u32,
    pub source: VersionedValueSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EstimatorUncertaintyInput {
    pub estimator_identity: String,
    pub estimator_version: String,
    pub managed_high_water_error: Option<ManagedTokenEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManagedThroughputEvidence {
    pub tokens_per_second: u32,
    pub source: VersionedValueSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeadlineCapacityInput {
    pub timeout_ms: u32,
    pub timeout_source: VersionedValueSource,
    pub throughput: ManagedThroughputEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BootstrapPromptEstimate {
    pub system_prompt_tokens: u32,
    pub current_user_input_tokens: u32,
    pub required_attachment_tokens: u32,
    pub required_response_schema_tokens: u32,
    pub known_serializer_framing_tokens: u32,
    pub uncertainty: EstimatorUncertaintyInput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AssembledPromptEstimate {
    pub prompt_tokens: u32,
    pub serializer_system_schema_framing_tokens: u32,
    pub uncertainty: EstimatorUncertaintyInput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GenerationCapacityInputs {
    pub context_window_tokens: u32,
    pub context_source: VersionedValueSource,
    pub model_completion_cap: Option<ActiveTokenCapacity>,
    pub protocol_capacity: Option<ActiveTokenCapacity>,
    pub sink_capacity: Option<ActiveTokenCapacity>,
    pub deadline: Option<DeadlineCapacityInput>,
    pub semantic_capacity: Option<ActiveTokenCapacity>,
    pub governance_capacity: Option<ActiveTokenCapacity>,
    pub resource_capacity: Option<ActiveTokenCapacity>,
    pub explicit_user_override: Option<ActiveTokenCapacity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProvisionalBudgetInput {
    pub intent: GenerationIntent,
    pub prompt: BootstrapPromptEstimate,
    pub capacities: GenerationCapacityInputs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FinalBudgetInput {
    pub intent: GenerationIntent,
    pub prompt: AssembledPromptEstimate,
    pub capacities: GenerationCapacityInputs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectedUncertainty {
    pub tokens: u32,
    pub source: VersionedValueSource,
    pub estimator_identity: String,
    pub estimator_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectedThroughput {
    pub tokens_per_second: u32,
    pub source: VersionedValueSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EvaluatedCapacity {
    pub factor: GenerationLimitingFactor,
    pub tokens: u32,
    pub source: VersionedValueSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GenerationBudgetDiagnostics {
    pub policy_profile_version: &'static str,
    pub prompt_estimate_tokens: u32,
    pub estimator_uncertainty: SelectedUncertainty,
    pub deadline_timeout_ms: Option<u32>,
    pub deadline_source: Option<VersionedValueSource>,
    pub deadline_capacity_tokens: Option<u32>,
    pub selected_throughput: Option<SelectedThroughput>,
    pub active_capacities: Vec<EvaluatedCapacity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProvisionalGenerationBudget {
    pub intent: GenerationIntent,
    pub provisional_max_tokens: u32,
    pub remaining_context_tokens: u32,
    pub limiting_factors: Vec<GenerationLimitingFactor>,
    pub diagnostics: GenerationBudgetDiagnostics,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FinalGenerationBudget {
    pub intent: GenerationIntent,
    pub provisional_max_tokens: u32,
    pub final_max_tokens: u32,
    pub remaining_context_tokens: u32,
    pub limiting_factors: Vec<GenerationLimitingFactor>,
    pub diagnostics: GenerationBudgetDiagnostics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GenerationPolicyError {
    InsufficientCapacity { factor: GenerationLimitingFactor },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GenerationPolicyProfileV1 {
    pub policy_profile_version: &'static str,
    pub prompt_output_reserve_bps: u32,
    pub bootstrap_unseen_framing_tokens: u32,
    pub fallback_estimator_error_bps: u32,
    pub minimum_estimator_uncertainty_tokens: u32,
    pub deadline_terminal_reserve_ms: u32,
}

impl Default for GenerationPolicyProfileV1 {
    fn default() -> Self {
        Self {
            policy_profile_version: "generation-policy-v1",
            prompt_output_reserve_bps: 2_500,
            bootstrap_unseen_framing_tokens: 128,
            fallback_estimator_error_bps: 2_500,
            minimum_estimator_uncertainty_tokens: 128,
            deadline_terminal_reserve_ms: 2_000,
        }
    }
}

impl GenerationPolicyProfileV1 {
    pub(crate) fn prompt_output_reserve(
        &self,
        context_window_tokens: u32,
    ) -> Result<u32, GenerationPolicyError> {
        let tokens = ceil_basis_points(context_window_tokens, self.prompt_output_reserve_bps);
        if tokens == 0 {
            return Err(GenerationPolicyError::InsufficientCapacity {
                factor: GenerationLimitingFactor::ContextCapacity,
            });
        }
        Ok(tokens)
    }

    pub(crate) fn provisional_budget(
        &self,
        input: &ProvisionalBudgetInput,
    ) -> Result<ProvisionalGenerationBudget, GenerationPolicyError> {
        let prompt_estimate_tokens = saturating_sum(&[
            input.prompt.system_prompt_tokens,
            input.prompt.current_user_input_tokens,
            input.prompt.required_attachment_tokens,
            input.prompt.required_response_schema_tokens,
            input.prompt.known_serializer_framing_tokens,
            self.bootstrap_unseen_framing_tokens,
        ]);
        let calculation = self.calculate(
            prompt_estimate_tokens,
            &input.prompt.uncertainty,
            &input.capacities,
        )?;

        Ok(ProvisionalGenerationBudget {
            intent: input.intent,
            provisional_max_tokens: calculation.selected_tokens,
            remaining_context_tokens: calculation.remaining_context_tokens,
            limiting_factors: calculation.limiting_factors,
            diagnostics: calculation.diagnostics,
        })
    }

    pub(crate) fn final_budget(
        &self,
        provisional: &ProvisionalGenerationBudget,
        input: &FinalBudgetInput,
    ) -> Result<FinalGenerationBudget, GenerationPolicyError> {
        let prompt_estimate_tokens = saturating_sum(&[
            input.prompt.prompt_tokens,
            input.prompt.serializer_system_schema_framing_tokens,
        ]);
        let mut calculation = self.calculate(
            prompt_estimate_tokens,
            &input.prompt.uncertainty,
            &input.capacities,
        )?;

        if provisional.provisional_max_tokens < calculation.selected_tokens {
            calculation.selected_tokens = provisional.provisional_max_tokens;
            calculation.limiting_factors = vec![GenerationLimitingFactor::ProvisionalReservation];
        }

        Ok(FinalGenerationBudget {
            intent: input.intent,
            provisional_max_tokens: provisional.provisional_max_tokens,
            final_max_tokens: calculation.selected_tokens,
            remaining_context_tokens: calculation.remaining_context_tokens,
            limiting_factors: calculation.limiting_factors,
            diagnostics: calculation.diagnostics,
        })
    }

    fn calculate(
        &self,
        prompt_estimate_tokens: u32,
        uncertainty_input: &EstimatorUncertaintyInput,
        capacities: &GenerationCapacityInputs,
    ) -> Result<BudgetCalculation, GenerationPolicyError> {
        let estimator_uncertainty =
            self.select_uncertainty(prompt_estimate_tokens, uncertainty_input);
        let remaining_context_tokens = capacities
            .context_window_tokens
            .saturating_sub(prompt_estimate_tokens)
            .saturating_sub(estimator_uncertainty.tokens);

        let (deadline_capacity_tokens, selected_throughput) = capacities
            .deadline
            .as_ref()
            .map(|deadline| {
                let throughput = SelectedThroughput {
                    tokens_per_second: deadline.throughput.tokens_per_second,
                    source: deadline.throughput.source.clone(),
                };
                let available_ms = deadline
                    .timeout_ms
                    .saturating_sub(self.deadline_terminal_reserve_ms);
                let capacity = u64::from(available_ms)
                    .saturating_mul(u64::from(throughput.tokens_per_second))
                    / 1_000;
                (capacity.min(u64::from(u32::MAX)) as u32, throughput)
            })
            .unzip();

        let active_factors = [
            Some(EvaluatedCapacity {
                factor: GenerationLimitingFactor::ContextCapacity,
                tokens: remaining_context_tokens,
                source: capacities.context_source.clone(),
            }),
            optional_factor(
                GenerationLimitingFactor::ModelCompletionCap,
                capacities.model_completion_cap.as_ref(),
            ),
            optional_factor(
                GenerationLimitingFactor::ProtocolCapacity,
                capacities.protocol_capacity.as_ref(),
            ),
            optional_factor(
                GenerationLimitingFactor::SinkCapacity,
                capacities.sink_capacity.as_ref(),
            ),
            capacities
                .deadline
                .as_ref()
                .zip(deadline_capacity_tokens)
                .map(|(deadline, tokens)| EvaluatedCapacity {
                    factor: GenerationLimitingFactor::DeadlineThroughput,
                    tokens,
                    source: deadline.timeout_source.clone(),
                }),
            optional_factor(
                GenerationLimitingFactor::SemanticCapacity,
                capacities.semantic_capacity.as_ref(),
            ),
            optional_factor(
                GenerationLimitingFactor::GovernanceCapacity,
                capacities.governance_capacity.as_ref(),
            ),
            optional_factor(
                GenerationLimitingFactor::ResourceCapacity,
                capacities.resource_capacity.as_ref(),
            ),
            optional_factor(
                GenerationLimitingFactor::ExplicitUserOverride,
                capacities.explicit_user_override.as_ref(),
            ),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

        let mut selected_tokens = u32::MAX;
        let mut limiting_factors = Vec::new();
        for capacity in &active_factors {
            if capacity.tokens == 0 {
                return Err(GenerationPolicyError::InsufficientCapacity {
                    factor: capacity.factor,
                });
            }
            if capacity.tokens < selected_tokens {
                selected_tokens = capacity.tokens;
                limiting_factors.clear();
                limiting_factors.push(capacity.factor);
            } else if capacity.tokens == selected_tokens {
                limiting_factors.push(capacity.factor);
            }
        }

        Ok(BudgetCalculation {
            selected_tokens,
            remaining_context_tokens,
            limiting_factors,
            diagnostics: GenerationBudgetDiagnostics {
                policy_profile_version: self.policy_profile_version,
                prompt_estimate_tokens,
                estimator_uncertainty,
                deadline_timeout_ms: capacities.deadline.as_ref().map(|value| value.timeout_ms),
                deadline_source: capacities
                    .deadline
                    .as_ref()
                    .map(|value| value.timeout_source.clone()),
                deadline_capacity_tokens,
                selected_throughput,
                active_capacities: active_factors,
            },
        })
    }

    fn select_uncertainty(
        &self,
        prompt_estimate_tokens: u32,
        input: &EstimatorUncertaintyInput,
    ) -> SelectedUncertainty {
        let ratio_tokens =
            ceil_basis_points(prompt_estimate_tokens, self.fallback_estimator_error_bps);
        let fallback_tokens = ratio_tokens.max(self.minimum_estimator_uncertainty_tokens);
        let profile_source = VersionedValueSource::new(
            PolicyValueSourceKind::PolicyProfile,
            self.policy_profile_version,
        );
        let (tokens, source) = input
            .managed_high_water_error
            .as_ref()
            .filter(|evidence| evidence.tokens >= fallback_tokens)
            .map_or((fallback_tokens, profile_source), |evidence| {
                (evidence.tokens, evidence.source.clone())
            });

        SelectedUncertainty {
            tokens,
            source,
            estimator_identity: input.estimator_identity.clone(),
            estimator_version: input.estimator_version.clone(),
        }
    }
}

#[derive(Debug)]
struct BudgetCalculation {
    selected_tokens: u32,
    remaining_context_tokens: u32,
    limiting_factors: Vec<GenerationLimitingFactor>,
    diagnostics: GenerationBudgetDiagnostics,
}

fn optional_factor(
    factor: GenerationLimitingFactor,
    capacity: Option<&ActiveTokenCapacity>,
) -> Option<EvaluatedCapacity> {
    capacity.map(|capacity| EvaluatedCapacity {
        factor,
        tokens: capacity.tokens,
        source: capacity.source.clone(),
    })
}

fn saturating_sum(values: &[u32]) -> u32 {
    values
        .iter()
        .fold(0_u32, |total, value| total.saturating_add(*value))
}

fn ceil_basis_points(tokens: u32, basis_points: u32) -> u32 {
    if tokens == 0 || basis_points == 0 {
        return 0;
    }
    let numerator = u64::from(tokens)
        .saturating_mul(u64::from(basis_points))
        .saturating_add(9_999);
    (numerator / 10_000).min(u64::from(u32::MAX)) as u32
}

#[cfg(test)]
mod tests;
