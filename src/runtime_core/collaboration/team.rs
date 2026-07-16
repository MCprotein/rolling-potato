//! Team admission, dispatch, continuation, and governor decision policy.

use crate::runtime_core::inference::resource;
use crate::runtime_core::policy::decision::Decision;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PolicyGate {
    pub status: &'static str,
    pub checks: Vec<PolicyCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PolicyCheck {
    pub target_type: &'static str,
    pub target: String,
    pub decision: Decision,
    pub class: &'static str,
    pub approval_prompt: &'static str,
    pub reason: String,
}

impl PolicyGate {
    pub(crate) fn is_blocked(&self) -> bool {
        matches!(self.status, "approval-required" | "blocked")
    }

    pub(crate) fn blocked_label(&self) -> &'static str {
        if self.is_blocked() {
            "yes"
        } else {
            "no"
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OwnershipGate {
    pub status: &'static str,
    pub checks: Vec<OwnershipCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OwnershipCheck {
    pub lane: u32,
    pub raw_path: String,
    pub normalized_path: String,
    pub status: &'static str,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OwnershipClaim {
    pub lane: u32,
    pub raw_path: String,
    pub normalized_path: String,
}

impl OwnershipGate {
    pub(crate) fn is_blocked(&self) -> bool {
        matches!(self.status, "invalid" | "conflict")
    }

    pub(crate) fn blocked_label(&self) -> &'static str {
        if self.is_blocked() {
            "yes"
        } else {
            "no"
        }
    }
}

pub(crate) fn policy_write_paths(
    write_paths: &[String],
    owned_write_paths: &[(u32, String)],
) -> Vec<String> {
    let mut paths = write_paths.to_vec();
    paths.extend(owned_write_paths.iter().map(|(_, path)| path.clone()));
    paths
}

pub(crate) fn evaluate_policy_gate(checks: Vec<PolicyCheck>) -> PolicyGate {
    let status = if checks.is_empty() {
        "not-requested"
    } else if checks.iter().any(|check| check.decision == Decision::Deny) {
        "blocked"
    } else if checks.iter().any(|check| check.decision == Decision::Ask) {
        "approval-required"
    } else {
        "allowed"
    };
    PolicyGate { status, checks }
}

pub(crate) fn evaluate_ownership_gate(
    admitted_lanes: u32,
    claims: Vec<OwnershipClaim>,
) -> OwnershipGate {
    if claims.is_empty() {
        return OwnershipGate {
            status: "not-requested",
            checks: Vec::new(),
        };
    }

    let mut owners: HashMap<String, u32> = HashMap::new();
    let mut checks = Vec::new();
    for claim in claims {
        let mut status = "assigned";
        let mut reason = "write path assigned to lane before dispatch".to_string();
        if claim.lane > admitted_lanes {
            status = "invalid";
            reason = format!(
                "lane {} exceeds admitted lanes {admitted_lanes}; reduce lanes or wait for resources",
                claim.lane
            );
        } else if let Some(existing_lane) = owners.get(&claim.normalized_path) {
            if *existing_lane != claim.lane {
                status = "conflict";
                reason = format!(
                    "path already owned by lane {existing_lane}; cross-lane writes are blocked"
                );
            }
        } else {
            owners.insert(claim.normalized_path.clone(), claim.lane);
        }
        checks.push(OwnershipCheck {
            lane: claim.lane,
            raw_path: claim.raw_path,
            normalized_path: claim.normalized_path,
            status,
            reason,
        });
    }

    let status = if checks.iter().any(|check| check.status == "conflict") {
        "conflict"
    } else if checks.iter().any(|check| check.status == "invalid") {
        "invalid"
    } else {
        "allocated"
    };
    OwnershipGate { status, checks }
}

pub(crate) fn decision_label(decision: Decision) -> &'static str {
    match decision {
        Decision::Allow => "allow",
        Decision::Ask => "ask",
        Decision::Deny => "deny",
    }
}

pub(crate) fn pressure_from_status(value: &str) -> resource::ResourcePressure {
    match value {
        "normal" => resource::ResourcePressure::Normal,
        "degraded" => resource::ResourcePressure::Degraded,
        "critical" => resource::ResourcePressure::Critical,
        _ => resource::ResourcePressure::Unknown,
    }
}

pub(crate) fn is_team_runtime_event(event_type: &str) -> bool {
    event_type.starts_with("team.admission.")
        || event_type.starts_with("team.dispatch.")
        || event_type.starts_with("team.continuation.")
        || event_type.starts_with("team.governor.")
        || event_type.starts_with("team.worker.")
        || event_type.starts_with("team.subagent.")
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

fn admission_status(admission: resource::ResourceLaneAdmission) -> &'static str {
    match admission {
        resource::ResourceLaneAdmission::AllowParallel => "admitted",
        resource::ResourceLaneAdmission::SequentialFallback => "sequential-fallback",
        resource::ResourceLaneAdmission::Blocked => "blocked",
    }
}

pub(crate) fn overall_status(
    admission: resource::ResourceLaneAdmission,
    blocked_by_policy: bool,
    blocked_by_ownership: bool,
) -> &'static str {
    if admission == resource::ResourceLaneAdmission::Blocked {
        return "blocked";
    }
    if blocked_by_ownership {
        return "ownership-blocked";
    }
    if blocked_by_policy {
        return "policy-blocked";
    }
    admission_status(admission)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ContinuationDecision {
    pub status: &'static str,
    pub action: &'static str,
    pub remaining_lanes: u32,
    pub reason: String,
    pub hint: &'static str,
}

impl ContinuationDecision {
    pub(crate) fn is_blocked(&self) -> bool {
        self.status == "blocked"
    }
}

pub(crate) fn continuation_decision(
    admitted_lanes: u32,
    failed_lane: Option<u32>,
    redacted_failure_reason: &str,
) -> ContinuationDecision {
    let Some(failed_lane) = failed_lane else {
        return ContinuationDecision {
            status: "not-requested",
            action: "none",
            remaining_lanes: admitted_lanes,
            reason: "no failed worker reported".to_string(),
            hint: "dispatch may proceed without continuation handling if other gates allow it",
        };
    };

    if failed_lane == 0 || failed_lane > admitted_lanes {
        return ContinuationDecision {
            status: "blocked",
            action: "none",
            remaining_lanes: 0,
            reason: format!(
                "failed lane {failed_lane} is outside admitted lanes {admitted_lanes}; cannot continue safely"
            ),
            hint: "re-run admission with current resources and a valid failed lane before continuing",
        };
    }

    if admitted_lanes <= 1 {
        return ContinuationDecision {
            status: "blocked",
            action: "wait",
            remaining_lanes: 0,
            reason: "no remaining admitted lanes after the failed worker".to_string(),
            hint: "resume as a single-agent repair or re-run admission after resources recover",
        };
    }

    ContinuationDecision {
        status: "continue-with-remaining",
        action: "continue",
        remaining_lanes: admitted_lanes - 1,
        reason: format!(
            "lane {failed_lane} is excluded after failure; reason recorded as {redacted_failure_reason}"
        ),
        hint: "continue only unfailed lanes and keep file ownership boundaries unchanged",
    }
}

pub(crate) fn dispatch_status(
    admission: resource::ResourceLaneAdmission,
    blocked_by_ownership: bool,
    continuation: &ContinuationDecision,
) -> &'static str {
    if admission == resource::ResourceLaneAdmission::Blocked || continuation.is_blocked() {
        return "blocked";
    }
    if blocked_by_ownership {
        return "ownership-blocked";
    }
    if continuation.status == "continue-with-remaining" {
        return "continuation-ready";
    }
    admission_status(admission)
}

pub(crate) fn dispatch_event_type(
    admission: resource::ResourceLaneAdmission,
    blocked_by_ownership: bool,
    continuation: &ContinuationDecision,
) -> &'static str {
    if admission == resource::ResourceLaneAdmission::Blocked {
        return "team.dispatch.blocked";
    }
    if blocked_by_ownership {
        return "team.dispatch.ownership_blocked";
    }
    if continuation.is_blocked() {
        return "team.continuation.blocked";
    }
    if continuation.status == "continue-with-remaining" {
        return "team.continuation.recorded";
    }
    match admission {
        resource::ResourceLaneAdmission::AllowParallel => "team.dispatch.ready",
        resource::ResourceLaneAdmission::SequentialFallback => "team.dispatch.fallback",
        resource::ResourceLaneAdmission::Blocked => "team.dispatch.blocked",
    }
}

