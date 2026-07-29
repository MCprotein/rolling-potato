use crate::foundation::serialization as strict_json;

use super::super::WorkflowRecord;
use super::versions::WORKFLOW_SCHEMA_VERSION;
#[cfg(test)]
use super::versions::{LEGACY_WORKFLOW_SCHEMA_VERSION, PREVIOUS_WORKFLOW_SCHEMA_VERSION};

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
