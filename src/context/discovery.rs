use std::cmp::Reverse;
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};

use crate::adapters::filesystem::layout as paths;
use crate::app::policy_adapter::{self as policy, Decision, PathMode};
use crate::foundation::error::AppError;
use crate::runtime_core::knowledge::context::{
    truncate_chars, ContextPack, SourcePointer, MAX_CONTEXT_CHARS, MAX_CONTEXT_FILES,
    MAX_FILE_BYTES, MAX_FILE_CHARS,
};

const MAX_SCAN_FILES: usize = 512;

pub(super) fn build_filesystem_fallback(request: &str) -> Result<ContextPack, AppError> {
    let project_root = fs::canonicalize(paths::project_root()).map_err(|err| {
        AppError::runtime(format!(
            "project root를 해석하지 못했습니다: {} ({err})",
            paths::project_root().display()
        ))
    })?;
    let mut candidates = discover_candidate_files(&project_root)?;
    let request_terms = request_terms(request);
    candidates.sort_by_key(|path| (Reverse(score_path(path, &request_terms)), path.clone()));

    let mut source_pointers = Vec::new();
    let mut chars_read = 0usize;
    let mut files_considered = 0usize;

    for path in candidates {
        if source_pointers.len() >= MAX_CONTEXT_FILES || chars_read >= MAX_CONTEXT_CHARS {
            break;
        }
        files_considered += 1;
        let relative = relative_path(&project_root, &path);
        let decision = policy::classify_path(PathMode::Read, &relative)?;
        if decision.decision != Decision::Allow {
            continue;
        }
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        if metadata.len() > MAX_FILE_BYTES || !metadata.is_file() {
            continue;
        }
        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };
        if contents.trim().is_empty() {
            continue;
        }
        let remaining = MAX_CONTEXT_CHARS.saturating_sub(chars_read);
        let snippet_limit = remaining.min(MAX_FILE_CHARS);
        if snippet_limit == 0 {
            break;
        }
        let snippet = truncate_chars(&contents, snippet_limit);
        let chars = snippet.chars().count();
        chars_read += chars;
        source_pointers.push(SourcePointer {
            stable_ref: format!("{relative}:1"),
            path: relative,
            chars,
            fingerprint: content_fingerprint(&contents),
            snippet,
        });
    }

    Ok(ContextPack {
        project_root,
        origin: "filesystem-empty-ontology-fallback".to_string(),
        ontology_records_selected: 0,
        ontology_stale_rejected: 0,
        files_considered,
        files_read: source_pointers.len(),
        chars_read,
        dropped_files: files_considered.saturating_sub(source_pointers.len()),
        source_pointers,
    })
}

pub(super) fn discover_candidate_files(root: &Path) -> Result<Vec<PathBuf>, AppError> {
    let mut queue = VecDeque::from([root.to_path_buf()]);
    let mut files = Vec::new();

    while let Some(dir) = queue.pop_front() {
        if files.len() >= MAX_SCAN_FILES {
            break;
        }
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if path.is_dir() {
                if should_skip_dir(&name) {
                    continue;
                }
                queue.push_back(path);
            } else if path.is_file() && is_context_file(&path) {
                files.push(path);
                if files.len() >= MAX_SCAN_FILES {
                    break;
                }
            }
        }
    }

    Ok(files)
}

fn should_skip_dir(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | ".rpotato"
            | ".omx"
            | ".codex"
            | "target"
            | "node_modules"
            | "vendor"
            | "dist"
            | "build"
            | ".next"
            | ".venv"
            | "__pycache__"
    )
}

fn is_context_file(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    if matches!(
        file_name,
        "README.md" | "README.ko.md" | "PLAN.md" | "ROADMAP.md" | "Cargo.toml" | "package.json"
    ) {
        return true;
    }

    matches!(
        path.extension().and_then(|value| value.to_str()),
        Some(
            "rs" | "toml"
                | "md"
                | "json"
                | "yaml"
                | "yml"
                | "sh"
                | "ts"
                | "tsx"
                | "js"
                | "jsx"
                | "py"
                | "go"
                | "java"
                | "kt"
                | "swift"
                | "c"
                | "h"
                | "cpp"
                | "hpp"
                | "css"
                | "html"
                | "txt"
        )
    )
}

pub(super) fn request_terms(request: &str) -> Vec<String> {
    request
        .split(|ch: char| !ch.is_alphanumeric() && ch != '_' && ch != '-')
        .map(|term| term.trim().to_ascii_lowercase())
        .filter(|term| term.chars().count() >= 2)
        .collect()
}

pub(super) fn score_path(path: &Path, request_terms: &[String]) -> i32 {
    let path_text = path.display().to_string().to_ascii_lowercase();
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mut score = 0;

    for term in request_terms {
        if path_text.contains(term) {
            score += 100;
        }
    }
    if matches!(
        file_name.as_str(),
        "cargo.toml" | "package.json" | "pyproject.toml" | "README.md"
    ) {
        score += 60;
    }
    if path_text.contains("/src/") {
        score += 40;
    }
    if path_text.contains("/docs/") {
        score += 10;
    }
    score
}

pub(super) fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

pub(super) fn content_fingerprint(contents: &str) -> String {
    crate::app::workflow_adapter::state::sha256_text(contents)
}
