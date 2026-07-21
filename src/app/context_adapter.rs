//! Context assembly adapter over ontology, transcript, and filesystem sources.

use std::collections::BTreeSet;
use std::fs;
use std::io::Read;

use crate::adapters::filesystem::layout as paths;
use crate::app::ontology_adapter as ontology;
use crate::app::policy_adapter::{self as policy, Decision, PathMode};
use crate::app::workflow_adapter::transcript;
use crate::foundation::error::AppError;
pub use crate::runtime_core::knowledge::context::{
    enforce_shared_source_budget, ContextPack, ResumeContext, SourcePointer,
};
use crate::runtime_core::knowledge::context::{
    truncate_chars, truncate_tail_chars, MAX_CONTEXT_CHARS, MAX_CONTEXT_FILES, MAX_FILE_BYTES,
    MAX_FILE_CHARS, MAX_RESUME_TRANSCRIPT_CHARS, MAX_RESUME_TURNS, MAX_RESUME_TURN_CHARS,
};

mod compaction;
mod discovery;

pub(crate) use compaction::{compact_automatically, compact_manually};
use discovery::{build_filesystem_fallback, content_fingerprint};

pub fn build_context_pack(request: &str) -> Result<ContextPack, AppError> {
    ontology::ensure_seeded()?;
    let selection = ontology::runtime_context(request, MAX_CONTEXT_FILES)?;
    if selection.current_records == 0 {
        return build_filesystem_fallback(request);
    }
    if selection.selected.is_empty() && selection.stale_rejected > 0 {
        return Err(AppError::blocked(
            "ontology context 준비 차단\n- 이유: 선택된 source pointer가 모두 stale입니다.\n- 동작: stale graph를 filesystem scan으로 우회하지 않습니다.",
        ));
    }

    let project_root = fs::canonicalize(paths::project_root()).map_err(|err| {
        AppError::runtime(format!(
            "project root를 해석하지 못했습니다: {} ({err})",
            paths::project_root().display()
        ))
    })?;
    let mut source_pointers = Vec::new();
    let mut chars_read = 0usize;
    for record in &selection.selected {
        if source_pointers.len() >= MAX_CONTEXT_FILES || chars_read >= MAX_CONTEXT_CHARS {
            break;
        }
        let source = ontology::reread_runtime_source(&record.source_pointer, &record.source_hash)?;
        if source.contents.len() as u64 > MAX_FILE_BYTES || source.contents.trim().is_empty() {
            continue;
        }
        let remaining = MAX_CONTEXT_CHARS.saturating_sub(chars_read);
        let snippet = truncate_chars(&source.contents, remaining.min(MAX_FILE_CHARS));
        let chars = snippet.chars().count();
        chars_read += chars;
        source_pointers.push(SourcePointer {
            path: source.relative_path,
            stable_ref: source.stable_ref,
            chars,
            fingerprint: source.source_hash,
            snippet,
        });
    }

    Ok(ContextPack {
        project_root,
        origin: "ontology".to_string(),
        ontology_records_selected: selection.selected.len(),
        ontology_stale_rejected: selection.stale_rejected,
        files_considered: selection.selected.len(),
        files_read: source_pointers.len(),
        chars_read,
        dropped_files: selection
            .selected
            .len()
            .saturating_sub(source_pointers.len()),
        source_pointers,
    })
}

