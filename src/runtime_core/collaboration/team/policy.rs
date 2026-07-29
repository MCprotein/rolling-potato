//! Team command and write-path policy gate.

use crate::runtime_core::policy::decision::Decision;

use super::{PolicyCheck, PolicyGate};

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

pub(crate) fn decision_label(decision: Decision) -> &'static str {
    match decision {
        Decision::Allow => "allow",
        Decision::Ask => "ask",
        Decision::Deny => "deny",
    }
}
