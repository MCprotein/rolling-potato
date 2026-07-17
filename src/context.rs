use std::cmp::Reverse;
use std::collections::{BTreeSet, VecDeque};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::adapters::filesystem::layout as paths;
use crate::app::policy_adapter::{self as policy, Decision, PathMode};
use crate::app::workflow_adapter::transcript;
use crate::foundation::error::AppError;
use crate::ontology;
pub use crate::runtime_core::knowledge::context::{
    enforce_shared_source_budget, ContextPack, ResumeContext, SourcePointer,
};
use crate::runtime_core::knowledge::context::{
    truncate_chars, truncate_tail_chars, MAX_CONTEXT_CHARS, MAX_CONTEXT_FILES, MAX_FILE_BYTES,
    MAX_FILE_CHARS, MAX_RESUME_TRANSCRIPT_CHARS, MAX_RESUME_TURNS, MAX_RESUME_TURN_CHARS,
};

const MAX_SCAN_FILES: usize = 512;

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
    let eligible = records
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

    Ok(ResumeContext {
        session_id: session_id.to_string(),
        transcript_records_considered: eligible.len(),
        transcript_turns_selected: selected_reversed.len(),
        transcript_chars,
        transcript: selected_reversed,
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

fn build_filesystem_fallback(request: &str) -> Result<ContextPack, AppError> {
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

fn discover_candidate_files(root: &Path) -> Result<Vec<PathBuf>, AppError> {
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

fn request_terms(request: &str) -> Vec<String> {
    request
        .split(|ch: char| !ch.is_alphanumeric() && ch != '_' && ch != '-')
        .map(|term| term.trim().to_ascii_lowercase())
        .filter(|term| term.chars().count() >= 2)
        .collect()
}

fn score_path(path: &Path, request_terms: &[String]) -> i32 {
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

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn content_fingerprint(contents: &str) -> String {
    crate::app::workflow_adapter::state::sha256_text(contents)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_context_reads_only_named_files_with_canonical_budget() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = paths::project_root();
        fs::create_dir_all(root.join("src")).unwrap();
        for (name, marker) in [("a.rs", 'a'), ("b.rs", 'b'), ("c.rs", 'c'), ("d.rs", 'd')] {
            fs::write(
                root.join("src").join(name),
                marker.to_string().repeat(2_000),
            )
            .unwrap();
        }
        let read_paths = ["a.rs", "b.rs", "c.rs", "d.rs"]
            .map(|name| format!("src/{name}"))
            .to_vec();
        let pack = build_declared_context_pack(&read_paths).unwrap();
        assert_eq!(pack.origin, "subagent-declared-paths");
        assert_eq!(pack.files_read, 4);
        assert_eq!(pack.chars_read, MAX_CONTEXT_CHARS);
        assert_eq!(
            pack.source_pointers
                .iter()
                .map(|pointer| pointer.path.as_str())
                .collect::<Vec<_>>(),
            vec!["src/a.rs", "src/b.rs", "src/c.rs", "src/d.rs"]
        );
        assert!(pack
            .source_pointers
            .iter()
            .all(|pointer| pointer.fingerprint.len() == 64));
    }

    #[test]
    fn declared_context_fails_closed_for_missing_outside_or_non_utf8_sources() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = paths::project_root();
        fs::write(root.join("binary.dat"), [0xff, 0xfe]).unwrap();
        for paths in [
            vec!["missing.rs".to_string()],
            vec!["../outside.rs".to_string()],
            vec!["binary.dat".to_string()],
        ] {
            assert!(build_declared_context_pack(&paths).is_err());
        }
    }

    #[test]
    fn declared_context_enforces_exact_file_count_and_byte_bounds() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = paths::project_root();
        fs::write(root.join("max.txt"), vec![b'x'; MAX_FILE_BYTES as usize]).unwrap();
        fs::write(
            root.join("over.txt"),
            vec![b'x'; MAX_FILE_BYTES as usize + 1],
        )
        .unwrap();
        assert!(build_declared_context_pack(&["max.txt".to_string()]).is_ok());
        assert!(build_declared_context_pack(&["over.txt".to_string()]).is_err());
        assert!(build_declared_context_pack(&[]).is_err());
        assert!(build_declared_context_pack(
            &(0..=MAX_CONTEXT_FILES)
                .map(|index| format!("file-{index}.txt"))
                .collect::<Vec<_>>()
        )
        .is_err());
    }

    #[test]
    fn context_pack_reads_bounded_project_files_and_skips_generated_dirs() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root =
            std::env::temp_dir().join(format!("rpotato-context-test-{}", std::process::id()));
        let project_root = root.join("project");
        fs::create_dir_all(project_root.join("src")).unwrap();
        fs::create_dir_all(project_root.join("target")).unwrap();
        fs::write(
            project_root.join("Cargo.toml"),
            "[package]\nname = \"demo\"\n",
        )
        .unwrap();
        fs::write(project_root.join("src").join("main.rs"), "fn main() {}\n").unwrap();
        fs::write(
            project_root.join("target").join("generated.rs"),
            "generated",
        )
        .unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let pack = build_context_pack("main 테스트").unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");

        assert_eq!(pack.origin, "ontology");
        assert!(pack.ontology_records_selected > 0);
        assert_eq!(pack.ontology_stale_rejected, 0);
        assert!(pack.files_read > 0);
        assert!(pack
            .source_pointers
            .iter()
            .any(|pointer| pointer.path == "src/main.rs"));
        assert!(pack
            .source_pointers
            .iter()
            .all(|pointer| !pointer.path.starts_with("target/")));
        assert!(pack.prompt_section().contains("source pointer"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn current_and_resume_sources_share_one_budget_and_deduplicate() {
        let pointer = |name: &str, chars: usize| SourcePointer {
            path: name.to_string(),
            stable_ref: format!("{name}:1"),
            chars,
            fingerprint: "a".repeat(64),
            snippet: name.repeat(chars.div_ceil(name.len())),
        };
        let pack = |pointers: Vec<SourcePointer>| ContextPack {
            project_root: PathBuf::from("/project"),
            origin: "test".to_string(),
            ontology_records_selected: 0,
            ontology_stale_rejected: 0,
            files_considered: pointers.len(),
            files_read: pointers.len(),
            chars_read: pointers.iter().map(|pointer| pointer.chars).sum(),
            dropped_files: 0,
            source_pointers: pointers,
        };
        let mut current = pack(vec![
            pointer("current.rs", 1_800),
            pointer("shared.rs", 1_800),
        ]);
        let mut resume = ResumeContext {
            session_id: "session-test".to_string(),
            transcript_records_considered: 0,
            transcript_turns_selected: 0,
            transcript_chars: 0,
            transcript: Vec::new(),
            sources: pack(vec![
                pointer("shared.rs", 1_000),
                pointer("older.rs", 1_000),
            ]),
        };

        enforce_shared_source_budget(&mut resume, &mut current);

        let pointer_count = current.files_read + resume.sources.files_read;
        let source_chars = current.chars_read + resume.sources.chars_read;
        let prompt = format!("{}{}", resume.prompt_section(), current.prompt_section());
        assert!(pointer_count <= MAX_CONTEXT_FILES);
        assert!(source_chars <= MAX_CONTEXT_CHARS);
        assert_eq!(prompt.matches("source pointer: shared.rs:1").count(), 1);
    }

    #[test]
    fn resume_context_is_bounded_and_rejects_stale_source_pointer() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-resume-context-test-{}",
            std::process::id()
        ));
        let project_root = root.join("project");
        let source_path = project_root.join("src/main.rs");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(source_path.parent().unwrap()).unwrap();
        fs::write(&source_path, "fn main() {}\n").unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        crate::app::workflow_adapter::state::initialize().unwrap();
        let workflow =
            crate::app::workflow_adapter::state::create_workflow("resume context test").unwrap();
        let pointer = SourcePointer {
            path: "src/main.rs".to_string(),
            stable_ref: "src/main.rs:1".to_string(),
            chars: 0,
            fingerprint: crate::foundation::integrity::sha256_file(&source_path).unwrap(),
            snippet: String::new(),
        };
        for index in 0..12 {
            transcript::record_workflow_turn(
                &workflow,
                if index % 2 == 0 { "user" } else { "model" },
                &format!("turn-{index}"),
                &format!("turn {index} {}", "x".repeat(500)),
                std::slice::from_ref(&pointer),
            )
            .unwrap();
        }
        let other_identity = crate::app::workflow_adapter::ledger::RuntimeIdentity {
            project_id: workflow.project_id.clone(),
            session_id: "session-other".to_string(),
            project_root: project_root.display().to_string(),
        };
        let other_workflow = crate::app::workflow_adapter::state::WorkflowRecord::new(
            &other_identity,
            "other session",
        );
        transcript::record_workflow_turn(
            &other_workflow,
            "user",
            "other-turn",
            "OTHER_SESSION_SENTINEL",
            &[],
        )
        .unwrap();

        let resumed = rebuild_resume_context(&workflow.session_id, None).unwrap();
        assert!(resumed.transcript_turns_selected > 0);
        assert!(resumed.transcript_turns_selected <= MAX_RESUME_TURNS);
        assert!(resumed.transcript_chars <= MAX_RESUME_TRANSCRIPT_CHARS);
        assert_eq!(resumed.sources.files_read, 1);
        assert!(resumed.sources.chars_read <= MAX_CONTEXT_CHARS);
        assert!(!resumed.prompt_section().contains("OTHER_SESSION_SENTINEL"));

        fs::write(&source_path, "fn main() { println!(\"changed\"); }\n").unwrap();
        let stale = rebuild_resume_context(&workflow.session_id, None).unwrap_err();
        assert_eq!(stale.code, 3);
        assert!(stale.message.contains("source reread 차단"));

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);
    }
}