pub fn build_declared_context_pack(read_paths: &[String]) -> Result<ContextPack, AppError> {
    if read_paths.is_empty() || read_paths.len() > MAX_CONTEXT_FILES {
        return Err(AppError::blocked(format!(
            "subagent declared context file 범위 오류: 1..={MAX_CONTEXT_FILES}"
        )));
    }
    let project_root = fs::canonicalize(paths::project_root()).map_err(|err| {
        AppError::runtime(format!(
            "project root를 해석하지 못했습니다: {} ({err})",
            paths::project_root().display()
        ))
    })?;
    let mut source_pointers = Vec::with_capacity(read_paths.len());
    let mut chars_read = 0usize;
    for relative in read_paths {
        let decision = policy::classify_path(PathMode::Read, relative)?;
        if decision.decision != Decision::Allow {
            return Err(AppError::blocked(format!(
                "subagent declared context 읽기 차단\n- path: {relative}\n- reason: {}",
                decision.reason
            )));
        }
        let requested = project_root.join(relative);
        let canonical = fs::canonicalize(&requested).map_err(|err| {
            AppError::blocked(format!(
                "subagent declared context path 해석 실패\n- path: {relative}\n- error: {err}"
            ))
        })?;
        if !canonical.starts_with(&project_root) || !canonical.is_file() {
            return Err(AppError::blocked(format!(
                "subagent declared context project/file boundary 차단: {relative}"
            )));
        }
        let metadata = fs::metadata(&canonical).map_err(|err| {
            AppError::blocked(format!(
                "subagent declared context metadata 실패\n- path: {relative}\n- error: {err}"
            ))
        })?;
        if metadata.len() > MAX_FILE_BYTES {
            return Err(AppError::blocked(format!(
                "subagent declared context file byte 상한 초과\n- path: {relative}\n- max: {MAX_FILE_BYTES}"
            )));
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        fs::File::open(&canonical)
            .and_then(|file| {
                file.take(MAX_FILE_BYTES + 1)
                    .read_to_end(&mut bytes)
                    .map(|_| ())
            })
            .map_err(|err| {
                AppError::blocked(format!(
                    "subagent declared context 읽기 실패\n- path: {relative}\n- error: {err}"
                ))
            })?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(AppError::blocked(format!(
                "subagent declared context file byte 상한 초과\n- path: {relative}\n- max: {MAX_FILE_BYTES}"
            )));
        }
        let contents = String::from_utf8(bytes).map_err(|_| {
            AppError::blocked(format!(
                "subagent declared context는 UTF-8 text file이어야 합니다: {relative}"
            ))
        })?;
        let canonical_after = fs::canonicalize(&requested).map_err(|err| {
            AppError::blocked(format!(
                "subagent declared context 재확인 실패\n- path: {relative}\n- error: {err}"
            ))
        })?;
        if canonical_after != canonical {
            return Err(AppError::blocked(format!(
                "subagent declared context path가 읽기 중 변경되었습니다: {relative}"
            )));
        }
        let remaining = MAX_CONTEXT_CHARS.saturating_sub(chars_read);
        let snippet = truncate_chars(&contents, remaining.min(MAX_FILE_CHARS));
        let chars = snippet.chars().count();
        chars_read += chars;
        source_pointers.push(SourcePointer {
            path: relative.clone(),
            stable_ref: format!("{relative}:1"),
            chars,
            fingerprint: content_fingerprint(&contents),
            snippet,
        });
    }
    Ok(ContextPack {
        project_root,
        origin: "subagent-declared-paths".to_string(),
        ontology_records_selected: 0,
        ontology_stale_rejected: 0,
        files_considered: read_paths.len(),
        files_read: source_pointers.len(),
        chars_read,
        dropped_files: 0,
        source_pointers,
    })
}

pub fn verify_declared_context_pack(
    expected: &ContextPack,
    read_paths: &[String],
) -> Result<ContextPack, AppError> {
    let actual = build_declared_context_pack(read_paths)?;
    let expected_bindings = expected
        .source_pointers
        .iter()
        .map(|pointer| (&pointer.path, &pointer.stable_ref, &pointer.fingerprint))
        .collect::<Vec<_>>();
    let actual_bindings = actual
        .source_pointers
        .iter()
        .map(|pointer| (&pointer.path, &pointer.stable_ref, &pointer.fingerprint))
        .collect::<Vec<_>>();
    if expected.project_root != actual.project_root
        || expected.files_read != actual.files_read
        || expected_bindings != actual_bindings
    {
        return Err(AppError::blocked(
            "subagent declared context source binding이 dispatch 전에 변경되었습니다.",
        ));
    }
    Ok(actual)
}

