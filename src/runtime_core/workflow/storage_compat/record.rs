use std::time::{SystemTime, UNIX_EPOCH};

use crate::foundation::serialization as strict_json;
use crate::ledger::RuntimeIdentity;
use sha2::{Digest, Sha256};

const WORKFLOW_SCHEMA_VERSION: u64 = 4;
const PREVIOUS_WORKFLOW_SCHEMA_VERSION: u64 = 3;
const LEGACY_WORKFLOW_SCHEMA_VERSION: u64 = 2;

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

pub(crate) fn payload(record: &WorkflowRecord) -> String {
    format!(
        "schema_version={WORKFLOW_SCHEMA_VERSION}\nworkflow_id={}\nrevision={}\nprevious_hash={}\nproject_id={}\nsession_id={}\nphase={}\nrequest_hash={}\nworkflow_kind={}\nactive_skill_id={}\nskill_invocation={}\nskill_state={}\nskill_completed_hooks={}\nskill_evidence={}\nskill_stop_criteria={}\naction_id={}\naction_kind={}\naction_status={}\nresult_summary={}\nsource_path={}\nsource_hash={}\nfind_text={}\nreplace_text={}\nproposal_id={}\nproposal_hash={}\napproval_credential_hash={}\nbefore_hash={}\nafter_hash={}\nverification_plan={}\napproval_state={}\nverification_credential_hash={}\nverification_approval_state={}\nevidence_id={}\nevidence_hash={}\nfailure_reason={}\n",
        record.workflow_id,
        record.revision,
        record.previous_hash,
        record.project_id,
        record.session_id,
        record.phase,
        record.request_hash,
        record.workflow_kind,
        record.active_skill_id,
        record.skill_invocation,
        record.skill_state,
        record.skill_completed_hooks,
        record.skill_evidence,
        record.skill_stop_criteria,
        record.action_id,
        record.action_kind,
        record.action_status,
        record.result_summary,
        record.source_path,
        record.source_hash,
        record.find_text,
        record.replace_text,
        record.proposal_id,
        record.proposal_hash,
        record.approval_credential_hash,
        record.before_hash,
        record.after_hash,
        record.verification_plan,
        record.approval_state,
        record.verification_credential_hash,
        record.verification_approval_state,
        record.evidence_id,
        record.evidence_hash,
        record.failure_reason
    )
}

pub(crate) fn payload_v2(record: &WorkflowRecord) -> String {
    format!(
        "schema_version={LEGACY_WORKFLOW_SCHEMA_VERSION}\nworkflow_id={}\nrevision={}\nprevious_hash={}\nproject_id={}\nsession_id={}\nphase={}\nrequest_hash={}\nworkflow_kind={}\naction_id={}\naction_kind={}\naction_status={}\nresult_summary={}\nsource_path={}\nsource_hash={}\nfind_text={}\nreplace_text={}\nproposal_id={}\nproposal_hash={}\napproval_credential_hash={}\nbefore_hash={}\nafter_hash={}\nverification_plan={}\napproval_state={}\nevidence_id={}\nevidence_hash={}\nfailure_reason={}\n",
        record.workflow_id,
        record.revision,
        record.previous_hash,
        record.project_id,
        record.session_id,
        record.phase,
        record.request_hash,
        record.workflow_kind,
        record.action_id,
        record.action_kind,
        record.action_status,
        record.result_summary,
        record.source_path,
        record.source_hash,
        record.find_text,
        record.replace_text,
        record.proposal_id,
        record.proposal_hash,
        record.approval_credential_hash,
        record.before_hash,
        record.after_hash,
        record.verification_plan,
        record.approval_state,
        record.evidence_id,
        record.evidence_hash,
        record.failure_reason
    )
}

pub(crate) fn payload_v3(record: &WorkflowRecord) -> String {
    format!(
        "schema_version={PREVIOUS_WORKFLOW_SCHEMA_VERSION}\nworkflow_id={}\nrevision={}\nprevious_hash={}\nproject_id={}\nsession_id={}\nphase={}\nrequest_hash={}\nworkflow_kind={}\naction_id={}\naction_kind={}\naction_status={}\nresult_summary={}\nsource_path={}\nsource_hash={}\nfind_text={}\nreplace_text={}\nproposal_id={}\nproposal_hash={}\napproval_credential_hash={}\nbefore_hash={}\nafter_hash={}\nverification_plan={}\napproval_state={}\nverification_credential_hash={}\nverification_approval_state={}\nevidence_id={}\nevidence_hash={}\nfailure_reason={}\n",
        record.workflow_id,
        record.revision,
        record.previous_hash,
        record.project_id,
        record.session_id,
        record.phase,
        record.request_hash,
        record.workflow_kind,
        record.action_id,
        record.action_kind,
        record.action_status,
        record.result_summary,
        record.source_path,
        record.source_hash,
        record.find_text,
        record.replace_text,
        record.proposal_id,
        record.proposal_hash,
        record.approval_credential_hash,
        record.before_hash,
        record.after_hash,
        record.verification_plan,
        record.approval_state,
        record.verification_credential_hash,
        record.verification_approval_state,
        record.evidence_id,
        record.evidence_hash,
        record.failure_reason
    )
}

