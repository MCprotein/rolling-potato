//! Surface-neutral evidence validation and stop-gate inputs.

use std::path::{Component, Path, PathBuf};

use crate::foundation::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationEvidence {
    pub evidence_id: String,
    pub artifact_hash: String,
    pub passed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceStoreStatus {
    pub runtime_evidence_file: PathBuf,
    pub runtime_evidence_records: usize,
    pub project_evidence_dir: PathBuf,
    pub project_artifacts: usize,
    pub stale_policy: &'static str,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceValidation {
    pub artifact: PathBuf,
    pub project_root: PathBuf,
    pub stale_policy: &'static str,
}

pub(crate) struct StopGateInputs<'a> {
    pub phase: &'a str,
    pub approval_state: &'a str,
    pub verification_approval_state: &'a str,
    pub expected_workflow_id: &'a str,
    pub expected_proposal_id: &'a str,
    pub expected_action_id: &'a str,
    pub expected_evidence_id: &'a str,
    pub expected_evidence_hash: &'a str,
    pub expected_command_hash: &'a str,
    pub expected_source_hash: &'a str,
    pub evidence_workflow_id: &'a str,
    pub evidence_proposal_id: &'a str,
    pub evidence_action_id: &'a str,
    pub evidence_id: &'a str,
    pub body_artifact_hash: &'a str,
    pub recomputed_artifact_hash: &'a str,
    pub command_hash: &'a str,
    pub source_hash: &'a str,
    pub authoritative_source_hash: &'a str,
    pub passed: bool,
}

pub(crate) fn validate_stop_inputs(inputs: &StopGateInputs<'_>) -> bool {
    matches!(inputs.phase, "verified" | "complete")
        && inputs.approval_state == "applied"
        && inputs.verification_approval_state == "approved"
        && !inputs.expected_proposal_id.is_empty()
        && !inputs.expected_evidence_id.is_empty()
        && inputs.recomputed_artifact_hash == inputs.expected_evidence_hash
        && inputs.body_artifact_hash == inputs.recomputed_artifact_hash
        && inputs.evidence_id == inputs.expected_evidence_id
        && inputs.evidence_workflow_id == inputs.expected_workflow_id
        && inputs.evidence_proposal_id == inputs.expected_proposal_id
        && inputs.evidence_action_id == inputs.expected_action_id
        && inputs.command_hash == inputs.expected_command_hash
        && inputs.source_hash == inputs.expected_source_hash
        && inputs.authoritative_source_hash == inputs.expected_source_hash
        && inputs.passed
}

pub(crate) fn validate_artifact_pointer_syntax(pointer: &str) -> Result<(), AppError> {
    if pointer.trim().is_empty() {
        return Err(AppError::usage("evidence artifact pointer가 필요합니다."));
    }
    if pointer.contains("://") {
        return Err(AppError::blocked(
            "evidence artifact pointer는 local project path만 허용합니다.",
        ));
    }

    let pointer_path = Path::new(pointer);
    if pointer_path.is_absolute() {
        return Err(AppError::blocked(
            "evidence artifact pointer는 project-relative path만 허용합니다.",
        ));
    }
    if pointer_path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(AppError::blocked(
            "evidence artifact pointer는 상위 경로(..)를 포함할 수 없습니다.",
        ));
    }
    Ok(())
}

pub fn stale_policy_summary() -> &'static str {
    "artifact 누락, project boundary 이탈, stale_after_ms 만료 시 stale"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_inputs() -> StopGateInputs<'static> {
        StopGateInputs {
            phase: "verified",
            approval_state: "applied",
            verification_approval_state: "approved",
            expected_workflow_id: "workflow",
            expected_proposal_id: "proposal",
            expected_action_id: "action",
            expected_evidence_id: "evidence",
            expected_evidence_hash: "artifact-hash",
            expected_command_hash: "command-hash",
            expected_source_hash: "source-hash",
            evidence_workflow_id: "workflow",
            evidence_proposal_id: "proposal",
            evidence_action_id: "action",
            evidence_id: "evidence",
            body_artifact_hash: "artifact-hash",
            recomputed_artifact_hash: "artifact-hash",
            command_hash: "command-hash",
            source_hash: "source-hash",
            authoritative_source_hash: "source-hash",
            passed: true,
        }
    }

    #[test]
    fn stop_gate_requires_every_binding_and_fresh_source() {
        assert!(validate_stop_inputs(&valid_inputs()));

        let mut stale_source = valid_inputs();
        stale_source.authoritative_source_hash = "changed";
        assert!(!validate_stop_inputs(&stale_source));

        let mut wrong_action = valid_inputs();
        wrong_action.evidence_action_id = "other-action";
        assert!(!validate_stop_inputs(&wrong_action));

        let mut failed = valid_inputs();
        failed.passed = false;
        assert!(!validate_stop_inputs(&failed));
    }

    #[test]
    fn artifact_pointer_syntax_is_fail_closed() {
        assert!(validate_artifact_pointer_syntax(".rpotato/evidence/one.json").is_ok());
        for pointer in [
            "",
            "https://example.com/evidence.json",
            "/tmp/evidence",
            "../evidence",
        ] {
            assert!(validate_artifact_pointer_syntax(pointer).is_err());
        }
    }
}
