use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::app::AppError;
use crate::context::SourcePointer;
use crate::ledger::{self, ParsedLedgerEvent, RuntimeIdentity};
use crate::{observability, paths, state};

const TRANSCRIPT_SCHEMA_V1: u64 = 1;
const TRANSCRIPT_SCHEMA_V2: u64 = 2;
const MAX_TRANSCRIPT_CONTENT_BYTES: usize = 64 * 1024;
const MAX_SANITIZED_STREAM_BYTES: usize = 64 * 1024;
const MAX_TOOL_ARTIFACT_BYTES: usize = 256 * 1024;
const UNAVAILABLE_STREAM: &str = "<unavailable>";
const TRANSCRIPT_V1_KEYS: &[&str] = &[
    "schema_version",
    "record_id",
    "project_id",
    "session_id",
    "workflow_id",
    "kind",
    "causal_id",
    "content",
    "content_hash",
    "source_pointers",
    "recorded_at_ms",
    "artifact_hash",
];
const TRANSCRIPT_V2_KEYS: &[&str] = &[
    "schema_version",
    "record_id",
    "project_id",
    "session_id",
    "workflow_id",
    "kind",
    "causal_id",
    "content",
    "content_hash",
    "source_pointers",
    "recorded_at_ms",
    "tool_output_artifact",
    "artifact_hash",
];
const TOOL_BINDING_KEYS: &[&str] = &["id", "path", "hash"];
const TOOL_ARTIFACT_KEYS: &[&str] = &[
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
pub struct TranscriptSourcePointer {
    pub stable_ref: String,
    pub path: String,
    pub source_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutputArtifactBinding {
    pub id: String,
    pub path: String,
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutputView {
    pub artifact_id: String,
    pub session_id: String,
    pub workflow_id: String,
    pub tool_id: String,
    pub created_at_ms: u128,
    pub stdout: String,
    pub stderr: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub stdout_redacted: bool,
    pub stderr_redacted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SanitizedToolOutputArtifact {
    artifact_id: String,
    project_id: String,
    session_id: String,
    workflow_id: String,
    tool_id: String,
    created_at_ms: u128,
    stdout: String,
    stderr: String,
    stdout_original_bytes: u64,
    stderr_original_bytes: u64,
    stdout_retained_chars: u64,
    stderr_retained_chars: u64,
    stdout_truncated: bool,
    stderr_truncated: bool,
    stdout_redacted: bool,
    stderr_redacted: bool,
    content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptRecord {
    pub schema_version: u64,
    pub record_id: String,
    pub project_id: String,
    pub session_id: String,
    pub workflow_id: String,
    pub kind: String,
    pub causal_id: String,
    pub content: String,
    pub content_hash: String,
    pub source_pointers: Vec<TranscriptSourcePointer>,
    pub recorded_at_ms: u128,
    pub tool_output_artifact: Option<ToolOutputArtifactBinding>,
    pub artifact_hash: String,
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
    pub event: crate::ledger::LedgerEvent,
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
        let _tool_lock = crate::lease::RecoverableLease::acquire(
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
        let _transcript_lock = crate::lease::RecoverableLease::acquire(
            prepared.transcript_path.with_extension("checkpoint.lock"),
            "transcript checkpoint",
        )?;
        install_exact_artifact(&prepared.transcript_path, &prepared.transcript_bytes)?;
        let record = load_record_path(&prepared.transcript_path)?;
        if record != prepared.record || record.to_json() != prepared.transcript_bytes {
            return Err(AppError::blocked(
                "prepared TranscriptRecord installed bytes 불일치",
            ));
        }
    }
    Ok(())
}

pub(crate) fn decode_prepared_no_stream_tool_turn(
    tool_member: &crate::transition::PreparedMember,
    transcript_member: &crate::transition::PreparedMember,
    event: &crate::ledger::LedgerEvent,
) -> Result<PreparedTranscriptTurn, AppError> {
    use crate::transition::PreparedMemberKind;

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
    state::atomic_replace_bytes(path, bytes.as_bytes())
}

pub fn record_workflow_turn(
    workflow: &state::WorkflowRecord,
    kind: &str,
    causal_id: &str,
    content: &str,
    source_pointers: &[SourcePointer],
) -> Result<TranscriptRecord, AppError> {
    record_workflow_turn_with_streams(
        workflow,
        kind,
        causal_id,
        content,
        source_pointers,
        None,
        None,
    )
}

pub fn record_workflow_turn_with_streams(
    workflow: &state::WorkflowRecord,
    kind: &str,
    causal_id: &str,
    content: &str,
    source_pointers: &[SourcePointer],
    stdout: Option<&str>,
    stderr: Option<&str>,
) -> Result<TranscriptRecord, AppError> {
    validate_kind(kind)?;
    validate_id("project id", &workflow.project_id)?;
    validate_id("workflow id", &workflow.workflow_id)?;
    validate_id("session id", &workflow.session_id)?;
    validate_id("causal id", causal_id)?;
    if content.trim().is_empty() {
        return Err(AppError::blocked("transcript content가 비어 있습니다."));
    }
    if content.len() > MAX_TRANSCRIPT_CONTENT_BYTES {
        return Err(AppError::blocked(format!(
            "transcript content 저장 차단\n- 최대 UTF-8 byte 수: {MAX_TRANSCRIPT_CONTENT_BYTES}"
        )));
    }

    let record_id = format!(
        "transcript-{}",
        &state::sha256_text(&format!(
            "{}\n{}\n{}\n{}\n{}",
            workflow.project_id, workflow.session_id, workflow.workflow_id, kind, causal_id
        ))[..24]
    );
    let ledger_guard = crate::ledger::LedgerWriterGuard::acquire()?;
    let path =
        validated_transcript_path(&workflow.project_id, &workflow.session_id, &record_id, true)?;
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

    if path.exists() {
        let existing = {
            let _lease = crate::lease::RecoverableLease::acquire(
                path.with_extension("checkpoint.lock"),
                "transcript checkpoint",
            )?;
            load_record_path(&path)?
        };
        validate_expected_record(&existing, workflow, kind, causal_id, content, &pointers)?;
        validate_requested_tool_streams(&existing, stdout, stderr)?;
        ensure_ledger_event_under_guard(&existing, &ledger_guard)?;
        return Ok(existing);
    }

    let tool_output_artifact = if kind == "tool" {
        Some(record_tool_output_artifact(
            workflow, causal_id, stdout, stderr,
        )?)
    } else {
        if stdout.is_some() || stderr.is_some() {
            return Err(AppError::blocked(
                "non-tool transcript에는 tool stream을 바인딩할 수 없습니다.",
            ));
        }
        None
    };

    let record = {
        let _lease = crate::lease::RecoverableLease::acquire(
            path.with_extension("checkpoint.lock"),
            "transcript checkpoint",
        )?;
        if path.exists() {
            let existing = load_record_path(&path)?;
            validate_expected_record(&existing, workflow, kind, causal_id, content, &pointers)?;
            validate_requested_tool_streams(&existing, stdout, stderr)?;
            existing
        } else {
            let mut record = TranscriptRecord {
                schema_version: TRANSCRIPT_SCHEMA_V2,
                record_id,
                project_id: workflow.project_id.clone(),
                session_id: workflow.session_id.clone(),
                workflow_id: workflow.workflow_id.clone(),
                kind: kind.to_string(),
                causal_id: causal_id.to_string(),
                content: content.to_string(),
                content_hash: state::sha256_text(content),
                source_pointers: pointers,
                recorded_at_ms: now_ms(),
                tool_output_artifact,
                artifact_hash: String::new(),
            };
            validate_tool_binding_for_record(&record)?;
            record.artifact_hash = state::sha256_text(&record.artifact_payload());
            let bytes = record.to_json();
            if bytes.len() > 128 * 1024 {
                return Err(AppError::blocked(
                    "TranscriptRecord v2 canonical byte limit 초과",
                ));
            }
            state::atomic_replace_bytes(&path, bytes.as_bytes())?;
            record
        }
    };
    ensure_ledger_event_under_guard(&record, &ledger_guard)?;
    Ok(record)
}

pub fn records_for_session(session_id: &str) -> Result<Vec<TranscriptRecord>, AppError> {
    validate_id("session id", session_id)?;
    let identity = ledger::validated_current_identity()?;
    let mut records = Vec::new();
    let mut seen = BTreeSet::new();
    for event in ledger::read_runtime_events()? {
        if event.project_id != identity.project_id
            || event.session_id != session_id
            || event.event_type != "transcript.recorded"
        {
            continue;
        }
        let record = record_from_event(&event)?;
        if !seen.insert(record.record_id.clone()) {
            return Err(AppError::blocked(format!(
                "transcript replay 차단\n- 이유: duplicate canonical record event\n- record id: {}",
                record.record_id
            )));
        }
        records.push(record);
    }
    Ok(records)
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

pub fn record_from_event(event: &ParsedLedgerEvent) -> Result<TranscriptRecord, AppError> {
    let record = record_from_binding(
        &event.project_id,
        &event.session_id,
        &event.event_type,
        &event.details,
    )?;
    if record.schema_version == TRANSCRIPT_SCHEMA_V2 {
        let expected = transcript_ledger_event(&record)?;
        if event.event_id != expected.event_id
            || event.ts_ms != expected.ts_ms
            || event.summary != expected.summary
        {
            return Err(AppError::blocked(format!(
                "transcript event identity/timestamp 불일치\n- record id: {}",
                record.record_id
            )));
        }
    }
    Ok(record)
}

pub fn record_from_binding(
    project_id: &str,
    session_id: &str,
    event_type: &str,
    details: &str,
) -> Result<TranscriptRecord, AppError> {
    if event_type != "transcript.recorded" {
        return Err(AppError::blocked("transcript event type 불일치"));
    }
    validate_id("project id", project_id)?;
    validate_id("session id", session_id)?;
    let parsed_details = parse_event_details(details)?;
    let record_id = detail_from_pairs(&parsed_details, "record_id")
        .ok_or_else(|| AppError::blocked("transcript event field 누락: record_id"))?;
    validate_id("record id", record_id)?;
    let expected_pointer = format!(
        "state/transcripts/{}/{}/{}.json",
        project_id, session_id, record_id
    );
    if detail_from_pairs(&parsed_details, "artifact_pointer") != Some(expected_pointer.as_str()) {
        return Err(AppError::blocked(format!(
            "transcript event artifact pointer 불일치\n- record id: {record_id}"
        )));
    }
    let path = validated_transcript_path(project_id, session_id, record_id, false)?;
    let record = load_record_path(&path)?;
    validate_event_details_for_schema(details, record.schema_version)?;
    if record.record_id != record_id
        || record.project_id != project_id
        || record.session_id != session_id
        || detail_from_pairs(&parsed_details, "workflow_id") != Some(record.workflow_id.as_str())
        || detail_from_pairs(&parsed_details, "kind") != Some(record.kind.as_str())
        || detail_from_pairs(&parsed_details, "content_hash") != Some(record.content_hash.as_str())
        || detail_from_pairs(&parsed_details, "artifact_hash")
            != Some(record.artifact_hash.as_str())
    {
        return Err(AppError::blocked(format!(
            "transcript event binding 불일치\n- record id: {record_id}"
        )));
    }
    if record.schema_version == TRANSCRIPT_SCHEMA_V2 {
        let expected = record.tool_output_artifact.as_ref();
        for (key, actual) in [
            (
                "tool_output_artifact_id",
                detail_from_pairs(&parsed_details, "tool_output_artifact_id"),
            ),
            (
                "tool_output_artifact_path",
                detail_from_pairs(&parsed_details, "tool_output_artifact_path"),
            ),
            (
                "tool_output_artifact_hash",
                detail_from_pairs(&parsed_details, "tool_output_artifact_hash"),
            ),
        ] {
            let wanted = match (key, expected) {
                ("tool_output_artifact_id", Some(binding)) => binding.id.as_str(),
                ("tool_output_artifact_path", Some(binding)) => binding.path.as_str(),
                ("tool_output_artifact_hash", Some(binding)) => binding.hash.as_str(),
                (_, None) => "none",
                _ => unreachable!(),
            };
            if actual != Some(wanted) {
                return Err(AppError::blocked(format!(
                    "transcript event tool binding 불일치\n- record id: {record_id}"
                )));
            }
        }
    }
    Ok(record)
}

impl TranscriptRecord {
    pub fn source_pointers_json(&self) -> String {
        render_source_pointers(&self.source_pointers)
    }

    fn artifact_payload(&self) -> String {
        match self.schema_version {
            TRANSCRIPT_SCHEMA_V1 => format!(
                "{{\"schema_version\":1,\"record_id\":\"{}\",\"project_id\":\"{}\",\"session_id\":\"{}\",\"workflow_id\":\"{}\",\"kind\":\"{}\",\"causal_id\":\"{}\",\"content\":\"{}\",\"content_hash\":\"{}\",\"source_pointers\":{},\"recorded_at_ms\":{}}}",
                ledger::json_string(&self.record_id),
                ledger::json_string(&self.project_id),
                ledger::json_string(&self.session_id),
                ledger::json_string(&self.workflow_id),
                ledger::json_string(&self.kind),
                ledger::json_string(&self.causal_id),
                ledger::json_string(&self.content),
                self.content_hash,
                render_source_pointers(&self.source_pointers),
                self.recorded_at_ms
            ),
            TRANSCRIPT_SCHEMA_V2 => format!(
                "{{\"schema_version\":2,\"record_id\":\"{}\",\"project_id\":\"{}\",\"session_id\":\"{}\",\"workflow_id\":\"{}\",\"kind\":\"{}\",\"causal_id\":\"{}\",\"content\":\"{}\",\"content_hash\":\"{}\",\"source_pointers\":{},\"recorded_at_ms\":{},\"tool_output_artifact\":{}}}",
                ledger::json_string(&self.record_id),
                ledger::json_string(&self.project_id),
                ledger::json_string(&self.session_id),
                ledger::json_string(&self.workflow_id),
                ledger::json_string(&self.kind),
                ledger::json_string(&self.causal_id),
                ledger::json_string(&self.content),
                self.content_hash,
                render_source_pointers(&self.source_pointers),
                self.recorded_at_ms,
                render_tool_binding(self.tool_output_artifact.as_ref())
            ),
            _ => String::new(),
        }
    }

    fn to_json(&self) -> String {
        match self.schema_version {
            TRANSCRIPT_SCHEMA_V1 => format!(
                "{{\n  \"schema_version\": 1,\n  \"record_id\": \"{}\",\n  \"project_id\": \"{}\",\n  \"session_id\": \"{}\",\n  \"workflow_id\": \"{}\",\n  \"kind\": \"{}\",\n  \"causal_id\": \"{}\",\n  \"content\": \"{}\",\n  \"content_hash\": \"{}\",\n  \"source_pointers\": {},\n  \"recorded_at_ms\": {},\n  \"artifact_hash\": \"{}\"\n}}\n",
                ledger::json_string(&self.record_id),
                ledger::json_string(&self.project_id),
                ledger::json_string(&self.session_id),
                ledger::json_string(&self.workflow_id),
                ledger::json_string(&self.kind),
                ledger::json_string(&self.causal_id),
                ledger::json_string(&self.content),
                self.content_hash,
                render_source_pointers(&self.source_pointers),
                self.recorded_at_ms,
                self.artifact_hash
            ),
            TRANSCRIPT_SCHEMA_V2 => format!(
                "{{\"schema_version\":2,\"record_id\":\"{}\",\"project_id\":\"{}\",\"session_id\":\"{}\",\"workflow_id\":\"{}\",\"kind\":\"{}\",\"causal_id\":\"{}\",\"content\":\"{}\",\"content_hash\":\"{}\",\"source_pointers\":{},\"recorded_at_ms\":{},\"tool_output_artifact\":{},\"artifact_hash\":\"{}\"}}",
                ledger::json_string(&self.record_id),
                ledger::json_string(&self.project_id),
                ledger::json_string(&self.session_id),
                ledger::json_string(&self.workflow_id),
                ledger::json_string(&self.kind),
                ledger::json_string(&self.causal_id),
                ledger::json_string(&self.content),
                self.content_hash,
                render_source_pointers(&self.source_pointers),
                self.recorded_at_ms,
                render_tool_binding(self.tool_output_artifact.as_ref()),
                self.artifact_hash
            ),
            _ => String::new(),
        }
    }
}

impl SanitizedToolOutputArtifact {
    fn payload(&self) -> String {
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

    fn to_json(&self) -> String {
        format!(
            "{},\"content_hash\":\"{}\"}}",
            self.payload().trim_end_matches('}'),
            self.content_hash
        )
    }

    fn binding(&self) -> ToolOutputArtifactBinding {
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

fn record_tool_output_artifact(
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
    let _lease = crate::lease::RecoverableLease::acquire(
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
    state::atomic_replace_bytes(&path, body.as_bytes())?;
    Ok(artifact.binding())
}

struct SanitizedStream {
    text: String,
    original_bytes: u64,
    retained_chars: u64,
    truncated: bool,
    redacted: bool,
}

fn sanitize_tool_stream(value: Option<&str>) -> Result<SanitizedStream, AppError> {
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

fn validate_requested_tool_streams(
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

fn ensure_ledger_event_under_guard(
    record: &TranscriptRecord,
    guard: &crate::ledger::LedgerWriterGuard,
) -> Result<(), AppError> {
    if record.schema_version == TRANSCRIPT_SCHEMA_V1 {
        let existing = guard
            .events()?
            .into_iter()
            .enumerate()
            .filter(|(_, candidate)| {
                candidate.event_type == "transcript.recorded"
                    && parse_event_details(&candidate.details)
                        .ok()
                        .and_then(|pairs| {
                            detail_from_pairs(&pairs, "record_id").map(str::to_string)
                        })
                        .as_deref()
                        == Some(record.record_id.as_str())
            })
            .collect::<Vec<_>>();
        if existing.len() > 1 {
            return Err(AppError::blocked(format!(
                "duplicate legacy transcript event 차단\n- record id: {}",
                record.record_id
            )));
        }
        if let Some((index, existing)) = existing.first() {
            let event = crate::ledger::LedgerEvent {
                event_id: existing.event_id.clone(),
                ts_ms: existing.ts_ms,
                event_type: existing.event_type.clone(),
                project_id: existing.project_id.clone(),
                session_id: existing.session_id.clone(),
                summary: existing.summary.clone(),
                details: existing.details.clone(),
            };
            return observability::project_event_with_ordinal(
                &event,
                u64::try_from(index + 1)
                    .map_err(|_| AppError::blocked("legacy transcript ordinal overflow"))?,
            );
        }
        let identity = RuntimeIdentity {
            project_id: record.project_id.clone(),
            session_id: record.session_id.clone(),
            project_root: paths::project_root().display().to_string(),
        };
        let artifact_pointer = format!(
            "state/transcripts/{}/{}/{}.json",
            record.project_id, record.session_id, record.record_id
        );
        let event = ledger::new_event_for(
            &identity,
            "transcript.recorded",
            &format!("{} transcript record persisted", record.kind),
            &format!(
                "record_id={} workflow_id={} kind={} artifact_pointer={} artifact_hash={} content_hash={}",
                record.record_id,
                record.workflow_id,
                record.kind,
                artifact_pointer,
                record.artifact_hash,
                record.content_hash
            ),
        );
        let appended = guard.append_planned(&event)?;
        return observability::project_event_with_ordinal(&event, appended.ordinal);
    }
    let event = transcript_ledger_event(record)?;
    let existing = guard
        .events()?
        .into_iter()
        .filter(|candidate| {
            candidate.event_type == "transcript.recorded"
                && parse_event_details(&candidate.details)
                    .ok()
                    .and_then(|details| {
                        detail_from_pairs(&details, "record_id").map(str::to_string)
                    })
                    .as_deref()
                    == Some(record.record_id.as_str())
        })
        .collect::<Vec<_>>();
    if existing.len() > 1 {
        return Err(AppError::blocked(format!(
            "duplicate transcript ledger event 차단\n- record id: {}",
            record.record_id
        )));
    }
    if let Some(existing) = existing.first() {
        if existing.event_id != event.event_id
            || existing.ts_ms != event.ts_ms
            || existing.project_id != event.project_id
            || existing.session_id != event.session_id
            || existing.summary != event.summary
            || existing.details != event.details
        {
            return Err(AppError::blocked(format!(
                "transcript ledger event immutable binding 불일치\n- record id: {}",
                record.record_id
            )));
        }
        let ordinal = u64::try_from(
            guard
                .events()?
                .iter()
                .position(|candidate| candidate.event_id == event.event_id)
                .ok_or_else(|| AppError::blocked("transcript event ordinal 누락"))?
                + 1,
        )
        .map_err(|_| AppError::blocked("transcript event ordinal overflow"))?;
        return observability::project_event_with_ordinal(&event, ordinal);
    }
    let appended = guard.append_planned(&event)?;
    observability::project_event_with_ordinal(&event, appended.ordinal)
}

fn transcript_ledger_event(
    record: &TranscriptRecord,
) -> Result<crate::ledger::LedgerEvent, AppError> {
    validate_tool_binding_shape_for_record(record)?;
    let identity = RuntimeIdentity {
        project_id: record.project_id.clone(),
        session_id: record.session_id.clone(),
        project_root: paths::project_root().display().to_string(),
    };
    let artifact_pointer = format!(
        "state/transcripts/{}/{}/{}.json",
        record.project_id, record.session_id, record.record_id
    );
    let details = match (record.schema_version, &record.tool_output_artifact) {
        (TRANSCRIPT_SCHEMA_V1, _) => format!(
            "record_id={} workflow_id={} kind={} artifact_pointer={} artifact_hash={} content_hash={}",
            record.record_id,
            record.workflow_id,
            record.kind,
            artifact_pointer,
            record.artifact_hash,
            record.content_hash
        ),
        (TRANSCRIPT_SCHEMA_V2, binding) => format!(
            "record_id={} workflow_id={} kind={} artifact_pointer={} artifact_hash={} content_hash={} tool_output_artifact_id={} tool_output_artifact_path={} tool_output_artifact_hash={}",
            record.record_id,
            record.workflow_id,
            record.kind,
            artifact_pointer,
            record.artifact_hash,
            record.content_hash,
            binding.as_ref().map(|value| value.id.as_str()).unwrap_or("none"),
            binding.as_ref().map(|value| value.path.as_str()).unwrap_or("none"),
            binding.as_ref().map(|value| value.hash.as_str()).unwrap_or("none")
        ),
        _ => return Err(AppError::blocked("transcript schema version 불일치")),
    };
    validate_event_details_for_schema(&details, record.schema_version)?;
    let pointer_hash = state::sha256_text(&record.source_pointers_json());
    let (tool_id, tool_path, tool_hash) = record
        .tool_output_artifact
        .as_ref()
        .map(|binding| {
            (
                binding.id.as_str(),
                binding.path.as_str(),
                binding.hash.as_str(),
            )
        })
        .unwrap_or(("", "", ""));
    let digest_input = [
        "rpotato.transcript-recorded-event-v1",
        &identity.project_id,
        &identity.session_id,
        &record.workflow_id,
        &record.record_id,
        &record.kind,
        &record.causal_id,
        &record.content_hash,
        &pointer_hash,
        tool_id,
        tool_path,
        tool_hash,
        &record.recorded_at_ms.to_string(),
        &record.artifact_hash,
    ]
    .join("\0");
    Ok(crate::ledger::LedgerEvent {
        event_id: format!("event-transcript-{}", state::sha256_text(&digest_input)),
        ts_ms: record.recorded_at_ms,
        event_type: "transcript.recorded".to_string(),
        project_id: identity.project_id,
        session_id: identity.session_id,
        summary: format!("{} transcript record persisted", record.kind),
        details,
    })
}

fn load_record_path(path: &std::path::Path) -> Result<TranscriptRecord, AppError> {
    let body = fs::read_to_string(path).map_err(|err| {
        AppError::blocked(format!(
            "transcript artifact 읽기 실패\n- path: {}\n- error: {err}",
            path.display()
        ))
    })?;
    let record = parse_transcript_record_body(&body)?;
    validate_tool_binding_for_record(&record)?;
    Ok(record)
}

fn parse_transcript_record_body(body: &str) -> Result<TranscriptRecord, AppError> {
    let version_probe =
        crate::strict_json::parse_object(body, TRANSCRIPT_V2_KEYS, "transcript artifact version")?;
    let schema_version = crate::strict_json::number(
        &version_probe,
        "schema_version",
        "transcript artifact version",
    )?;
    let mut record = match schema_version {
        TRANSCRIPT_SCHEMA_V1 => parse_transcript_v1(body)?,
        TRANSCRIPT_SCHEMA_V2 => parse_transcript_v2(body)?,
        _ => return Err(AppError::blocked("transcript schema version 불일치")),
    };
    validate_kind(&record.kind)?;
    validate_id("project id", &record.project_id)?;
    validate_id("record id", &record.record_id)?;
    validate_id("workflow id", &record.workflow_id)?;
    validate_id("session id", &record.session_id)?;
    validate_id("causal id", &record.causal_id)?;
    if record.content.trim().is_empty() || record.content.len() > MAX_TRANSCRIPT_CONTENT_BYTES {
        return Err(AppError::blocked(format!(
            "transcript content boundary 불일치\n- record id: {}",
            record.record_id
        )));
    }
    for pointer in &record.source_pointers {
        validate_source_pointer(pointer)?;
    }
    validate_tool_binding_shape_for_record(&record)?;
    if record.content_hash != state::sha256_text(&record.content) {
        return Err(AppError::blocked(format!(
            "transcript content hash 불일치\n- record id: {}",
            record.record_id
        )));
    }
    let expected_artifact_hash = state::sha256_text(&record.artifact_payload());
    if record.artifact_hash != expected_artifact_hash {
        return Err(AppError::blocked(format!(
            "transcript artifact hash 불일치\n- record id: {}",
            record.record_id
        )));
    }
    record.artifact_hash = expected_artifact_hash;
    Ok(record)
}

fn parse_transcript_v1(body: &str) -> Result<TranscriptRecord, AppError> {
    let object = crate::strict_json::parse_object(body, TRANSCRIPT_V1_KEYS, "transcript v1")?;
    if crate::strict_json::number(&object, "schema_version", "transcript v1")?
        != TRANSCRIPT_SCHEMA_V1
    {
        return Err(AppError::blocked("transcript v1 schema 불일치"));
    }
    Ok(TranscriptRecord {
        schema_version: TRANSCRIPT_SCHEMA_V1,
        record_id: crate::strict_json::string(&object, "record_id", "transcript v1")?,
        project_id: crate::strict_json::string(&object, "project_id", "transcript v1")?,
        session_id: crate::strict_json::string(&object, "session_id", "transcript v1")?,
        workflow_id: crate::strict_json::string(&object, "workflow_id", "transcript v1")?,
        kind: crate::strict_json::string(&object, "kind", "transcript v1")?,
        causal_id: crate::strict_json::string(&object, "causal_id", "transcript v1")?,
        content: crate::strict_json::string(&object, "content", "transcript v1")?,
        content_hash: crate::strict_json::string(&object, "content_hash", "transcript v1")?,
        source_pointers: parse_source_pointers(object.get("source_pointers"))?,
        recorded_at_ms: crate::strict_json::number_u128(
            &object,
            "recorded_at_ms",
            "transcript v1",
        )?,
        tool_output_artifact: None,
        artifact_hash: crate::strict_json::string(&object, "artifact_hash", "transcript v1")?,
    })
}

fn parse_transcript_v2(body: &str) -> Result<TranscriptRecord, AppError> {
    use crate::strict_json::CanonicalValue;

    let object = crate::strict_json::parse_canonical_object(
        body,
        TRANSCRIPT_V2_KEYS,
        "TranscriptRecord v2",
    )?;
    if crate::strict_json::canonical_u64(&object, "schema_version", "TranscriptRecord v2")?
        != TRANSCRIPT_SCHEMA_V2
    {
        return Err(AppError::blocked("TranscriptRecord v2 schema 불일치"));
    }
    let string = |key: &str| match object.get(key) {
        Some(CanonicalValue::String(value)) => Ok(value.clone()),
        _ => Err(AppError::blocked(format!(
            "TranscriptRecord v2 field type 불일치: {key}"
        ))),
    };
    let source_pointers = parse_canonical_source_pointers(object.get("source_pointers"))?;
    let tool_output_artifact = parse_tool_binding(object.get("tool_output_artifact"))?;
    Ok(TranscriptRecord {
        schema_version: TRANSCRIPT_SCHEMA_V2,
        record_id: string("record_id")?,
        project_id: string("project_id")?,
        session_id: string("session_id")?,
        workflow_id: string("workflow_id")?,
        kind: string("kind")?,
        causal_id: string("causal_id")?,
        content: string("content")?,
        content_hash: string("content_hash")?,
        source_pointers,
        recorded_at_ms: crate::strict_json::canonical_u128(
            &object,
            "recorded_at_ms",
            "TranscriptRecord v2",
        )?,
        tool_output_artifact,
        artifact_hash: string("artifact_hash")?,
    })
}

fn load_tool_output_artifact(path: &Path) -> Result<SanitizedToolOutputArtifact, AppError> {
    let metadata = fs::symlink_metadata(path).map_err(|err| {
        AppError::blocked(format!(
            "SanitizedToolOutputArtifact metadata 실패\n- path: {}\n- error: {err}",
            path.display()
        ))
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AppError::blocked(
            "SanitizedToolOutputArtifact regular-file boundary 불일치",
        ));
    }
    if metadata.len() > u64::try_from(MAX_TOOL_ARTIFACT_BYTES).unwrap_or(u64::MAX) {
        return Err(AppError::blocked(
            "SanitizedToolOutputArtifact canonical byte limit 초과",
        ));
    }
    let mut file = fs::File::open(path).map_err(|err| {
        AppError::blocked(format!(
            "SanitizedToolOutputArtifact 읽기 실패\n- path: {}\n- error: {err}",
            path.display()
        ))
    })?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.by_ref()
        .take(u64::try_from(MAX_TOOL_ARTIFACT_BYTES + 1).unwrap_or(u64::MAX))
        .read_to_end(&mut bytes)
        .map_err(|err| {
            AppError::blocked(format!(
                "SanitizedToolOutputArtifact bounded 읽기 실패: {err}"
            ))
        })?;
    if bytes.len() > MAX_TOOL_ARTIFACT_BYTES {
        return Err(AppError::blocked(
            "SanitizedToolOutputArtifact canonical byte limit 초과",
        ));
    }
    let body = String::from_utf8(bytes)
        .map_err(|_| AppError::blocked("SanitizedToolOutputArtifact UTF-8 불일치"))?;
    parse_tool_output_artifact_body(&body)
}

fn parse_tool_output_artifact_body(body: &str) -> Result<SanitizedToolOutputArtifact, AppError> {
    use crate::strict_json::CanonicalValue;

    if body.len() > MAX_TOOL_ARTIFACT_BYTES {
        return Err(AppError::blocked(
            "SanitizedToolOutputArtifact canonical byte limit 초과",
        ));
    }
    let object = crate::strict_json::parse_canonical_object(
        body,
        TOOL_ARTIFACT_KEYS,
        "SanitizedToolOutputArtifact",
    )?;
    if crate::strict_json::canonical_u64(&object, "schema_version", "SanitizedToolOutputArtifact")?
        != 1
        || string_from_canonical(&object, "redaction_policy")? != "credential-and-control-redaction"
        || crate::strict_json::canonical_u64(
            &object,
            "redaction_version",
            "SanitizedToolOutputArtifact",
        )? != 1
    {
        return Err(AppError::blocked(
            "SanitizedToolOutputArtifact schema/policy 불일치",
        ));
    }
    let boolean = |key: &str| match object.get(key) {
        Some(CanonicalValue::Bool(value)) => Ok(*value),
        _ => Err(AppError::blocked(format!(
            "SanitizedToolOutputArtifact boolean field 불일치: {key}"
        ))),
    };
    let artifact = SanitizedToolOutputArtifact {
        artifact_id: string_from_canonical(&object, "artifact_id")?,
        project_id: string_from_canonical(&object, "project_id")?,
        session_id: string_from_canonical(&object, "session_id")?,
        workflow_id: string_from_canonical(&object, "workflow_id")?,
        tool_id: string_from_canonical(&object, "tool_id")?,
        created_at_ms: crate::strict_json::canonical_u128(
            &object,
            "created_at_ms",
            "SanitizedToolOutputArtifact",
        )?,
        stdout: string_from_canonical(&object, "stdout")?,
        stderr: string_from_canonical(&object, "stderr")?,
        stdout_original_bytes: crate::strict_json::canonical_u64(
            &object,
            "stdout_original_bytes",
            "SanitizedToolOutputArtifact",
        )?,
        stderr_original_bytes: crate::strict_json::canonical_u64(
            &object,
            "stderr_original_bytes",
            "SanitizedToolOutputArtifact",
        )?,
        stdout_retained_chars: crate::strict_json::canonical_u64(
            &object,
            "stdout_retained_chars",
            "SanitizedToolOutputArtifact",
        )?,
        stderr_retained_chars: crate::strict_json::canonical_u64(
            &object,
            "stderr_retained_chars",
            "SanitizedToolOutputArtifact",
        )?,
        stdout_truncated: boolean("stdout_truncated")?,
        stderr_truncated: boolean("stderr_truncated")?,
        stdout_redacted: boolean("stdout_redacted")?,
        stderr_redacted: boolean("stderr_redacted")?,
        content_hash: string_from_canonical(&object, "content_hash")?,
    };
    validate_id("tool artifact id", &artifact.artifact_id)?;
    validate_id("tool id", &artifact.tool_id)?;
    validate_id("project id", &artifact.project_id)?;
    validate_id("session id", &artifact.session_id)?;
    validate_id("workflow id", &artifact.workflow_id)?;
    validate_sha256("tool artifact content hash", &artifact.content_hash)?;
    if artifact.stdout.len() > MAX_SANITIZED_STREAM_BYTES
        || artifact.stderr.len() > MAX_SANITIZED_STREAM_BYTES
        || artifact.stdout_retained_chars
            != u64::try_from(artifact.stdout.chars().count())
                .map_err(|_| AppError::blocked("stdout retained count overflow"))?
        || artifact.stderr_retained_chars
            != u64::try_from(artifact.stderr.chars().count())
                .map_err(|_| AppError::blocked("stderr retained count overflow"))?
        || artifact.content_hash != state::sha256_text(&artifact.payload())
        || artifact.to_json() != body
    {
        return Err(AppError::blocked(
            "SanitizedToolOutputArtifact byte/count/hash binding 불일치",
        ));
    }
    Ok(artifact)
}

fn string_from_canonical(
    object: &crate::strict_json::CanonicalObject,
    key: &str,
) -> Result<String, AppError> {
    match object.get(key) {
        Some(crate::strict_json::CanonicalValue::String(value)) => Ok(value.clone()),
        _ => Err(AppError::blocked(format!(
            "canonical string field 불일치: {key}"
        ))),
    }
}

fn validate_tool_binding_for_record(record: &TranscriptRecord) -> Result<(), AppError> {
    validate_tool_binding_shape_for_record(record)?;
    let Some(binding) = record.tool_output_artifact.as_ref() else {
        return Ok(());
    };
    let path = validated_tool_output_path(
        &record.project_id,
        &record.session_id,
        &record.workflow_id,
        &binding.id,
        false,
    )?;
    let artifact = load_tool_output_artifact(&path)?;
    if artifact.artifact_id != binding.id
        || artifact.project_id != record.project_id
        || artifact.session_id != record.session_id
        || artifact.workflow_id != record.workflow_id
        || artifact.tool_id != record.causal_id
        || artifact.content_hash != binding.hash
    {
        return Err(AppError::blocked(
            "TranscriptRecord v2 tool artifact owner/hash binding 불일치",
        ));
    }
    Ok(())
}

fn validate_tool_binding_shape_for_record(record: &TranscriptRecord) -> Result<(), AppError> {
    match (
        record.schema_version,
        record.kind.as_str(),
        &record.tool_output_artifact,
    ) {
        (TRANSCRIPT_SCHEMA_V1, _, None) => return Ok(()),
        (TRANSCRIPT_SCHEMA_V1, _, Some(_)) => {
            return Err(AppError::blocked(
                "TranscriptRecord v1 tool binding은 허용되지 않습니다.",
            ))
        }
        (TRANSCRIPT_SCHEMA_V2, "tool", Some(_)) => {}
        (TRANSCRIPT_SCHEMA_V2, "tool", None) => {
            return Err(AppError::blocked("TranscriptRecord v2 tool binding 누락"))
        }
        (TRANSCRIPT_SCHEMA_V2, _, None) => return Ok(()),
        (TRANSCRIPT_SCHEMA_V2, _, Some(_)) => {
            return Err(AppError::blocked(
                "TranscriptRecord v2 non-tool binding은 null이어야 합니다.",
            ))
        }
        _ => return Err(AppError::blocked("transcript schema version 불일치")),
    }

    let binding = record
        .tool_output_artifact
        .as_ref()
        .expect("tool binding checked above");
    validate_id("tool artifact id", &binding.id)?;
    validate_sha256("tool artifact hash", &binding.hash)?;
    let expected_path = tool_output_artifact_relative_path(
        &record.project_id,
        &record.session_id,
        &record.workflow_id,
        &binding.id,
    );
    if binding.path != expected_path {
        return Err(AppError::blocked(
            "TranscriptRecord v2 tool artifact path binding 불일치",
        ));
    }
    Ok(())
}

fn validate_tool_artifact_owner(
    artifact: &SanitizedToolOutputArtifact,
    workflow: &state::WorkflowRecord,
    tool_id: &str,
    artifact_id: &str,
) -> Result<(), AppError> {
    if artifact.artifact_id != artifact_id
        || artifact.project_id != workflow.project_id
        || artifact.session_id != workflow.session_id
        || artifact.workflow_id != workflow.workflow_id
        || artifact.tool_id != tool_id
    {
        return Err(AppError::blocked(
            "SanitizedToolOutputArtifact deterministic owner 충돌",
        ));
    }
    Ok(())
}

fn parse_source_pointers(
    value: Option<&crate::strict_json::Value>,
) -> Result<Vec<TranscriptSourcePointer>, AppError> {
    let Some(crate::strict_json::Value::Array(values)) = value else {
        return Err(AppError::blocked("transcript source_pointers type 불일치"));
    };
    let mut pointers = Vec::new();
    for value in values {
        let crate::strict_json::Value::Object(object) = value else {
            return Err(AppError::blocked("transcript source pointer type 불일치"));
        };
        if object
            .keys()
            .any(|key| !matches!(key.as_str(), "stable_ref" | "path" | "source_hash"))
        {
            return Err(AppError::blocked("transcript source pointer key 불일치"));
        }
        pointers.push(TranscriptSourcePointer {
            stable_ref: crate::strict_json::string(object, "stable_ref", "transcript pointer")?,
            path: crate::strict_json::string(object, "path", "transcript pointer")?,
            source_hash: crate::strict_json::string(object, "source_hash", "transcript pointer")?,
        });
    }
    Ok(pointers)
}

fn parse_canonical_source_pointers(
    value: Option<&crate::strict_json::CanonicalValue>,
) -> Result<Vec<TranscriptSourcePointer>, AppError> {
    use crate::strict_json::CanonicalValue;

    let Some(CanonicalValue::Array(values)) = value else {
        return Err(AppError::blocked(
            "TranscriptRecord v2 source_pointers type 불일치",
        ));
    };
    values
        .iter()
        .map(|value| {
            let CanonicalValue::Object(object) = value else {
                return Err(AppError::blocked(
                    "TranscriptRecord v2 source pointer type 불일치",
                ));
            };
            let keys = object
                .entries
                .iter()
                .map(|(key, _)| key.as_str())
                .collect::<Vec<_>>();
            if keys != ["stable_ref", "path", "source_hash"] {
                return Err(AppError::blocked(
                    "TranscriptRecord v2 source pointer key/order 불일치",
                ));
            }
            let string = |key: &str| match object.get(key) {
                Some(CanonicalValue::String(value)) => Ok(value.clone()),
                _ => Err(AppError::blocked(format!(
                    "TranscriptRecord v2 source pointer field 불일치: {key}"
                ))),
            };
            Ok(TranscriptSourcePointer {
                stable_ref: string("stable_ref")?,
                path: string("path")?,
                source_hash: string("source_hash")?,
            })
        })
        .collect()
}

fn parse_tool_binding(
    value: Option<&crate::strict_json::CanonicalValue>,
) -> Result<Option<ToolOutputArtifactBinding>, AppError> {
    use crate::strict_json::CanonicalValue;

    match value {
        Some(CanonicalValue::Null) => Ok(None),
        Some(CanonicalValue::Object(object)) => {
            let keys = object
                .entries
                .iter()
                .map(|(key, _)| key.as_str())
                .collect::<Vec<_>>();
            if keys != TOOL_BINDING_KEYS {
                return Err(AppError::blocked(
                    "TranscriptRecord v2 tool binding key/order 불일치",
                ));
            }
            let string = |key: &str| match object.get(key) {
                Some(CanonicalValue::String(value)) => Ok(value.clone()),
                _ => Err(AppError::blocked(format!(
                    "TranscriptRecord v2 tool binding field 불일치: {key}"
                ))),
            };
            Ok(Some(ToolOutputArtifactBinding {
                id: string("id")?,
                path: string("path")?,
                hash: string("hash")?,
            }))
        }
        _ => Err(AppError::blocked(
            "TranscriptRecord v2 tool_output_artifact type 불일치",
        )),
    }
}

fn render_source_pointers(pointers: &[TranscriptSourcePointer]) -> String {
    let rows = pointers
        .iter()
        .map(|pointer| {
            format!(
                "{{\"stable_ref\":\"{}\",\"path\":\"{}\",\"source_hash\":\"{}\"}}",
                ledger::json_string(&pointer.stable_ref),
                ledger::json_string(&pointer.path),
                ledger::json_string(&pointer.source_hash)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("[{rows}]")
}

fn render_tool_binding(binding: Option<&ToolOutputArtifactBinding>) -> String {
    binding.map_or_else(
        || "null".to_string(),
        |binding| {
            format!(
                "{{\"id\":\"{}\",\"path\":\"{}\",\"hash\":\"{}\"}}",
                ledger::json_string(&binding.id),
                ledger::json_string(&binding.path),
                binding.hash
            )
        },
    )
}

fn validate_expected_record(
    existing: &TranscriptRecord,
    workflow: &state::WorkflowRecord,
    kind: &str,
    causal_id: &str,
    content: &str,
    pointers: &[TranscriptSourcePointer],
) -> Result<(), AppError> {
    if existing.project_id != workflow.project_id
        || existing.session_id != workflow.session_id
        || existing.workflow_id != workflow.workflow_id
        || existing.kind != kind
        || existing.causal_id != causal_id
        || existing.content != content
        || existing.source_pointers != pointers
    {
        return Err(AppError::blocked(format!(
            "transcript deterministic record 충돌\n- record id: {}",
            existing.record_id
        )));
    }
    Ok(())
}

fn parse_event_details(details: &str) -> Result<Vec<(&str, &str)>, AppError> {
    if details.is_empty()
        || details.trim() != details
        || details.contains("  ")
        || details.contains(['\n', '\r', '\t'])
    {
        return Err(AppError::blocked("transcript event details spacing 불일치"));
    }
    let mut pairs = Vec::new();
    for part in details.split(' ') {
        let (key, value) = part
            .split_once('=')
            .ok_or_else(|| AppError::blocked("transcript event detail token 불일치"))?;
        if key.is_empty() || value.is_empty() || pairs.iter().any(|(stored, _)| *stored == key) {
            return Err(AppError::blocked(
                "transcript event detail key/value 불일치",
            ));
        }
        pairs.push((key, value));
    }
    Ok(pairs)
}

fn detail_from_pairs<'a>(pairs: &'a [(&'a str, &'a str)], key: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find_map(|(stored, value)| (*stored == key).then_some(*value))
}

fn validate_event_details_for_schema(details: &str, schema_version: u64) -> Result<(), AppError> {
    let pairs = parse_event_details(details)?;
    let actual = pairs.iter().map(|(key, _)| *key).collect::<Vec<_>>();
    let expected = match schema_version {
        TRANSCRIPT_SCHEMA_V1 => vec![
            "record_id",
            "workflow_id",
            "kind",
            "artifact_pointer",
            "artifact_hash",
            "content_hash",
        ],
        TRANSCRIPT_SCHEMA_V2 => vec![
            "record_id",
            "workflow_id",
            "kind",
            "artifact_pointer",
            "artifact_hash",
            "content_hash",
            "tool_output_artifact_id",
            "tool_output_artifact_path",
            "tool_output_artifact_hash",
        ],
        _ => return Err(AppError::blocked("transcript event schema 불일치")),
    };
    if actual != expected {
        return Err(AppError::blocked(
            "transcript event detail key/order 불일치",
        ));
    }
    Ok(())
}

fn validate_kind(kind: &str) -> Result<(), AppError> {
    if matches!(kind, "user" | "model" | "tool" | "evidence") {
        Ok(())
    } else {
        Err(AppError::blocked(format!("transcript kind 불일치: {kind}")))
    }
}

fn validate_id(label: &str, value: &str) -> Result<(), AppError> {
    if value.is_empty()
        || value.len() > 160
        || matches!(value, "." | "..")
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(AppError::blocked(format!("transcript {label} 형식 불일치")));
    }
    Ok(())
}

fn validate_source_pointer(pointer: &TranscriptSourcePointer) -> Result<(), AppError> {
    if pointer.stable_ref.is_empty()
        || pointer.stable_ref.len() > 4_096
        || pointer.stable_ref.contains(['\r', '\n'])
        || pointer.path.is_empty()
        || pointer.path.len() > 4_096
        || pointer.path.contains(['\r', '\n'])
        || Path::new(&pointer.path).components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
        || pointer.source_hash.len() != 64
        || !pointer
            .source_hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(AppError::blocked(
            "transcript source pointer boundary 불일치",
        ));
    }
    Ok(())
}

fn validate_sha256(label: &str, value: &str) -> Result<(), AppError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(AppError::blocked(format!("{label} 형식 불일치")))
    }
}

fn tool_output_artifact_relative_path(
    project_id: &str,
    session_id: &str,
    workflow_id: &str,
    artifact_id: &str,
) -> String {
    format!("state/tool-output/{project_id}/{session_id}/{workflow_id}/{artifact_id}.json")
}

fn validated_tool_output_path(
    project_id: &str,
    session_id: &str,
    workflow_id: &str,
    artifact_id: &str,
    create_parent: bool,
) -> Result<PathBuf, AppError> {
    for (label, value) in [
        ("project id", project_id),
        ("session id", session_id),
        ("workflow id", workflow_id),
        ("tool artifact id", artifact_id),
    ] {
        validate_id(label, value)?;
    }
    let app_root = paths::app_data_root();
    ensure_directory_boundary(&app_root, create_parent, true)?;
    let app_root = fs::canonicalize(&app_root)
        .map_err(|err| AppError::blocked(format!("app-data root 해석 실패: {err}")))?;
    ensure_directory_boundary(&paths::state_dir(), create_parent, false)?;
    let root = paths::tool_outputs_dir();
    ensure_directory_boundary(&root, create_parent, false)?;
    let root_canonical = fs::canonicalize(&root)
        .map_err(|err| AppError::blocked(format!("tool-output root 해석 실패: {err}")))?;
    if !root_canonical.starts_with(&app_root) {
        return Err(AppError::blocked("tool-output app-data 경계 이탈 차단"));
    }
    let mut parent = root;
    for component in [project_id, session_id, workflow_id] {
        parent.push(component);
        match fs::symlink_metadata(&parent) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(AppError::blocked(format!(
                    "tool-output path boundary 불일치: {}",
                    parent.display()
                )))
            }
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound && create_parent => {
                fs::create_dir(&parent).map_err(|err| {
                    AppError::runtime(format!(
                        "tool-output directory 생성 실패: {} ({err})",
                        parent.display()
                    ))
                })?;
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(AppError::blocked(format!(
                    "tool-output directory 누락: {}",
                    parent.display()
                )))
            }
            Err(err) => {
                return Err(AppError::blocked(format!(
                    "tool-output directory 검사 실패: {} ({err})",
                    parent.display()
                )))
            }
        }
    }
    let parent_canonical = fs::canonicalize(&parent)
        .map_err(|err| AppError::blocked(format!("tool-output parent 해석 실패: {err}")))?;
    if !parent_canonical.starts_with(&root_canonical) {
        return Err(AppError::blocked("tool-output root 이탈 차단"));
    }
    let path = paths::tool_output_file(project_id, session_id, workflow_id, artifact_id);
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(AppError::blocked("tool-output artifact path type 불일치"));
        }
        let canonical = fs::canonicalize(&path)
            .map_err(|err| AppError::blocked(format!("tool-output artifact 해석 실패: {err}")))?;
        if !canonical.starts_with(&root_canonical) {
            return Err(AppError::blocked("tool-output artifact root 이탈 차단"));
        }
    }
    Ok(path)
}

fn validated_transcript_path(
    project_id: &str,
    session_id: &str,
    record_id: &str,
    create_parent: bool,
) -> Result<PathBuf, AppError> {
    validate_id("project id", project_id)?;
    validate_id("session id", session_id)?;
    validate_id("record id", record_id)?;

    let app_root = paths::app_data_root();
    ensure_directory_boundary(&app_root, create_parent, true)?;
    let app_root_canonical = fs::canonicalize(&app_root).map_err(|err| {
        AppError::blocked(format!(
            "app-data root 해석 실패: {} ({err})",
            app_root.display()
        ))
    })?;
    let state_root = paths::state_dir();
    ensure_directory_boundary(&state_root, create_parent, false)?;
    let root = paths::transcripts_dir();
    ensure_directory_boundary(&root, create_parent, false)?;
    let root_canonical = fs::canonicalize(&root).map_err(|err| {
        AppError::blocked(format!(
            "transcript root 해석 실패: {} ({err})",
            root.display()
        ))
    })?;
    if !root_canonical.starts_with(&app_root_canonical) {
        return Err(AppError::blocked("transcript root app-data 경계 이탈 차단"));
    }

    let mut parent = root.clone();
    for component in [project_id, session_id] {
        parent.push(component);
        if let Ok(metadata) = fs::symlink_metadata(&parent) {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(AppError::blocked(format!(
                    "transcript path boundary 불일치: {}",
                    parent.display()
                )));
            }
        } else if create_parent {
            fs::create_dir(&parent).map_err(|err| {
                AppError::runtime(format!(
                    "transcript directory 생성 실패: {} ({err})",
                    parent.display()
                ))
            })?;
        }
    }
    let parent_canonical = fs::canonicalize(&parent).map_err(|err| {
        AppError::blocked(format!(
            "transcript directory 해석 실패: {} ({err})",
            parent.display()
        ))
    })?;
    if !parent_canonical.starts_with(&root_canonical) {
        return Err(AppError::blocked("transcript path root 이탈 차단"));
    }

    let path = paths::transcript_file(project_id, session_id, record_id);
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(AppError::blocked(format!(
                "transcript artifact path boundary 불일치: {}",
                path.display()
            )));
        }
        let canonical = fs::canonicalize(&path)
            .map_err(|err| AppError::blocked(format!("transcript artifact 해석 실패: {err}")))?;
        if !canonical.starts_with(&root_canonical) {
            return Err(AppError::blocked("transcript artifact root 이탈 차단"));
        }
    }
    Ok(path)
}

fn ensure_directory_boundary(
    path: &Path,
    create: bool,
    create_ancestors: bool,
) -> Result<(), AppError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(AppError::blocked(format!(
                "transcript directory boundary 불일치: {}",
                path.display()
            )));
        }
        Ok(_) => return Ok(()),
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
            return Err(AppError::blocked(format!(
                "transcript directory 검사 실패: {} ({err})",
                path.display()
            )));
        }
        Err(_) if !create => {
            return Err(AppError::blocked(format!(
                "transcript directory 누락: {}",
                path.display()
            )));
        }
        Err(_) => {}
    }

    let result = if create_ancestors {
        fs::create_dir_all(path)
    } else {
        fs::create_dir(path)
    };
    result.map_err(|err| {
        AppError::runtime(format!(
            "transcript directory 생성 실패: {} ({err})",
            path.display()
        ))
    })
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitized_stream_limits_use_utf8_bytes_at_each_boundary() {
        for length in [MAX_SANITIZED_STREAM_BYTES - 1, MAX_SANITIZED_STREAM_BYTES] {
            let value = "x".repeat(length);
            let sanitized = sanitize_tool_stream(Some(&value)).unwrap();
            assert_eq!(sanitized.text.len(), length);
            assert!(!sanitized.truncated);
        }
        let over = "x".repeat(MAX_SANITIZED_STREAM_BYTES + 1);
        let sanitized = sanitize_tool_stream(Some(&over)).unwrap();
        assert_eq!(sanitized.text.len(), MAX_SANITIZED_STREAM_BYTES);
        assert!(sanitized.truncated);

        let multibyte = "가".repeat((MAX_SANITIZED_STREAM_BYTES / 3) + 1);
        let sanitized = sanitize_tool_stream(Some(&multibyte)).unwrap();
        assert!(sanitized.text.len() <= MAX_SANITIZED_STREAM_BYTES);
        assert!(sanitized.truncated);
        assert!(sanitized.text.is_char_boundary(sanitized.text.len()));
    }

    #[test]
    fn transcript_content_limit_uses_utf8_bytes_at_each_boundary() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-transcript-byte-limit-{}",
            std::process::id()
        ));
        let project = root.join("project");
        let data = root.join("data");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&project).unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
        std::env::set_var("RPOTATO_DATA_HOME", &data);
        state::initialize().unwrap();
        let workflow = state::create_workflow("transcript byte boundary").unwrap();

        for length in [
            MAX_TRANSCRIPT_CONTENT_BYTES - 1,
            MAX_TRANSCRIPT_CONTENT_BYTES,
        ] {
            let content = "x".repeat(length);
            assert_eq!(content.len(), length);
            prepare_no_stream_tool_turn(&workflow, &format!("event-limit-{length}"), &content, &[])
                .unwrap();
        }
        let too_large = "x".repeat(MAX_TRANSCRIPT_CONTENT_BYTES + 1);
        assert!(
            prepare_no_stream_tool_turn(&workflow, "event-limit-over", &too_large, &[])
                .unwrap_err()
                .message
                .contains("content boundary")
        );
        let multibyte = "가".repeat((MAX_TRANSCRIPT_CONTENT_BYTES / 3) + 1);
        assert!(multibyte.chars().count() < MAX_TRANSCRIPT_CONTENT_BYTES);
        assert!(multibyte.len() > MAX_TRANSCRIPT_CONTENT_BYTES);
        assert!(
            prepare_no_stream_tool_turn(&workflow, "event-limit-utf8", &multibyte, &[])
                .unwrap_err()
                .message
                .contains("content boundary")
        );

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn prepared_no_stream_turn_installs_exact_artifacts_without_ledger_side_effect() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-prepared-transcript-test-{}",
            std::process::id()
        ));
        let project = root.join("project");
        let data = root.join("data");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&project).unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
        std::env::set_var("RPOTATO_DATA_HOME", &data);
        state::initialize().unwrap();
        let workflow = state::create_workflow("prepared transcript test").unwrap();
        let before_count = ledger::read_runtime_events().unwrap().len();

        let prepared = prepare_no_stream_tool_turn(
            &workflow,
            "event-patch-applied",
            "patch applied: proposal_id=proposal-a path=src/lib.rs",
            &[],
        )
        .unwrap();
        assert!(!prepared.tool_path.exists());
        assert!(!prepared.transcript_path.exists());
        assert_eq!(prepared.event.event_type, "transcript.recorded");
        assert_eq!(ledger::read_runtime_events().unwrap().len(), before_count);

        install_prepared_no_stream_tool_turn(&prepared).unwrap();
        install_prepared_no_stream_tool_turn(&prepared).unwrap();
        let artifact = load_tool_output_artifact(&prepared.tool_path).unwrap();
        assert_eq!(artifact.stdout, UNAVAILABLE_STREAM);
        assert_eq!(artifact.stderr, UNAVAILABLE_STREAM);
        assert_eq!(artifact.stdout_original_bytes, 0);
        assert_eq!(artifact.stderr_original_bytes, 0);
        assert_eq!(
            fs::read_to_string(&prepared.transcript_path).unwrap(),
            prepared.transcript_bytes
        );
        assert_eq!(
            load_record_path(&prepared.transcript_path).unwrap(),
            prepared.record
        );
        assert_eq!(ledger::read_runtime_events().unwrap().len(), before_count);

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn transcript_v2_tool_binding_strict_round_trip() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root =
            std::env::temp_dir().join(format!("rpotato-transcript-v2-test-{}", std::process::id()));
        let project = root.join("project");
        let data = root.join("data");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&project).unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
        std::env::set_var("RPOTATO_DATA_HOME", &data);

        state::initialize().unwrap();
        let workflow = state::create_workflow("transcript v2 strict test").unwrap();
        let record = record_workflow_turn_with_streams(
            &workflow,
            "tool",
            "tool-call-v2",
            "bounded tool result",
            &[],
            Some("ok api_key=SUPER_SECRET_SENTINEL\u{001b}[31m"),
            None,
        )
        .unwrap();
        assert_eq!(record.schema_version, TRANSCRIPT_SCHEMA_V2);
        let binding = record.tool_output_artifact.as_ref().unwrap();
        let transcript_path =
            paths::transcript_file(&record.project_id, &record.session_id, &record.record_id);
        let transcript_body = fs::read_to_string(&transcript_path).unwrap();
        assert!(!transcript_body.ends_with('\n'));
        crate::strict_json::parse_canonical_object(
            &transcript_body,
            TRANSCRIPT_V2_KEYS,
            "test TranscriptRecord v2",
        )
        .unwrap();
        assert_eq!(load_record_path(&transcript_path).unwrap(), record);

        let tool_path = paths::tool_output_file(
            &record.project_id,
            &record.session_id,
            &record.workflow_id,
            &binding.id,
        );
        let tool_body = fs::read_to_string(&tool_path).unwrap();
        assert!(!tool_body.contains("SUPER_SECRET_SENTINEL"));
        assert!(!tool_body.contains('\u{001b}'));
        let artifact = load_tool_output_artifact(&tool_path).unwrap();
        assert_eq!(artifact.stderr, UNAVAILABLE_STREAM);
        assert_eq!(artifact.stderr_original_bytes, 0);
        assert!(!artifact.stderr_truncated);
        assert!(!artifact.stderr_redacted);
        assert_eq!(artifact.content_hash, binding.hash);

        let event = ledger::read_runtime_events()
            .unwrap()
            .into_iter()
            .find(|event| {
                event.event_type == "transcript.recorded"
                    && event.details.contains(&record.record_id)
            })
            .unwrap();
        assert_eq!(record_from_event(&event).unwrap(), record);
        validate_event_details_for_schema(&event.details, TRANSCRIPT_SCHEMA_V2).unwrap();

        let reordered =
            transcript_body.replacen("{\"schema_version\":2,\"record_id\"", "{\"record_id\"", 1);
        fs::write(&transcript_path, reordered).unwrap();
        assert_eq!(load_record_path(&transcript_path).unwrap_err().code, 3);
        fs::write(&transcript_path, &transcript_body).unwrap();

        let non_tool =
            record_workflow_turn(&workflow, "user", "user-v2", "non-tool v2 record", &[]).unwrap();
        assert_eq!(non_tool.schema_version, TRANSCRIPT_SCHEMA_V2);
        assert!(non_tool.tool_output_artifact.is_none());

        let legacy_causal = "legacy-tool";
        let legacy_record_id = format!(
            "transcript-{}",
            &state::sha256_text(&format!(
                "{}\n{}\n{}\n{}\n{}",
                workflow.project_id,
                workflow.session_id,
                workflow.workflow_id,
                "tool",
                legacy_causal
            ))[..24]
        );
        let mut legacy = TranscriptRecord {
            schema_version: TRANSCRIPT_SCHEMA_V1,
            record_id: legacy_record_id.clone(),
            project_id: workflow.project_id.clone(),
            session_id: workflow.session_id.clone(),
            workflow_id: workflow.workflow_id.clone(),
            kind: "tool".to_string(),
            causal_id: legacy_causal.to_string(),
            content: "legacy result".to_string(),
            content_hash: state::sha256_text("legacy result"),
            source_pointers: Vec::new(),
            recorded_at_ms: 1,
            tool_output_artifact: None,
            artifact_hash: String::new(),
        };
        legacy.artifact_hash = state::sha256_text(&legacy.artifact_payload());
        let legacy_path = validated_transcript_path(
            &legacy.project_id,
            &legacy.session_id,
            &legacy.record_id,
            true,
        )
        .unwrap();
        let legacy_bytes = legacy.to_json();
        state::atomic_replace_bytes(&legacy_path, legacy_bytes.as_bytes()).unwrap();
        let retried =
            record_workflow_turn(&workflow, "tool", legacy_causal, "legacy result", &[]).unwrap();
        assert_eq!(retried.schema_version, TRANSCRIPT_SCHEMA_V1);
        assert!(retried.tool_output_artifact.is_none());
        assert_eq!(fs::read_to_string(&legacy_path).unwrap(), legacy_bytes);

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn transcript_record_is_idempotent_and_sqlite_rebuilds_from_canonical_artifacts() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-transcript-rebuild-test-{}",
            std::process::id()
        ));
        let project = root.join("project");
        let data = root.join("data");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(project.join("src")).unwrap();
        fs::write(project.join("src/lib.rs"), "pub const VALUE: i32 = 1;\n").unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
        std::env::set_var("RPOTATO_DATA_HOME", &data);

        state::initialize().unwrap();
        let workflow = state::create_workflow("값을 확인해줘").unwrap();
        let first =
            record_workflow_turn(&workflow, "user", "request", "값을 확인해줘", &[]).unwrap();
        let repeated =
            record_workflow_turn(&workflow, "user", "request", "값을 확인해줘", &[]).unwrap();
        let second =
            record_workflow_turn(&workflow, "tool", "context", "context prepared", &[]).unwrap();
        assert_eq!(first, repeated);
        assert_eq!(records_for_session(&workflow.session_id).unwrap().len(), 2);
        assert_eq!(
            crate::observability::status().unwrap().transcript_records,
            2
        );
        assert_projection_order(&[&first.record_id, &second.record_id]);

        let mut escaped = workflow.clone();
        escaped.project_id = "../escape".to_string();
        assert_eq!(
            record_workflow_turn(&escaped, "user", "request", "차단", &[])
                .unwrap_err()
                .code,
            3
        );
        let bad_pointer = SourcePointer {
            path: "src/lib.rs".to_string(),
            stable_ref: "src/lib.rs:1".to_string(),
            chars: 0,
            fingerprint: "not-a-sha256".to_string(),
            snippet: String::new(),
        };
        assert_eq!(
            record_workflow_turn(&workflow, "tool", "bad-pointer", "차단", &[bad_pointer])
                .unwrap_err()
                .code,
            3
        );
        let traversal_pointer = SourcePointer {
            path: "../secret".to_string(),
            stable_ref: "../secret:1".to_string(),
            chars: 0,
            fingerprint: "a".repeat(64),
            snippet: String::new(),
        };
        assert_eq!(
            record_workflow_turn(
                &workflow,
                "tool",
                "traversal-pointer",
                "차단",
                &[traversal_pointer]
            )
            .unwrap_err()
            .code,
            3
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;

            let outside = root.join("outside");
            fs::create_dir_all(&outside).unwrap();
            symlink(&outside, paths::transcripts_dir().join("symlink-project")).unwrap();
            assert_eq!(
                validated_transcript_path("symlink-project", "session-safe", "record-safe", true)
                    .unwrap_err()
                    .code,
                3
            );
        }

        let db = paths::observability_db_file();
        let _ = fs::remove_file(&db);
        let _ = fs::remove_file(db.with_extension("sqlite-wal"));
        let _ = fs::remove_file(db.with_extension("sqlite-shm"));
        assert_eq!(
            crate::observability::status().unwrap().transcript_records,
            2
        );
        assert_projection_order(&[&first.record_id, &second.record_id]);

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;

            let outside_root = root.join("outside-root");
            fs::create_dir_all(&outside_root).unwrap();
            fs::remove_dir_all(paths::transcripts_dir()).unwrap();
            symlink(&outside_root, paths::transcripts_dir()).unwrap();
            assert_eq!(
                validated_transcript_path("project-safe", "session-safe", "record-safe", true)
                    .unwrap_err()
                    .code,
                3
            );
        }

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);
    }

    fn assert_projection_order(expected: &[&str]) {
        let connection = rusqlite::Connection::open(paths::observability_db_file()).unwrap();
        let mut statement = connection
            .prepare(
                "SELECT record_id, ledger_event_id, event_ordinal
                   FROM transcript_records
               ORDER BY event_ordinal",
            )
            .unwrap();
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            rows.iter().map(|row| row.0.as_str()).collect::<Vec<_>>(),
            expected
        );
        assert!(rows.iter().all(|row| !row.1.is_empty() && row.2 > 0));
        assert!(rows.windows(2).all(|pair| pair[0].2 < pair[1].2));
    }
}