pub(crate) fn dispatch_summary(
    admission: resource::ResourceLaneAdmission,
    blocked_by_ownership: bool,
    continuation: &ContinuationDecision,
) -> &'static str {
    if admission == resource::ResourceLaneAdmission::Blocked {
        return "team dispatch blocked";
    }
    if blocked_by_ownership {
        return "team dispatch ownership blocked";
    }
    if continuation.is_blocked() {
        return "team continuation blocked";
    }
    if continuation.status == "continue-with-remaining" {
        return "team continuation recorded";
    }
    match admission {
        resource::ResourceLaneAdmission::AllowParallel => "team dispatch ready",
        resource::ResourceLaneAdmission::SequentialFallback => "team dispatch sequential fallback",
        resource::ResourceLaneAdmission::Blocked => "team dispatch blocked",
    }
}

pub(crate) fn admission_event_type(
    admission: resource::ResourceLaneAdmission,
    blocked_by_policy: bool,
    blocked_by_ownership: bool,
) -> &'static str {
    if admission == resource::ResourceLaneAdmission::Blocked {
        return "team.admission.blocked";
    }
    if blocked_by_ownership {
        return "team.admission.ownership_blocked";
    }
    if blocked_by_policy {
        return "team.admission.policy_blocked";
    }
    match admission {
        resource::ResourceLaneAdmission::AllowParallel => "team.admission.admitted",
        resource::ResourceLaneAdmission::SequentialFallback => "team.admission.fallback",
        resource::ResourceLaneAdmission::Blocked => "team.admission.blocked",
    }
}

pub(crate) fn admission_summary(
    admission: resource::ResourceLaneAdmission,
    blocked_by_policy: bool,
    blocked_by_ownership: bool,
) -> &'static str {
    if admission == resource::ResourceLaneAdmission::Blocked {
        return "team admission blocked";
    }
    if blocked_by_ownership {
        return "team admission ownership blocked";
    }
    if blocked_by_policy {
        return "team admission policy blocked";
    }
    match admission {
        resource::ResourceLaneAdmission::AllowParallel => "team admission admitted",
        resource::ResourceLaneAdmission::SequentialFallback => "team admission sequential fallback",
        resource::ResourceLaneAdmission::Blocked => "team admission blocked",
    }
}
