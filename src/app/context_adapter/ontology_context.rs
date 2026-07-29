//! Ontology-backed current-request context assembly.

use std::fs;

use crate::adapters::filesystem::layout as paths;
use crate::app::ontology_adapter as ontology;
use crate::foundation::error::AppError;
use crate::runtime_core::knowledge::context::{
    truncate_chars, ContextPack, SourcePointer, MAX_CONTEXT_CHARS, MAX_CONTEXT_FILES,
    MAX_FILE_BYTES, MAX_FILE_CHARS,
};

use super::discovery::build_filesystem_fallback;

pub fn build_context_pack(request: &str) -> Result<ContextPack, AppError> {
    ontology::ensure_seeded()?;
    let selection = ontology::runtime_context(request, MAX_CONTEXT_FILES)?;
    if selection.current_records == 0 {
        return build_filesystem_fallback(request);
    }
    if selection.selected.is_empty() && selection.stale_rejected > 0 {
        return Ok(ContextPack {
            project_root: canonical_project_root()?,
            origin: "ontology-stale-dropped".to_string(),
            ontology_records_selected: 0,
            ontology_stale_rejected: selection.stale_rejected,
            files_considered: selection.stale_rejected,
            files_read: 0,
            chars_read: 0,
            dropped_files: selection.stale_rejected,
            source_pointers: Vec::new(),
        });
    }

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
        project_root: canonical_project_root()?,
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

fn canonical_project_root() -> Result<std::path::PathBuf, AppError> {
    fs::canonicalize(paths::project_root()).map_err(|err| {
        AppError::runtime(format!(
            "project root를 해석하지 못했습니다: {} ({err})",
            paths::project_root().display()
        ))
    })
}
