//! Strict current-source reads and best-effort historical-source reads.

use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::foundation::error::AppError;
use crate::foundation::integrity as checksum;
use crate::runtime_core::knowledge::ontology::{RuntimeSourceRead, SOURCE_POINTER_NONE};

use super::{canonical_project_root, relative_to_root};

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourcePointer {
    path: PathBuf,
    line: usize,
}

pub fn reread_runtime_source(
    pointer: &str,
    expected_hash: &str,
) -> Result<RuntimeSourceRead, AppError> {
    let source = resolve_source_pointer(pointer)?;
    reread_resolved_source_if_current(pointer, expected_hash, source, false)?.ok_or_else(|| {
        AppError::blocked(format!(
            "ontology source reread 차단\n- source pointer: {pointer}\n- 이유: graph source hash와 현재 원문 hash가 다릅니다.\n- 동작: ontology seed를 갱신한 뒤 다시 시도하세요."
        ))
    })
}

pub fn reread_historical_source(
    pointer: &str,
    expected_hash: &str,
) -> Result<Option<RuntimeSourceRead>, AppError> {
    let Some(source) = resolve_historical_source_pointer(pointer)? else {
        return Ok(None);
    };
    reread_resolved_source_if_current(pointer, expected_hash, source, true)
}

pub fn reread_report(pointer: &str) -> Result<String, AppError> {
    let source = resolve_source_pointer(pointer)?;
    let contents = fs::read_to_string(&source.path).map_err(|err| {
        AppError::runtime(format!(
            "source pointer 원문을 읽지 못했습니다: {} ({err})",
            source.path.display()
        ))
    })?;
    let hash = checksum::sha256_file(&source.path)?;
    let excerpt = contents
        .lines()
        .nth(source.line.saturating_sub(1))
        .unwrap_or("");

    Ok(format!(
        "ontology reread 결과\n- source pointer: {}\n- file: {}\n- line: {}\n- current sha256: {}\n- excerpt:\n  {} | {}\n- rule: 이 원문이 authoritative source입니다. Ontology snippet만 근거로 patch하지 않습니다.",
        pointer,
        source.path.display(),
        source.line,
        hash,
        source.line,
        excerpt
    ))
}

pub(super) fn resolve_project_relative_file(relative: &str) -> Result<PathBuf, AppError> {
    resolve_project_relative_file_if_present(relative)?.ok_or_else(|| {
        AppError::usage(format!(
            "project file이 존재하지 않거나 파일이 아닙니다: {relative}"
        ))
    })
}

pub(super) fn source_is_stale(pointer: &str, expected_hash: &str) -> bool {
    let Ok(source) = resolve_source_pointer(pointer) else {
        return true;
    };
    checksum::sha256_file(&source.path)
        .map(|current| current != expected_hash)
        .unwrap_or(true)
}

fn reread_resolved_source_if_current(
    pointer: &str,
    expected_hash: &str,
    source: SourcePointer,
    missing_is_stale: bool,
) -> Result<Option<RuntimeSourceRead>, AppError> {
    let bytes = match fs::read(&source.path) {
        Ok(bytes) => bytes,
        Err(err) if missing_is_stale && err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(None)
        }
        Err(err) => {
            return Err(AppError::runtime(format!(
                "ontology source 원문을 읽지 못했습니다: {} ({err})",
                source.path.display()
            )))
        }
    };
    let current_hash = checksum::sha256_bytes(&bytes);
    if current_hash != expected_hash {
        return Ok(None);
    }
    let contents = String::from_utf8(bytes).map_err(|err| {
        AppError::runtime(format!(
            "ontology source 원문을 읽지 못했습니다: {} ({err})",
            source.path.display()
        ))
    })?;
    let root = canonical_project_root()?;
    let relative_path = relative_to_root(&source.path, &root)
        .ok_or_else(|| AppError::blocked("ontology source가 project boundary를 벗어났습니다."))?;
    Ok(Some(RuntimeSourceRead {
        relative_path,
        stable_ref: pointer.to_string(),
        source_hash: current_hash,
        contents,
    }))
}

fn resolve_source_pointer(pointer: &str) -> Result<SourcePointer, AppError> {
    resolve_source_pointer_with_missing_policy(pointer, false)?.ok_or_else(|| {
        AppError::usage(format!(
            "project file이 존재하지 않거나 파일이 아닙니다: {pointer}"
        ))
    })
}

fn resolve_historical_source_pointer(pointer: &str) -> Result<Option<SourcePointer>, AppError> {
    resolve_source_pointer_with_missing_policy(pointer, true)
}

fn resolve_source_pointer_with_missing_policy(
    pointer: &str,
    allow_missing: bool,
) -> Result<Option<SourcePointer>, AppError> {
    if pointer.trim().is_empty() || pointer == SOURCE_POINTER_NONE {
        return Err(AppError::usage(
            "source pointer가 필요합니다. 예: src/main.rs:1",
        ));
    }
    if pointer.contains("://") {
        return Err(AppError::blocked(
            "source pointer는 remote URL을 허용하지 않습니다.",
        ));
    }
    let Some((relative, line)) = pointer.rsplit_once(':') else {
        return Err(AppError::usage(
            "source pointer는 <project-relative-path>:<line> 형식이어야 합니다.",
        ));
    };
    let line = line
        .parse::<usize>()
        .map_err(|_| AppError::usage("source pointer line은 양의 정수여야 합니다."))?;
    if line == 0 {
        return Err(AppError::usage(
            "source pointer line은 1 이상이어야 합니다.",
        ));
    }

    let path = resolve_project_relative_file_if_present(relative)?;
    match path {
        Some(path) => Ok(Some(SourcePointer { path, line })),
        None if allow_missing => Ok(None),
        None => Err(AppError::usage(format!(
            "project file이 존재하지 않거나 파일이 아닙니다: {relative}"
        ))),
    }
}

fn resolve_project_relative_file_if_present(relative: &str) -> Result<Option<PathBuf>, AppError> {
    if relative.trim().is_empty() {
        return Err(AppError::usage("project-relative path가 필요합니다."));
    }
    if relative.contains("://") {
        return Err(AppError::blocked("remote path는 허용하지 않습니다."));
    }
    let relative_path = Path::new(relative);
    if relative_path.is_absolute() {
        return Err(AppError::blocked(
            "project-relative path만 허용합니다. absolute path는 거부됩니다.",
        ));
    }
    if relative_path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(AppError::blocked(
            "project-relative path는 상위 경로(..)를 포함할 수 없습니다.",
        ));
    }

    let root = canonical_project_root()?;
    let candidate = root.join(relative_path);
    let canonical = match fs::canonicalize(&candidate) {
        Ok(canonical) => canonical,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(AppError::runtime(format!(
                "project file을 canonicalize하지 못했습니다: {} ({err})",
                candidate.display()
            )))
        }
    };
    if !canonical.starts_with(&root) {
        return Err(AppError::blocked(format!(
            "project boundary를 벗어난 path입니다: {}",
            canonical.display()
        )));
    }
    if !canonical.is_file() {
        return Ok(None);
    }
    Ok(Some(canonical))
}
