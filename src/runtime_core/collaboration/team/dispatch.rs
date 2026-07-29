//! Failed-lane continuation and team dispatch decisions.

use crate::runtime_core::inference::resource;

use super::admission::admission_status;
use super::ContinuationDecision;

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
