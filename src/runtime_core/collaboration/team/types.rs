//! Team gate and continuation value objects.

use crate::runtime_core::policy::decision::Decision;

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
