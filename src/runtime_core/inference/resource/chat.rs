use super::{
    ResourceGovernorAdmission, ResourceGovernorDecision, ResourceGovernorTokenAction,
    ResourcePressure, DEGRADED_CHAT_MAX_TOKENS,
};

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
