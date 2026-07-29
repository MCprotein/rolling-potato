#[test]
fn tui_read_budget_clamps_zero_and_overflow() {
    assert_eq!(
        TuiReadBudget::bounded(0, 0),
        TuiReadBudget {
            max_items: 1,
            max_chars: 1,
        }
    );
    assert_eq!(
        TuiReadBudget::bounded(usize::MAX, usize::MAX),
        TuiReadBudget {
            max_items: crate::surfaces::tui::runtime_bridge::TUI_MAX_ITEMS,
            max_chars: crate::surfaces::tui::runtime_bridge::TUI_MAX_CHARS,
        }
    );
}
#[test]
fn approvals_never_report_complete_when_canonical_tail_is_truncated() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-runtime-approvals-truncated-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::fs::create_dir_all(paths::project_root()).unwrap();
    let initialized = state::initialize().unwrap();
    let older_approval = ledger::new_event_for(
        &initialized.identity,
        "team.admission.policy_blocked",
        "older approval",
        "bounded tail 밖의 승인",
    );
    ledger::append_event(&older_approval).unwrap();
    for index in 0..80 {
        let noise = ledger::new_event_for(
            &initialized.identity,
            "runtime.noise",
            "tail displacement",
            &format!("index={index}"),
        );
        ledger::append_event(&noise).unwrap();
    }
    state::create_workflow("refresh current-state binding").unwrap();

    let page = read_tui_page(TuiReadRequest::Approvals {
        page: 0,
        budget: TuiReadBudget::bounded(20, 24 * 1024),
    })
    .unwrap();

    assert_eq!(page.continuation, TuiReadContinuation::Truncated);
    assert!(page
        .lines
        .iter()
        .all(|line| !line.contains(&older_approval.event_id)));

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn tui_read_facade_is_bounded_fresh_and_non_mutating_with_tool_output() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-runtime-read-facade-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::fs::create_dir_all(paths::project_root()).unwrap();
    state::initialize().unwrap();
    let workflow = state::create_workflow("read facade fixture").unwrap();
    let record = transcript::record_workflow_turn_with_streams(
        &workflow,
        "tool",
        "tool-read-facade",
        "tool finished",
        &[],
        Some("bounded stdout"),
        Some("bounded stderr"),
    )
    .unwrap();
    let artifact_id = record.tool_output_artifact.unwrap().id;
    let before = (
        std::fs::read(paths::current_state_file()).unwrap(),
        std::fs::read(paths::runtime_ledger_file()).unwrap(),
        std::fs::read(paths::observability_db_file()).unwrap(),
    );
    let budget = TuiReadBudget::bounded(4, 64);

    let tool = read_tui_page(TuiReadRequest::ToolOutput {
        artifact_id,
        page: 0,
        budget,
    })
    .unwrap();
    let transcript = read_tui_page(TuiReadRequest::Transcript {
        session_id: workflow.session_id.clone(),
        page: 0,
        budget,
    })
    .unwrap();
    let sessions = read_tui_page(TuiReadRequest::Sessions { page: 0, budget }).unwrap();

    assert_eq!(tool.title, "tool-output");
    assert!(tool.lines.concat().contains("artifact:"));
    assert_eq!(tool.freshness, TuiFreshness::Fresh);
    assert!(tool.lines.iter().all(|line| line.chars().count() <= 64));
    assert_eq!(transcript.freshness, TuiFreshness::Fresh);
    assert!(transcript.lines.len() <= 4);
    assert_eq!(sessions.freshness, TuiFreshness::Fresh);
    assert!(sessions.lines.len() <= 4);
    assert_eq!(
        before,
        (
            std::fs::read(paths::current_state_file()).unwrap(),
            std::fs::read(paths::runtime_ledger_file()).unwrap(),
            std::fs::read(paths::observability_db_file()).unwrap(),
        )
    );

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn tui_tool_output_rejects_canonical_artifact_from_another_project() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-runtime-tool-cross-project-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let project = root.join("project-current");
    let data = root.join("data");
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
    std::env::set_var("RPOTATO_DATA_HOME", &data);
    std::fs::create_dir_all(&project).unwrap();
    state::initialize().unwrap();

    let other_identity = ledger::RuntimeIdentity {
        project_id: "project-other-security-fixture".to_string(),
        session_id: "session-other-security-fixture".to_string(),
        project_root: root.join("project-other").display().to_string(),
    };
    let other_workflow = state::WorkflowRecord::new(&other_identity, "other project tool");
    let other_record = transcript::record_workflow_turn_with_streams(
        &other_workflow,
        "tool",
        "tool-cross-project",
        "other project tool finished",
        &[],
        Some("CROSS_PROJECT_STDOUT_MUST_NOT_RENDER"),
        Some("CROSS_PROJECT_STDERR_MUST_NOT_RENDER"),
    )
    .unwrap();
    let artifact_id = other_record.tool_output_artifact.unwrap().id;

    let page = read_tui_page(TuiReadRequest::ToolOutput {
        artifact_id,
        page: 0,
        budget: TuiReadBudget::bounded(16, 64 * 1024),
    })
    .unwrap();
    assert_eq!(page.freshness, TuiFreshness::Unavailable);
    let rendered = page.lines.join("\n");
    assert!(!rendered.contains("CROSS_PROJECT_STDOUT_MUST_NOT_RENDER"));
    assert!(!rendered.contains("CROSS_PROJECT_STDERR_MUST_NOT_RENDER"));

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn tui_read_facade_all_views_are_canonical_bounded_fresh_and_non_mutating() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-runtime-read-facade-matrix-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let project = root.join("project");
    let data = root.join("data");
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
    std::env::set_var("RPOTATO_DATA_HOME", &data);
    std::fs::create_dir_all(&project).unwrap();
    state::initialize().unwrap();
    std::fs::write(project.join("fixture.txt"), "before\n").unwrap();
    let mut workflow = state::create_workflow("read facade matrix fixture").unwrap();
    let proposal = patch::prepare_workflow_proposal(
        &workflow.workflow_id,
        &workflow.action_id,
        "fixture.txt",
        "before",
        "after",
        "pwd",
    )
    .unwrap();
    let proposal_id = proposal.proposal_id.clone();
    workflow.source_path = proposal.relative_path;
    workflow.source_hash = proposal.original_sha256.clone();
    workflow.before_hash = proposal.original_sha256;
    workflow.after_hash = proposal.proposed_sha256;
    workflow.proposal_id = proposal.proposal_id;
    workflow.proposal_hash = proposal.proposal_hash;
    workflow.approval_credential_hash = proposal.approval_credential_hash;
    workflow.verification_plan = proposal.verification_command;
    workflow.approval_state = "pending".to_string();
    workflow.phase = "pending-approval".to_string();
    workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();
    let record = transcript::record_workflow_turn_with_streams(
        &workflow,
        "tool",
        "tool-read-facade-matrix",
        "canonical tool finished",
        &[],
        Some("bounded stdout"),
        Some("bounded stderr"),
    )
    .unwrap();
    let artifact_id = record.tool_output_artifact.as_ref().unwrap().id.clone();
    let existing_artifact = paths::tool_output_file(
        &workflow.project_id,
        &workflow.session_id,
        &workflow.workflow_id,
        &artifact_id,
    );
    let orphan_id = "tool-output-orphan-read-facade";
    std::fs::write(
        existing_artifact
            .parent()
            .unwrap()
            .join(format!("{orphan_id}.json")),
        std::fs::read(&existing_artifact).unwrap(),
    )
    .unwrap();
    let connection = rusqlite::Connection::open(paths::observability_db_file()).unwrap();
    connection
        .execute(
            "INSERT INTO transcript_records (
                record_id, session_id, workflow_id, ledger_event_id, event_ordinal,
                record_kind, causal_id, content, content_hash, source_pointers_json,
                artifact_pointer, artifact_hash, recorded_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            rusqlite::params![
                "record-sqlite-only-forged",
                workflow.session_id,
                workflow.workflow_id,
                "event-sqlite-only-forged",
                9_999_i64,
                "assistant",
                "causal-sqlite-only-forged",
                "SQLITE_ONLY_FORGED",
                "f".repeat(64),
                "[]",
                "state/transcripts/forged.json",
                "e".repeat(64),
                9_999_i64,
            ],
        )
        .unwrap();
    drop(connection);
    let before = (snapshot_tree(&project), snapshot_tree(&data));
    let budget = TuiReadBudget::bounded(usize::MAX, usize::MAX);

    let pages = vec![
        read_tui_page(TuiReadRequest::Overview { budget }).unwrap(),
        read_tui_page(TuiReadRequest::Monitor { budget }).unwrap(),
        read_tui_page(TuiReadRequest::Sessions { page: 0, budget }).unwrap(),
        read_tui_page(TuiReadRequest::Transcript {
            session_id: workflow.session_id.clone(),
            page: 0,
            budget,
        })
        .unwrap(),
        read_tui_page(TuiReadRequest::ToolOutput {
            artifact_id: artifact_id.clone(),
            page: 0,
            budget,
        })
        .unwrap(),
        read_tui_page(TuiReadRequest::Approvals { page: 0, budget }).unwrap(),
        read_tui_page(TuiReadRequest::Diff {
            proposal_id: proposal_id.clone(),
            page: 0,
            budget,
        })
        .unwrap(),
        read_tui_page(TuiReadRequest::Evidence { page: 0, budget }).unwrap(),
    ];
    let orphan = read_tui_page(TuiReadRequest::ToolOutput {
        artifact_id: orphan_id.to_string(),
        page: 0,
        budget,
    })
    .unwrap();

    assert_eq!(
        pages
            .iter()
            .map(|page| page.title.as_str())
            .collect::<Vec<_>>(),
        [
            "overview",
            "monitor",
            "sessions",
            "transcript",
            "tool-output",
            "approvals",
            "diff",
            "evidence",
        ]
    );
    for page in &pages {
        assert_eq!(page.freshness, TuiFreshness::Fresh, "{}", page.title);
        assert!(page.authority.ledger_sequence.is_some(), "{}", page.title);
        assert!(page.authority.ledger_hash.is_some(), "{}", page.title);
        assert!(page.authority.validated_at_ms.is_some(), "{}", page.title);
        assert!(page.lines.len() <= 120, "{}", page.title);
        assert!(
            page.lines
                .iter()
                .map(|line| line.chars().count())
                .sum::<usize>()
                <= 65_536,
            "{}",
            page.title
        );
    }
    assert!(!pages[3].lines.concat().contains("SQLITE_ONLY_FORGED"));
    assert_eq!(orphan.freshness, TuiFreshness::Unavailable);
    assert!(matches!(
        orphan.continuation,
        TuiReadContinuation::Unavailable | TuiReadContinuation::Truncated
    ));
    let after = (snapshot_tree(&project), snapshot_tree(&data));
    let tree_delta =
        |label: &str, before: &BTreeMap<String, Vec<u8>>, after: &BTreeMap<String, Vec<u8>>| {
            let mut keys = before
                .keys()
                .chain(after.keys())
                .cloned()
                .collect::<Vec<_>>();
            keys.sort();
            keys.dedup();
            keys.into_iter()
                .filter_map(|key| {
                    let old = before.get(&key);
                    let new = after.get(&key);
                    (old != new).then(|| {
                        format!(
                            "{label}:{key}:{}->{}",
                            old.map(Vec::len)
                                .map_or_else(|| "missing".to_string(), |len| len.to_string()),
                            new.map(Vec::len)
                                .map_or_else(|| "missing".to_string(), |len| len.to_string())
                        )
                    })
                })
                .collect::<Vec<_>>()
        };
    let mut delta = tree_delta("project", &before.0, &after.0);
    delta.extend(tree_delta("data", &before.1, &after.1));
    assert!(delta.is_empty(), "TUI read mutated state: {delta:#?}");

    let database = paths::observability_db_file();
    let hidden_database = database.with_extension("sqlite.unavailable");
    std::fs::rename(&database, &hidden_database).unwrap();
    let unavailable_before = (snapshot_tree(&project), snapshot_tree(&data));
    let unavailable_pages = vec![
        read_tui_page(TuiReadRequest::Overview { budget }).unwrap(),
        read_tui_page(TuiReadRequest::Monitor { budget }).unwrap(),
        read_tui_page(TuiReadRequest::Sessions {
            page: u64::MAX,
            budget,
        })
        .unwrap(),
        read_tui_page(TuiReadRequest::Transcript {
            session_id: workflow.session_id.clone(),
            page: 0,
            budget,
        })
        .unwrap(),
        read_tui_page(TuiReadRequest::ToolOutput {
            artifact_id: artifact_id.clone(),
            page: 0,
            budget,
        })
        .unwrap(),
        read_tui_page(TuiReadRequest::Approvals { page: 0, budget }).unwrap(),
        read_tui_page(TuiReadRequest::Diff {
            proposal_id: proposal_id.clone(),
            page: 0,
            budget,
        })
        .unwrap(),
        read_tui_page(TuiReadRequest::Evidence { page: 0, budget }).unwrap(),
    ];
    for page in &unavailable_pages {
        assert_eq!(page.freshness, TuiFreshness::Unavailable, "{}", page.title);
        assert_eq!(page.authority.projected_sequence, None, "{}", page.title);
    }
    assert!(unavailable_pages[2].lines.is_empty());
    assert_eq!(
        unavailable_before,
        (snapshot_tree(&project), snapshot_tree(&data)),
        "unavailable projection reads must not mutate any file"
    );
    std::fs::rename(&hidden_database, &database).unwrap();

    let connection = rusqlite::Connection::open(&database).unwrap();
    assert_eq!(
        connection
            .execute(
                "DELETE FROM ledger_events WHERE rowid = (SELECT MAX(rowid) FROM ledger_events)",
                [],
            )
            .unwrap(),
        1
    );
    connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .unwrap();
    drop(connection);
    let stale_before = (snapshot_tree(&project), snapshot_tree(&data));
    let stale = read_tui_page(TuiReadRequest::Overview { budget }).unwrap();
    assert_eq!(stale.freshness, TuiFreshness::Stale);
    assert_eq!(
        stale.authority.projected_sequence,
        stale
            .authority
            .ledger_sequence
            .and_then(|sequence| sequence.checked_sub(1))
    );
    assert_eq!(
        stale_before,
        (snapshot_tree(&project), snapshot_tree(&data)),
        "stale projection read must not mutate DB/WAL/SHM or canonical state"
    );

    std::fs::create_dir_all(paths::projection_lag_dir()).unwrap();
    std::fs::write(
        paths::projection_lag_dir().join("corrupt-unbound.json"),
        "{}",
    )
    .unwrap();
    let corrupt_before = (snapshot_tree(&project), snapshot_tree(&data));
    let corrupt = read_tui_page(TuiReadRequest::Overview { budget }).unwrap();
    assert_eq!(corrupt.freshness, TuiFreshness::Unavailable);
    assert_eq!(
        corrupt_before,
        (snapshot_tree(&project), snapshot_tree(&data)),
        "corrupt projection-lag candidate must fail closed without mutation"
    );

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = std::fs::remove_dir_all(root);
}
