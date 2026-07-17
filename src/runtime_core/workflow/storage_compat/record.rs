use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use super::ledger::RuntimeIdentity;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowRecord {
    pub workflow_id: String,
    pub revision: u64,
    pub previous_hash: String,
    pub artifact_hash: String,
    pub project_id: String,
    pub session_id: String,
    pub phase: String,
    pub request_hash: String,
    pub workflow_kind: String,
    pub active_skill_id: String,
    pub skill_invocation: String,
    pub skill_state: String,
    pub skill_completed_hooks: String,
    pub skill_evidence: String,
    pub skill_stop_criteria: String,
    pub action_id: String,
    pub action_kind: String,
    pub action_status: String,
    pub result_summary: String,
    pub source_path: String,
    pub source_hash: String,
    pub find_text: String,
    pub replace_text: String,
    pub proposal_id: String,
    pub proposal_hash: String,
    pub approval_credential_hash: String,
    pub before_hash: String,
    pub after_hash: String,
    pub verification_plan: String,
    pub approval_state: String,
    pub verification_credential_hash: String,
    pub verification_approval_state: String,
    pub evidence_id: String,
    pub evidence_hash: String,
    pub failure_reason: String,
}

impl WorkflowRecord {
    pub fn new(identity: &RuntimeIdentity, request: &str) -> Self {
        let nonce = format!("{}\n{}\n{}", identity.session_id, request, now_ms());
        let workflow_id = format!("workflow-{}", &sha256_text(&nonce)[..20]);
        Self {
            action_id: format!(
                "action-{}",
                &sha256_text(&format!("{workflow_id}\naction"))[..20]
            ),
            workflow_id,
            revision: 0,
            previous_hash: "none".to_string(),
            artifact_hash: String::new(),
            project_id: identity.project_id.clone(),
            session_id: identity.session_id.clone(),
            phase: "model-pending".to_string(),
            request_hash: sha256_text(request),
            workflow_kind: "agent-run".to_string(),
            active_skill_id: String::new(),
            skill_invocation: String::new(),
            skill_state: String::new(),
            skill_completed_hooks: String::new(),
            skill_evidence: String::new(),
            skill_stop_criteria: String::new(),
            action_kind: "unclassified".to_string(),
            action_status: "runtime-candidate".to_string(),
            result_summary: String::new(),
            source_path: String::new(),
            source_hash: String::new(),
            find_text: String::new(),
            replace_text: String::new(),
            proposal_id: String::new(),
            proposal_hash: String::new(),
            approval_credential_hash: String::new(),
            before_hash: String::new(),
            after_hash: String::new(),
            verification_plan: String::new(),
            approval_state: "not-requested".to_string(),
            verification_credential_hash: String::new(),
            verification_approval_state: "not-issued".to_string(),
            evidence_id: String::new(),
            evidence_hash: String::new(),
            failure_reason: String::new(),
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self.phase.as_str(), "complete" | "failed" | "cancelled")
    }
}

#[path = "record/codec.rs"]
mod codec;

#[allow(unused_imports)]
pub(crate) use codec::{
    parse_pointer, parse_snapshot, payload, render, render_pointer, snapshot_schema,
    WorkflowPointer,
};
#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use codec::{payload_v2, payload_v3, render_v2, render_v3};

fn sha256_text(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}