pub fn rebuild_resume_context(
    session_id: &str,
    exclude_workflow_id: Option<&str>,
) -> Result<ResumeContext, AppError> {
    let records = transcript::records_for_session(session_id)?;
    let compacted = compaction::load_current_artifact(session_id).ok().flatten();
    let boundary_index = compacted.as_ref().and_then(|artifact| {
        records
            .iter()
            .position(|record| record.record_id == artifact.boundary_record_id)
    });
    let compacted = boundary_index.zip(compacted);
    let eligible_records = compacted
        .as_ref()
        .map_or(records.as_slice(), |(index, _)| &records[index + 1..]);
    let eligible = eligible_records
        .iter()
        .filter(|record| exclude_workflow_id != Some(record.workflow_id.as_str()))
        .collect::<Vec<_>>();

    let mut selected_reversed = Vec::new();
    let mut transcript_chars = 0usize;
    for record in eligible.iter().rev() {
        if selected_reversed.len() >= MAX_RESUME_TURNS
            || transcript_chars >= MAX_RESUME_TRANSCRIPT_CHARS
        {
            break;
        }
        let remaining = MAX_RESUME_TRANSCRIPT_CHARS.saturating_sub(transcript_chars);
        let content = truncate_tail_chars(&record.content, remaining.min(MAX_RESUME_TURN_CHARS));
        let chars = content.chars().count();
        if chars == 0 {
            continue;
        }
        transcript_chars += chars;
        selected_reversed.push((record.kind.clone(), content));
    }
    selected_reversed.reverse();

    let project_root = fs::canonicalize(paths::project_root()).map_err(|err| {
        AppError::runtime(format!(
            "project root를 해석하지 못했습니다: {} ({err})",
            paths::project_root().display()
        ))
    })?;
    let mut seen = BTreeSet::new();
    let mut pointers_reversed = Vec::new();
    let mut files_considered = 0usize;
    for record in eligible.iter().rev() {
        for pointer in record.source_pointers.iter().rev() {
            if pointers_reversed.len() >= MAX_CONTEXT_FILES {
                break;
            }
            if seen.insert(pointer.stable_ref.clone()) {
                files_considered += 1;
                pointers_reversed.push(pointer.clone());
            }
        }
        if pointers_reversed.len() >= MAX_CONTEXT_FILES {
            break;
        }
    }
    pointers_reversed.reverse();

    let mut source_pointers = Vec::new();
    let mut chars_read = 0usize;
    for pointer in pointers_reversed {
        if chars_read >= MAX_CONTEXT_CHARS {
            break;
        }
        let source = ontology::reread_runtime_source(&pointer.stable_ref, &pointer.source_hash)?;
        if source.relative_path != pointer.path {
            return Err(AppError::blocked(format!(
                "resume source pointer binding 불일치\n- pointer: {}",
                pointer.stable_ref
            )));
        }
        if source.contents.len() as u64 > MAX_FILE_BYTES || source.contents.trim().is_empty() {
            continue;
        }
        let remaining = MAX_CONTEXT_CHARS.saturating_sub(chars_read);
        let snippet = truncate_chars(&source.contents, remaining.min(MAX_FILE_CHARS));
        let chars = snippet.chars().count();
        chars_read += chars;
        source_pointers.push(SourcePointer {
            path: source.relative_path,
            stable_ref: source.stable_ref,
            chars,
            fingerprint: source.source_hash,
            snippet,
        });
    }

    let compaction_target_tokens = compacted
        .as_ref()
        .map(|(_, artifact)| usize::try_from(artifact.post_compact_target_tokens))
        .transpose()
        .map_err(|_| AppError::blocked("compaction target token count overflow"))?;

    Ok(ResumeContext {
        session_id: session_id.to_string(),
        transcript_records_considered: eligible.len(),
        transcript_turns_selected: selected_reversed.len(),
        transcript_chars,
        transcript: selected_reversed,
        compacted_checkpoint: compacted
            .as_ref()
            .map(|(_, artifact)| artifact.checkpoint.clone()),
        compaction_boundary: compacted
            .as_ref()
            .map(|(_, artifact)| artifact.boundary_record_id.clone()),
        compaction_target_tokens,
        sources: ContextPack {
            project_root,
            origin: "durable-transcript-source-pointers".to_string(),
            ontology_records_selected: 0,
            ontology_stale_rejected: 0,
            files_considered,
            files_read: source_pointers.len(),
            chars_read,
            dropped_files: files_considered.saturating_sub(source_pointers.len()),
            source_pointers,
        },
    })
}

#[cfg(test)]
#[path = "context_adapter/tests.rs"]
mod tests;
