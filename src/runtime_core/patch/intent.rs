//! Deterministic intent classification and side-effect-free action planning.

use crate::runtime_core::knowledge::context::ContextPack;

mod classification;
mod model_action;

pub(crate) use classification::{classify, detect_constraints, has_any};
pub(crate) use model_action::{model_action_body, parse_model_action};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentDecision {
    pub skill_id: String,
    pub mode: &'static str,
    pub invocation: &'static str,
    pub signals: Vec<&'static str>,
    pub constraints: Vec<&'static str>,
    pub classifier: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IntentSkill {
    pub id: String,
    pub mode: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ActionCandidate {
    pub kind: &'static str,
    pub approval_required: bool,
    pub next_gate: &'static str,
    pub allowed_side_effects: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedModelAction {
    pub status: &'static str,
    pub kind: String,
    pub source_pointers: String,
    pub next_gate: String,
    pub requested_side_effects: String,
    pub executable_now: bool,
    pub target_path: String,
    pub find_text: String,
    pub replace_text: String,
    pub verification_command: String,
}

pub(crate) fn display_list(values: &[&str]) -> String {
    if values.is_empty() {
        "없음".to_string()
    } else {
        values.join(", ")
    }
}

pub(crate) fn display_optional_u32(value: Option<u32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "없음".to_string())
}

pub(crate) fn display_bool(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

pub(crate) fn plan_action_candidate(
    decision: &IntentDecision,
    context_pack: &ContextPack,
) -> ActionCandidate {
    if decision.skill_id == "conversation" {
        return ActionCandidate {
            kind: "answer-only",
            approval_required: false,
            next_gate: "korean-output-guard",
            allowed_side_effects: "none",
        };
    }
    let has_context = !context_pack.source_pointers.is_empty();
    if matches!(decision.mode, "read-only" | "review-only" | "plan-only") {
        return ActionCandidate {
            kind: if has_context {
                "inspect-sources"
            } else {
                "answer-only"
            },
            approval_required: false,
            next_gate: "source-reread-before-claim",
            allowed_side_effects: "none",
        };
    }

    if decision.signals.contains(&"generated-artifact") {
        return ActionCandidate {
            kind: "generated-artifact-plan",
            approval_required: true,
            next_gate: "diff-before-write",
            allowed_side_effects: "none",
        };
    }

    if matches!(decision.skill_id.as_str(), "fix-test" | "small-patch") {
        return ActionCandidate {
            kind: "patch-proposal",
            approval_required: true,
            next_gate: "diff-before-write",
            allowed_side_effects: "none",
        };
    }

    ActionCandidate {
        kind: "answer-only",
        approval_required: false,
        next_gate: "korean-output-guard",
        allowed_side_effects: "none",
    }
}

#[cfg(test)]
mod tests;
