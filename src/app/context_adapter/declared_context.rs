//! Exact declared-file context assembly for subagent dispatch.

use std::fs;
use std::io::Read;

use crate::adapters::filesystem::layout as paths;
use crate::app::policy_adapter::{self as policy, Decision, PathMode};
use crate::foundation::error::AppError;
use crate::runtime_core::knowledge::context::{
    truncate_chars, ContextPack, SourcePointer, MAX_CONTEXT_CHARS, MAX_CONTEXT_FILES,
    MAX_FILE_BYTES, MAX_FILE_CHARS,
};

use super::discovery::content_fingerprint;

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
