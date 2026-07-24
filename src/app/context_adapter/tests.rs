use super::*;
use std::path::PathBuf;

#[test]
fn filesystem_discovery_skips_generated_dirs_and_ranks_request_matches() {
    let root = std::env::temp_dir().join(format!(
        "rpotato-context-discovery-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("target")).unwrap();
    fs::write(root.join("Cargo.toml"), "[package]\nname = \"demo\"\n").unwrap();
    fs::write(root.join("src/needle.rs"), "pub fn needle() {}\n").unwrap();
    fs::write(root.join("target/needle.rs"), "generated\n").unwrap();

    let candidates = discovery::discover_candidate_files(&root).unwrap();
    let relative = candidates
        .iter()
        .map(|path| discovery::relative_path(&root, path))
        .collect::<Vec<_>>();
    assert!(relative.contains(&"Cargo.toml".to_string()));
    assert!(relative.contains(&"src/needle.rs".to_string()));
    assert!(!relative.contains(&"target/needle.rs".to_string()));

    let terms = discovery::request_terms("needle 테스트");
    assert!(
        discovery::score_path(&root.join("src/needle.rs"), &terms)
            > discovery::score_path(&root.join("Cargo.toml"), &terms)
    );

    let _ = fs::remove_dir_all(root);
}

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
    let root = std::env::temp_dir().join(format!("rpotato-context-test-{}", std::process::id()));
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
        context_limit_tokens: 131_072,
        transcript_records_considered: 0,
        transcript_turns_selected: 0,
        transcript_tokens: 0,
        transcript_chars: 0,
        transcript: Vec::new(),
        compacted_checkpoint: None,
        compaction_boundary: None,
        compaction_target_tokens: None,
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
    for index in 0..20 {
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
    let other_workflow =
        crate::app::workflow_adapter::state::WorkflowRecord::new(&other_identity, "other session");
    transcript::record_workflow_turn(
        &other_workflow,
        "user",
        "other-turn",
        "OTHER_SESSION_SENTINEL",
        &[],
    )
    .unwrap();

    let budget =
        crate::runtime_core::knowledge::context::ResumeContextBudget::for_context_limit(131_072);
    let resumed = rebuild_resume_context_for_limit(&workflow.session_id, None, 131_072).unwrap();
    assert!(resumed.transcript_turns_selected > 0);
    assert!(resumed.transcript_turns_selected <= budget.max_turns);
    assert!(resumed.transcript_tokens <= budget.transcript_budget_tokens);
    assert_eq!(resumed.context_limit_tokens, 131_072);
    assert_eq!(resumed.sources.files_read, 1);
    assert!(resumed.sources.chars_read <= MAX_CONTEXT_CHARS);
    assert!(!resumed.prompt_section().contains("OTHER_SESSION_SENTINEL"));

    let compacted = compaction::compact_manually_for_context_limit(131_072).unwrap();
    assert!(compacted.compacted);
    assert!(compacted.artifact_path.is_some());
    let after_compaction =
        rebuild_resume_context_for_limit(&workflow.session_id, None, 131_072).unwrap();
    assert!(after_compaction.compacted_checkpoint.is_some());
    assert!(after_compaction.compaction_boundary.is_some());
    assert!(after_compaction.transcript_turns_selected <= 16);
    assert!(after_compaction
        .prompt_section()
        .contains("untrusted historical data"));
    assert_eq!(
        crate::app::workflow_adapter::ledger::read_runtime_events()
            .unwrap()
            .iter()
            .filter(|event| event.event_type == "context.compacted")
            .count(),
        1
    );

    fs::write(&source_path, "fn main() { println!(\"changed\"); }\n").unwrap();
    let stale = rebuild_resume_context_for_limit(&workflow.session_id, None, 131_072).unwrap_err();
    assert_eq!(stale.code, 3);
    assert!(stale.message.contains("source reread 차단"));

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(root);
}
