use super::*;

impl SanitizedToolOutputArtifact {
    pub(in super::super) fn payload(&self) -> String {
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

    pub(in super::super) fn to_json(&self) -> String {
        format!(
            "{},\"content_hash\":\"{}\"}}",
            self.payload().trim_end_matches('}'),
            self.content_hash
        )
    }

    pub(in super::super) fn binding(&self) -> ToolOutputArtifactBinding {
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

pub(in super::super) fn record_tool_output_artifact(
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

pub(in super::super) struct SanitizedStream {
    pub(in super::super) text: String,
    pub(in super::super) original_bytes: u64,
    pub(in super::super) retained_chars: u64,
    pub(in super::super) truncated: bool,
    pub(in super::super) redacted: bool,
}

pub(in super::super) fn sanitize_tool_stream(
    value: Option<&str>,
) -> Result<SanitizedStream, AppError> {
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

pub(in super::super) fn validate_requested_tool_streams(
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
