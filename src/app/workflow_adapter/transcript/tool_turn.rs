use std::fs;
use std::path::{Path, PathBuf};

use crate::adapters::filesystem::{layout as paths, lease};
use crate::app::workflow_adapter::ledger;
use crate::app::workflow_adapter::state;
use crate::context::SourcePointer;
use crate::foundation::error::AppError;
use crate::runtime_core::workflow::domain::transcript::ToolOutputView;
use crate::runtime_core::workflow::storage_compat::transcript::{
    self as transcript_codec, ToolOutputArtifactBinding, TranscriptRecord, TranscriptSourcePointer,
    MAX_TRANSCRIPT_CONTENT_BYTES, TRANSCRIPT_SCHEMA_V1, TRANSCRIPT_SCHEMA_V2,
};

use super::super::transition;
use super::storage::{
    load_record_path, load_tool_output_artifact, now_ms, parse_tool_output_artifact_body,
    parse_transcript_record_body, tool_output_artifact_relative_path, validate_id,
    validate_source_pointer, validate_tool_artifact_owner, validate_tool_binding_for_record,
    validate_tool_binding_shape_for_record, validated_tool_output_path,
};
use super::transcript_ledger_event;

pub(super) const MAX_SANITIZED_STREAM_BYTES: usize = 64 * 1024;
pub(super) const MAX_TOOL_ARTIFACT_BYTES: usize = 256 * 1024;
pub(super) const UNAVAILABLE_STREAM: &str = "<unavailable>";
pub(super) const TOOL_ARTIFACT_KEYS: &[&str] = &[
    "schema_version",
    "artifact_id",
    "project_id",
    "session_id",
    "workflow_id",
    "tool_id",
    "created_at_ms",
    "redaction_policy",
    "redaction_version",
    "stdout",
    "stderr",
    "stdout_original_bytes",
    "stderr_original_bytes",
    "stdout_retained_chars",
    "stderr_retained_chars",
    "stdout_truncated",
    "stderr_truncated",
    "stdout_redacted",
    "stderr_redacted",
    "content_hash",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SanitizedToolOutputArtifact {
    pub(super) artifact_id: String,
    pub(super) project_id: String,
    pub(super) session_id: String,
    pub(super) workflow_id: String,
    pub(super) tool_id: String,
    pub(super) created_at_ms: u128,
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) stdout_original_bytes: u64,
    pub(super) stderr_original_bytes: u64,
    pub(super) stdout_retained_chars: u64,
    pub(super) stderr_retained_chars: u64,
    pub(super) stdout_truncated: bool,
    pub(super) stderr_truncated: bool,
    pub(super) stdout_redacted: bool,
    pub(super) stderr_redacted: bool,
    pub(super) content_hash: String,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedTranscriptTurn {
    pub tool_artifact_id: String,
    pub tool_path: PathBuf,
    pub tool_stored_path: String,
    pub tool_bytes: String,
    pub transcript_path: PathBuf,
    pub transcript_stored_path: String,
    pub transcript_bytes: String,
    pub record: TranscriptRecord,
    pub event: crate::app::workflow_adapter::ledger::LedgerEvent,
}

pub(crate) fn prepare_no_stream_tool_turn(
    workflow: &state::WorkflowRecord,
    causal_id: &str,
    content: &str,
    source_pointers: &[SourcePointer],
) -> Result<PreparedTranscriptTurn, AppError> {
    validate_id("project id", &workflow.project_id)?;
    validate_id("workflow id", &workflow.workflow_id)?;
    validate_id("session id", &workflow.session_id)?;
    validate_id("causal id", causal_id)?;
    if content.trim().is_empty() || content.len() > MAX_TRANSCRIPT_CONTENT_BYTES {
        return Err(AppError::blocked(
            "prepared transcript content boundary 불일치",
        ));
    }
    let created_at_ms = now_ms();
    let tool_artifact_id = format!(
        "tool-output-{}",
        state::sha256_text(
            &[
                "rpotato.tool-output-artifact-id/v1",
                &workflow.project_id,
                &workflow.session_id,
                &workflow.workflow_id,
                causal_id,
            ]
            .join("\0")
        )
    );
    let stdout = sanitize_tool_stream(None)?;
    let stderr = sanitize_tool_stream(None)?;
    let mut artifact = SanitizedToolOutputArtifact {
        artifact_id: tool_artifact_id.clone(),
        project_id: workflow.project_id.clone(),
        session_id: workflow.session_id.clone(),
        workflow_id: workflow.workflow_id.clone(),
        tool_id: causal_id.to_string(),
        created_at_ms,
        stdout: stdout.text,
        stderr: stderr.text,
        stdout_original_bytes: stdout.original_bytes,
        stderr_original_bytes: stderr.original_bytes,
        stdout_retained_chars: stdout.retained_chars,
        stderr_retained_chars: stderr.retained_chars,
        stdout_truncated: stdout.truncated,
        stderr_truncated: stderr.truncated,
        stdout_redacted: stdout.redacted,
        stderr_redacted: stderr.redacted,
        content_hash: String::new(),
    };
    artifact.content_hash = state::sha256_text(&artifact.payload());
    let tool_bytes = artifact.to_json();
    if tool_bytes.len() > MAX_TOOL_ARTIFACT_BYTES {
        return Err(AppError::blocked(
            "prepared SanitizedToolOutputArtifact byte limit 초과",
        ));
    }
    let binding = artifact.binding();
    let pointers = source_pointers
        .iter()
        .map(|pointer| {
            let pointer = TranscriptSourcePointer {
                stable_ref: pointer.stable_ref.clone(),
                path: pointer.path.clone(),
                source_hash: pointer.fingerprint.clone(),
            };
            validate_source_pointer(&pointer)?;
            Ok(pointer)
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    let record_id = format!(
        "transcript-{}",
        &state::sha256_text(&format!(
            "{}\n{}\n{}\ntool\n{}",
            workflow.project_id, workflow.session_id, workflow.workflow_id, causal_id
        ))[..24]
    );
    let mut record = TranscriptRecord {
        schema_version: TRANSCRIPT_SCHEMA_V2,
        record_id: record_id.clone(),
        project_id: workflow.project_id.clone(),
        session_id: workflow.session_id.clone(),
        workflow_id: workflow.workflow_id.clone(),
        kind: "tool".to_string(),
        causal_id: causal_id.to_string(),
        content: content.to_string(),
        content_hash: state::sha256_text(content),
        source_pointers: pointers,
        recorded_at_ms: created_at_ms,
        tool_output_artifact: Some(binding.clone()),
        artifact_hash: String::new(),
    };
    validate_tool_binding_shape_for_record(&record)?;
    record.artifact_hash = state::sha256_text(&record.artifact_payload());
    let transcript_bytes = record.to_json();
    if transcript_bytes.len() > 128 * 1024 {
        return Err(AppError::blocked(
            "prepared TranscriptRecord v2 byte limit 초과",
        ));
    }
    let event = transcript_ledger_event(&record)?;
    Ok(PreparedTranscriptTurn {
        tool_path: paths::tool_output_file(
            &workflow.project_id,
            &workflow.session_id,
            &workflow.workflow_id,
            &tool_artifact_id,
        ),
        tool_stored_path: binding.path,
        tool_artifact_id,
        tool_bytes,
        transcript_path: paths::transcript_file(
            &workflow.project_id,
            &workflow.session_id,
            &record_id,
        ),
        transcript_stored_path: format!(
            "state/transcripts/{}/{}/{}.json",
            workflow.project_id, workflow.session_id, record_id
        ),
        transcript_bytes,
        record,
        event,
    })
}

pub(crate) fn install_prepared_no_stream_tool_turn(
    prepared: &PreparedTranscriptTurn,
) -> Result<(), AppError> {
    {
        let _tool_lock = lease::RecoverableLease::acquire(
            prepared.tool_path.with_extension("checkpoint.lock"),
            "tool-output artifact",
        )?;
        install_exact_artifact(&prepared.tool_path, &prepared.tool_bytes)?;
        let artifact = load_tool_output_artifact(&prepared.tool_path)?;
        if artifact.artifact_id != prepared.tool_artifact_id
            || artifact.to_json() != prepared.tool_bytes
        {
            return Err(AppError::blocked(
                "prepared tool-output installed bytes 불일치",
            ));
        }
    }
    {
        let _transcript_lock = lease::RecoverableLease::acquire(
            prepared.transcript_path.with_extension("checkpoint.lock"),
            "transcript checkpoint",
        )?;
        let installed_bytes = transcript_codec::install_record(
            &prepared.transcript_path,
            &prepared.record,
            crate::adapters::filesystem::atomic_write::atomic_replace_bytes,
        )?;
        let record = load_record_path(&prepared.transcript_path)?;
        if installed_bytes != prepared.transcript_bytes
            || record != prepared.record
            || record.to_json() != prepared.transcript_bytes
        {
            return Err(AppError::blocked(
                "prepared TranscriptRecord installed bytes 불일치",
            ));
        }
    }
    Ok(())
}

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

fn install_exact_artifact(path: &Path, bytes: &str) -> Result<(), AppError> {
    if path.exists() {
        let existing = fs::read_to_string(path)
            .map_err(|err| AppError::blocked(format!("prepared artifact reread 실패: {err}")))?;
        if existing == bytes {
            return Ok(());
        }
        return Err(AppError::blocked("prepared artifact immutable conflict"));
    }
    crate::adapters::filesystem::atomic_write::atomic_replace_bytes(path, bytes.as_bytes())
}

pub(crate) fn tool_output_view_from_canonical_record(
    record: &TranscriptRecord,
    artifact_id: &str,
) -> Result<ToolOutputView, AppError> {
    validate_id("tool artifact id", artifact_id)?;
    validate_tool_binding_for_record(record)?;
    let binding = record.tool_output_artifact.as_ref().ok_or_else(|| {
        AppError::blocked("tool-output view에 대응하는 TranscriptRecord v2 binding이 없습니다.")
    })?;
    if record.schema_version != TRANSCRIPT_SCHEMA_V2
        || record.kind != "tool"
        || binding.id != artifact_id
    {
        return Err(AppError::blocked(
            "tool-output view transcript/artifact id binding 불일치",
        ));
    }
    let path = validated_tool_output_path(
        &record.project_id,
        &record.session_id,
        &record.workflow_id,
        artifact_id,
        false,
    )?;
    let artifact = load_tool_output_artifact(&path)?;
    if artifact.content_hash != binding.hash
        || artifact.artifact_id != binding.id
        || artifact.project_id != record.project_id
        || artifact.session_id != record.session_id
        || artifact.workflow_id != record.workflow_id
        || artifact.tool_id != record.causal_id
    {
        return Err(AppError::blocked(
            "tool-output view canonical transcript/owner/hash binding 불일치",
        ));
    }
    Ok(tool_output_view(artifact))
}

fn tool_output_view(artifact: SanitizedToolOutputArtifact) -> ToolOutputView {
    ToolOutputView {
        artifact_id: artifact.artifact_id,
        session_id: artifact.session_id,
        workflow_id: artifact.workflow_id,
        tool_id: artifact.tool_id,
        created_at_ms: artifact.created_at_ms,
        stdout: artifact.stdout,
        stderr: artifact.stderr,
        stdout_truncated: artifact.stdout_truncated,
        stderr_truncated: artifact.stderr_truncated,
        stdout_redacted: artifact.stdout_redacted,
        stderr_redacted: artifact.stderr_redacted,
    }
}

impl SanitizedToolOutputArtifact {
    pub(super) fn payload(&self) -> String {
        format!(
            "{{\"schema_version\":1,\"artifact_id\":\"{}\",\"project_id\":\"{}\",\"session_id\":\"{}\",\"workflow_id\":\"{}\",\"tool_id\":\"{}\",\"created_at_ms\":{},\"redaction_policy\":\"credential-and-control-redaction\",\"redaction_version\":1,\"stdout\":\"{}\",\"stderr\":\"{}\",\"stdout_original_bytes\":{},\"stderr_original_bytes\":{},\"stdout_retained_chars\":{},\"stderr_retained_chars\":{},\"stdout_truncated\":{},\"stderr_truncated\":{},\"stdout_redacted\":{},\"stderr_redacted\":{}}}",
            ledger::json_string(&self.artifact_id),
            ledger::json_string(&self.project_id),
            ledger::json_string(&self.session_id),
            ledger::json_string(&self.workflow_id),
            ledger::json_string(&self.tool_id),
            self.created_at_ms,
            ledger::json_string(&self.stdout),
            ledger::json_string(&self.stderr),
            self.stdout_original_bytes,
            self.stderr_original_bytes,
            self.stdout_retained_chars,
            self.stderr_retained_chars,
            self.stdout_truncated,
            self.stderr_truncated,
            self.stdout_redacted,
            self.stderr_redacted,
        )
    }

    pub(super) fn to_json(&self) -> String {
        format!(
            "{},\"content_hash\":\"{}\"}}",
            self.payload().trim_end_matches('}'),
            self.content_hash
        )
    }

    pub(super) fn binding(&self) -> ToolOutputArtifactBinding {
        ToolOutputArtifactBinding {
            id: self.artifact_id.clone(),
            path: tool_output_artifact_relative_path(
                &self.project_id,
                &self.session_id,
                &self.workflow_id,
                &self.artifact_id,
            ),
            hash: self.content_hash.clone(),
        }
    }
}

pub(super) fn record_tool_output_artifact(
    workflow: &state::WorkflowRecord,
    tool_id: &str,
    stdout: Option<&str>,
    stderr: Option<&str>,
) -> Result<ToolOutputArtifactBinding, AppError> {
    validate_id("tool id", tool_id)?;
    let artifact_id = format!(
        "tool-output-{}",
        state::sha256_text(
            &[
                "rpotato.tool-output-artifact-id/v1",
                &workflow.project_id,
                &workflow.session_id,
                &workflow.workflow_id,
                tool_id,
            ]
            .join("\0")
        )
    );
    let path = validated_tool_output_path(
        &workflow.project_id,
        &workflow.session_id,
        &workflow.workflow_id,
        &artifact_id,
        true,
    )?;
    let _lease = lease::RecoverableLease::acquire(
        path.with_extension("checkpoint.lock"),
        "tool-output artifact",
    )?;
    let stdout = sanitize_tool_stream(stdout)?;
    let stderr = sanitize_tool_stream(stderr)?;
    if path.exists() {
        let existing = load_tool_output_artifact(&path)?;
        validate_tool_artifact_owner(&existing, workflow, tool_id, &artifact_id)?;
        validate_sanitized_streams(&existing, &stdout, &stderr)?;
        return Ok(existing.binding());
    }

    let mut artifact = SanitizedToolOutputArtifact {
        artifact_id,
        project_id: workflow.project_id.clone(),
        session_id: workflow.session_id.clone(),
        workflow_id: workflow.workflow_id.clone(),
        tool_id: tool_id.to_string(),
        created_at_ms: now_ms(),
        stdout: stdout.text,
        stderr: stderr.text,
        stdout_original_bytes: stdout.original_bytes,
        stderr_original_bytes: stderr.original_bytes,
        stdout_retained_chars: stdout.retained_chars,
        stderr_retained_chars: stderr.retained_chars,
        stdout_truncated: stdout.truncated,
        stderr_truncated: stderr.truncated,
        stdout_redacted: stdout.redacted,
        stderr_redacted: stderr.redacted,
        content_hash: String::new(),
    };
    artifact.content_hash = state::sha256_text(&artifact.payload());
    let body = artifact.to_json();
    if body.len() > MAX_TOOL_ARTIFACT_BYTES {
        return Err(AppError::blocked(
            "SanitizedToolOutputArtifact canonical byte limit 초과",
        ));
    }
    crate::adapters::filesystem::atomic_write::atomic_replace_bytes(&path, body.as_bytes())?;
    Ok(artifact.binding())
}

pub(super) struct SanitizedStream {
    pub(super) text: String,
    pub(super) original_bytes: u64,
    pub(super) retained_chars: u64,
    pub(super) truncated: bool,
    pub(super) redacted: bool,
}

pub(super) fn sanitize_tool_stream(value: Option<&str>) -> Result<SanitizedStream, AppError> {
    let Some(value) = value else {
        return Ok(SanitizedStream {
            text: UNAVAILABLE_STREAM.to_string(),
            original_bytes: 0,
            retained_chars: u64::try_from(UNAVAILABLE_STREAM.chars().count())
                .map_err(|_| AppError::blocked("tool stream retained count overflow"))?,
            truncated: false,
            redacted: false,
        });
    };
    let original_bytes = u64::try_from(value.len())
        .map_err(|_| AppError::blocked("tool stream original byte count overflow"))?;
    let without_controls = value
        .chars()
        .map(|ch| {
            if ch == '\n' || ch == '\t' || !ch.is_control() && ch != '\u{001b}' {
                ch
            } else {
                '�'
            }
        })
        .collect::<String>();
    let redacted_text = ledger::redact_text(&without_controls);
    let redacted = redacted_text != value;
    let mut text = String::new();
    for ch in redacted_text.chars() {
        if text.len().saturating_add(ch.len_utf8()) > MAX_SANITIZED_STREAM_BYTES {
            break;
        }
        text.push(ch);
    }
    let truncated = text.len() < redacted_text.len();
    let retained_chars = u64::try_from(text.chars().count())
        .map_err(|_| AppError::blocked("tool stream retained count overflow"))?;
    Ok(SanitizedStream {
        text,
        original_bytes,
        retained_chars,
        truncated,
        redacted,
    })
}

pub(super) fn validate_requested_tool_streams(
    record: &TranscriptRecord,
    stdout: Option<&str>,
    stderr: Option<&str>,
) -> Result<(), AppError> {
    if record.kind != "tool" {
        if stdout.is_some() || stderr.is_some() {
            return Err(AppError::blocked(
                "non-tool transcript에는 tool stream을 바인딩할 수 없습니다.",
            ));
        }
        return Ok(());
    }
    if record.schema_version == TRANSCRIPT_SCHEMA_V1 {
        if stdout.is_some() || stderr.is_some() {
            return Err(AppError::blocked(
                "legacy TranscriptRecord v1 tool output은 unavailable입니다.",
            ));
        }
        return Ok(());
    }
    let binding = record
        .tool_output_artifact
        .as_ref()
        .ok_or_else(|| AppError::blocked("TranscriptRecord v2 tool binding 누락"))?;
    let path = validated_tool_output_path(
        &record.project_id,
        &record.session_id,
        &record.workflow_id,
        &binding.id,
        false,
    )?;
    let artifact = load_tool_output_artifact(&path)?;
    let expected_stdout = sanitize_tool_stream(stdout)?;
    let expected_stderr = sanitize_tool_stream(stderr)?;
    validate_sanitized_streams(&artifact, &expected_stdout, &expected_stderr)
}

fn validate_sanitized_streams(
    artifact: &SanitizedToolOutputArtifact,
    stdout: &SanitizedStream,
    stderr: &SanitizedStream,
) -> Result<(), AppError> {
    if artifact.stdout != stdout.text
        || artifact.stderr != stderr.text
        || artifact.stdout_original_bytes != stdout.original_bytes
        || artifact.stderr_original_bytes != stderr.original_bytes
        || artifact.stdout_retained_chars != stdout.retained_chars
        || artifact.stderr_retained_chars != stderr.retained_chars
        || artifact.stdout_truncated != stdout.truncated
        || artifact.stderr_truncated != stderr.truncated
        || artifact.stdout_redacted != stdout.redacted
        || artifact.stderr_redacted != stderr.redacted
    {
        return Err(AppError::blocked(
            "SanitizedToolOutputArtifact deterministic stream 충돌",
        ));
    }
    Ok(())
}