pub(crate) fn render(record: &WorkflowRecord) -> String {
    format!(
        concat!(
            "{{\n",
            "  \"schema_version\": {},\n",
            "  \"artifact_version\": \"workflow-v4\",\n",
            "  \"workflow_id\": \"{}\",\n",
            "  \"revision\": {},\n",
            "  \"previous_hash\": \"{}\",\n",
            "  \"artifact_hash\": \"{}\",\n",
            "  \"project_id\": \"{}\",\n",
            "  \"session_id\": \"{}\",\n",
            "  \"phase\": \"{}\",\n",
            "  \"request_hash\": \"{}\",\n",
            "  \"workflow_kind\": \"{}\",\n",
            "  \"active_skill_id\": \"{}\",\n",
            "  \"skill_invocation\": \"{}\",\n",
            "  \"skill_state\": \"{}\",\n",
            "  \"skill_completed_hooks\": \"{}\",\n",
            "  \"skill_evidence\": \"{}\",\n",
            "  \"skill_stop_criteria\": \"{}\",\n",
            "  \"action_id\": \"{}\",\n",
            "  \"action_kind\": \"{}\",\n",
            "  \"action_status\": \"{}\",\n",
            "  \"result_summary\": \"{}\",\n",
            "  \"source_path\": \"{}\",\n",
            "  \"source_hash\": \"{}\",\n",
            "  \"find_text\": \"{}\",\n",
            "  \"replace_text\": \"{}\",\n",
            "  \"proposal_id\": \"{}\",\n",
            "  \"proposal_hash\": \"{}\",\n",
            "  \"approval_credential_hash\": \"{}\",\n",
            "  \"before_hash\": \"{}\",\n",
            "  \"after_hash\": \"{}\",\n",
            "  \"verification_plan\": \"{}\",\n",
            "  \"approval_state\": \"{}\",\n",
            "  \"verification_credential_hash\": \"{}\",\n",
            "  \"verification_approval_state\": \"{}\",\n",
            "  \"evidence_id\": \"{}\",\n",
            "  \"evidence_hash\": \"{}\",\n",
            "  \"failure_reason\": \"{}\"\n",
            "}}\n"
        ),
        WORKFLOW_SCHEMA_VERSION,
        strict_json::escape_string_content(&record.workflow_id),
        record.revision,
        strict_json::escape_string_content(&record.previous_hash),
        strict_json::escape_string_content(&record.artifact_hash),
        strict_json::escape_string_content(&record.project_id),
        strict_json::escape_string_content(&record.session_id),
        strict_json::escape_string_content(&record.phase),
        strict_json::escape_string_content(&record.request_hash),
        strict_json::escape_string_content(&record.workflow_kind),
        strict_json::escape_string_content(&record.active_skill_id),
        strict_json::escape_string_content(&record.skill_invocation),
        strict_json::escape_string_content(&record.skill_state),
        strict_json::escape_string_content(&record.skill_completed_hooks),
        strict_json::escape_string_content(&record.skill_evidence),
        strict_json::escape_string_content(&record.skill_stop_criteria),
        strict_json::escape_string_content(&record.action_id),
        strict_json::escape_string_content(&record.action_kind),
        strict_json::escape_string_content(&record.action_status),
        strict_json::escape_string_content(&record.result_summary),
        strict_json::escape_string_content(&record.source_path),
        strict_json::escape_string_content(&record.source_hash),
        strict_json::escape_string_content(&record.find_text),
        strict_json::escape_string_content(&record.replace_text),
        strict_json::escape_string_content(&record.proposal_id),
        strict_json::escape_string_content(&record.proposal_hash),
        strict_json::escape_string_content(&record.approval_credential_hash),
        strict_json::escape_string_content(&record.before_hash),
        strict_json::escape_string_content(&record.after_hash),
        strict_json::escape_string_content(&record.verification_plan),
        strict_json::escape_string_content(&record.approval_state),
        strict_json::escape_string_content(&record.verification_credential_hash),
        strict_json::escape_string_content(&record.verification_approval_state),
        strict_json::escape_string_content(&record.evidence_id),
        strict_json::escape_string_content(&record.evidence_hash),
        strict_json::escape_string_content(&record.failure_reason)
    )
}

#[cfg(test)]
pub(crate) fn render_v3(record: &WorkflowRecord) -> String {
    let rendered = render(record)
        .replacen(
            &format!("\"schema_version\": {WORKFLOW_SCHEMA_VERSION}"),
            &format!("\"schema_version\": {PREVIOUS_WORKFLOW_SCHEMA_VERSION}"),
            1,
        )
        .replacen("workflow-v4", "workflow-v3", 1);
    let mut lines = rendered
        .lines()
        .filter(|line| {
            !line.contains("\"active_skill_id\"")
                && !line.contains("\"skill_invocation\"")
                && !line.contains("\"skill_state\"")
                && !line.contains("\"skill_completed_hooks\"")
                && !line.contains("\"skill_evidence\"")
                && !line.contains("\"skill_stop_criteria\"")
        })
        .collect::<Vec<_>>()
        .join("\n");
    lines.push('\n');
    lines
}

#[cfg(test)]
pub(crate) fn render_v2(record: &WorkflowRecord) -> String {
    let rendered = render_v3(record)
        .replacen(
            &format!("\"schema_version\": {PREVIOUS_WORKFLOW_SCHEMA_VERSION}"),
            &format!("\"schema_version\": {LEGACY_WORKFLOW_SCHEMA_VERSION}"),
            1,
        )
        .replacen("workflow-v3", "workflow-v2", 1);
    let mut lines = rendered
        .lines()
        .filter(|line| {
            !line.contains("\"verification_credential_hash\"")
                && !line.contains("\"verification_approval_state\"")
        })
        .collect::<Vec<_>>()
        .join("\n");
    lines.push('\n');
    lines
}

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
