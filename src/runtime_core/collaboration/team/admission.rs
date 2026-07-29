//! Team admission status and event labels.

use crate::runtime_core::inference::resource;

pub(super) fn admission_status(admission: resource::ResourceLaneAdmission) -> &'static str {
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
