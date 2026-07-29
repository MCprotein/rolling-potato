//! Team resource governor decisions and event labels.

use crate::runtime_core::inference::resource;

pub(crate) fn pressure_from_status(value: &str) -> resource::ResourcePressure {
    match value {
        "normal" => resource::ResourcePressure::Normal,
        "degraded" => resource::ResourcePressure::Degraded,
        "critical" => resource::ResourcePressure::Critical,
        _ => resource::ResourcePressure::Unknown,
    }
}

pub(crate) fn governor_status(
    context_decision: &resource::ContextModelGovernorDecision,
    lane_decision: &resource::ResourceLaneDecision,
) -> &'static str {
    if context_decision.is_blocked() || lane_decision.is_blocked() {
        "blocked"
    } else if context_decision.context_action == resource::ContextGovernorAction::Clamped {
        "clamped"
    } else if context_decision.model_hint != resource::ModelRouteHint::Keep {
        "hinted"
    } else {
        "allowed"
    }
}

pub(crate) fn governor_event_type(status: &str) -> &'static str {
    match status {
        "blocked" => "team.governor.blocked",
        "clamped" => "team.governor.clamped",
        "hinted" => "team.governor.hinted",
        _ => "team.governor.allowed",
    }
}

pub(crate) fn governor_summary(status: &str) -> &'static str {
    match status {
        "blocked" => "team governor blocked",
        "clamped" => "team governor context clamped",
        "hinted" => "team governor model route hinted",
        _ => "team governor allowed",
    }
}
