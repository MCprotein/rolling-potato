use crate::adapters::filesystem::layout as paths;
use crate::foundation::error::AppError;
use crate::runtime_core::workflow::storage_compat::transcript::TRANSCRIPT_SCHEMA_V2;

use super::super::super::transition;
use super::super::storage::{
    parse_tool_output_artifact_body, parse_transcript_record_body,
    tool_output_artifact_relative_path,
};
use super::super::transcript_ledger_event;
use super::types::PreparedTranscriptTurn;

pub(crate) fn decode_prepared_no_stream_tool_turn(
    tool_member: &transition::PreparedMember,
    transcript_member: &transition::PreparedMember,
    event: &crate::app::workflow_adapter::ledger::LedgerEvent,
) -> Result<PreparedTranscriptTurn, AppError> {
    use transition::PreparedMemberKind;

    if tool_member.kind != PreparedMemberKind::ToolOutput
        || transcript_member.kind != PreparedMemberKind::TranscriptV2
        || tool_member.schema_version != 1
        || transcript_member.schema_version != TRANSCRIPT_SCHEMA_V2
        || tool_member.expected_type != "absent"
        || transcript_member.expected_type != "absent"
    {
        return Err(AppError::blocked(
            "prepared transcript member kind/schema/type 불일치",
        ));
    }
    let artifact = parse_tool_output_artifact_body(&tool_member.bytes_utf8)?;
    let record = parse_transcript_record_body(&transcript_member.bytes_utf8)?;
    let binding = artifact.binding();
    let expected_event = transcript_ledger_event(&record)?;
    let tool_stored_path = tool_output_artifact_relative_path(
        &artifact.project_id,
        &artifact.session_id,
        &artifact.workflow_id,
        &artifact.artifact_id,
    );
    let transcript_stored_path = format!(
        "state/transcripts/{}/{}/{}.json",
        record.project_id, record.session_id, record.record_id
    );
    if artifact.stdout_original_bytes != 0
        || artifact.stderr_original_bytes != 0
        || artifact.stdout != "<unavailable>"
        || artifact.stderr != "<unavailable>"
        || record.schema_version != TRANSCRIPT_SCHEMA_V2
        || record.kind != "tool"
        || record.project_id != artifact.project_id
        || record.session_id != artifact.session_id
        || record.workflow_id != artifact.workflow_id
        || record.causal_id != artifact.tool_id
        || record.tool_output_artifact.as_ref() != Some(&binding)
        || tool_member.path != tool_stored_path
        || transcript_member.path != transcript_stored_path
        || tool_member.binding.artifact_id.as_deref() != Some(artifact.artifact_id.as_str())
        || tool_member.binding.causal_id.as_deref() != Some(record.causal_id.as_str())
        || tool_member.binding.event_id.as_deref() != Some(record.causal_id.as_str())
        || transcript_member.binding.artifact_id.as_deref() != Some(record.record_id.as_str())
        || transcript_member.binding.causal_id.as_deref() != Some(artifact.artifact_id.as_str())
        || transcript_member.binding.event_id.as_deref() != Some(event.event_id.as_str())
        || artifact.to_json() != tool_member.bytes_utf8
        || record.to_json() != transcript_member.bytes_utf8
        || expected_event != *event
    {
        return Err(AppError::blocked(
            "prepared no-stream tool/transcript/event binding 불일치",
        ));
    }
    Ok(PreparedTranscriptTurn {
        tool_artifact_id: artifact.artifact_id.clone(),
        tool_path: paths::tool_output_file(
            &artifact.project_id,
            &artifact.session_id,
            &artifact.workflow_id,
            &artifact.artifact_id,
        ),
        tool_stored_path,
        tool_bytes: tool_member.bytes_utf8.clone(),
        transcript_path: paths::transcript_file(
            &record.project_id,
            &record.session_id,
            &record.record_id,
        ),
        transcript_stored_path,
        transcript_bytes: transcript_member.bytes_utf8.clone(),
        record,
        event: event.clone(),
    })
}
