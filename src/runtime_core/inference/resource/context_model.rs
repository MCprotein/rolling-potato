use super::{
    ContextGovernorAction, ContextModelGovernorDecision, ModelRouteHint, ModelTier,
    ResourceGovernorAdmission, ResourcePressure, DEGRADED_CONTEXT_LIMIT_TOKENS,
    SMALL_MODEL_CONTEXT_SOFT_LIMIT_TOKENS,
};

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
