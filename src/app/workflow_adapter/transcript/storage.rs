use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::adapters::filesystem::layout as paths;
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::foundation::serialization as strict_json;
use crate::runtime_core::workflow::domain::transcript as transcript_domain;
use crate::runtime_core::workflow::storage_compat::transcript::{
    self as transcript_codec, TranscriptRecord, TranscriptSourcePointer,
};

use super::{
    SanitizedToolOutputArtifact, MAX_SANITIZED_STREAM_BYTES, MAX_TOOL_ARTIFACT_BYTES,
    TOOL_ARTIFACT_KEYS,
};

pub(super) fn load_record_path(path: &std::path::Path) -> Result<TranscriptRecord, AppError> {
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

pub(super) fn parse_transcript_record_body(body: &str) -> Result<TranscriptRecord, AppError> {
    transcript_codec::parse_record(body)
}

pub(super) fn load_tool_output_artifact(
    path: &Path,
) -> Result<SanitizedToolOutputArtifact, AppError> {
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

pub(super) fn parse_tool_output_artifact_body(
    body: &str,
) -> Result<SanitizedToolOutputArtifact, AppError> {
    use strict_json::CanonicalValue;

    if body.len() > MAX_TOOL_ARTIFACT_BYTES {
        return Err(AppError::blocked(
            "SanitizedToolOutputArtifact canonical byte limit 초과",
        ));
    }
    let object = strict_json::parse_canonical_object(
        body,
        TOOL_ARTIFACT_KEYS,
        "SanitizedToolOutputArtifact",
    )?;
    if strict_json::canonical_u64(&object, "schema_version", "SanitizedToolOutputArtifact")? != 1
        || string_from_canonical(&object, "redaction_policy")? != "credential-and-control-redaction"
        || strict_json::canonical_u64(&object, "redaction_version", "SanitizedToolOutputArtifact")?
            != 1
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
        created_at_ms: strict_json::canonical_u128(
            &object,
            "created_at_ms",
            "SanitizedToolOutputArtifact",
        )?,
        stdout: string_from_canonical(&object, "stdout")?,
        stderr: string_from_canonical(&object, "stderr")?,
        stdout_original_bytes: strict_json::canonical_u64(
            &object,
            "stdout_original_bytes",
            "SanitizedToolOutputArtifact",
        )?,
        stderr_original_bytes: strict_json::canonical_u64(
            &object,
            "stderr_original_bytes",
            "SanitizedToolOutputArtifact",
        )?,
        stdout_retained_chars: strict_json::canonical_u64(
            &object,
            "stdout_retained_chars",
            "SanitizedToolOutputArtifact",
        )?,
        stderr_retained_chars: strict_json::canonical_u64(
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
    object: &strict_json::CanonicalObject,
    key: &str,
) -> Result<String, AppError> {
    match object.get(key) {
        Some(strict_json::CanonicalValue::String(value)) => Ok(value.clone()),
        _ => Err(AppError::blocked(format!(
            "canonical string field 불일치: {key}"
        ))),
    }
}

pub(super) fn validate_tool_binding_for_record(record: &TranscriptRecord) -> Result<(), AppError> {
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

pub(super) fn validate_tool_binding_shape_for_record(
    record: &TranscriptRecord,
) -> Result<(), AppError> {
    transcript_codec::validate_tool_binding_shape(record)
}

pub(super) fn validate_tool_artifact_owner(
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

pub(super) fn validate_expected_record(
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

pub(super) fn parse_event_details(details: &str) -> Result<Vec<(&str, &str)>, AppError> {
    transcript_domain::parse_event_details(details)
}

pub(super) fn detail_from_pairs<'a>(pairs: &'a [(&'a str, &'a str)], key: &str) -> Option<&'a str> {
    transcript_domain::detail_from_pairs(pairs, key)
}

pub(super) fn validate_event_details_for_schema(
    details: &str,
    schema_version: u64,
) -> Result<(), AppError> {
    transcript_domain::validate_event_details_for_schema(details, schema_version)
}

pub(super) fn validate_kind(kind: &str) -> Result<(), AppError> {
    transcript_codec::validate_kind(kind)
}

pub(super) fn validate_id(label: &str, value: &str) -> Result<(), AppError> {
    transcript_codec::validate_id(label, value)
}

pub(super) fn validate_source_pointer(pointer: &TranscriptSourcePointer) -> Result<(), AppError> {
    transcript_codec::validate_source_pointer(pointer)
}

fn validate_sha256(label: &str, value: &str) -> Result<(), AppError> {
    transcript_codec::validate_sha256(label, value)
}

pub(super) fn tool_output_artifact_relative_path(
    project_id: &str,
    session_id: &str,
    workflow_id: &str,
    artifact_id: &str,
) -> String {
    transcript_codec::tool_output_artifact_relative_path(
        project_id,
        session_id,
        workflow_id,
        artifact_id,
    )
}

pub(super) fn validated_tool_output_path(
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

pub(super) fn validated_transcript_path(
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

pub(super) fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
