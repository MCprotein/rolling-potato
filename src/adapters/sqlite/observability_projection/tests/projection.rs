#[test]
fn csv_cell_quotes_only_when_needed() {
    assert_eq!(csv_cell("plain"), "plain");
    assert_eq!(csv_cell("a,b"), "\"a,b\"");
    assert_eq!(csv_cell("a\"b"), "\"a\"\"b\"");
}

#[test]
fn supplied_event_ordinal_avoids_a_canonical_ledger_rescan() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let identity = crate::app::workflow_adapter::ledger::fresh_identity();
    let event = crate::app::workflow_adapter::ledger::new_event_for(
        &identity,
        "performance.ordinal",
        "supplied ordinal projection",
        "safe=true",
    );
    let ledger = CountingLedgerReader {
        reads: Cell::new(0),
    };

    project_event_with_ordinal(&event, 1, &ledger).unwrap();

    assert_eq!(ledger.reads.get(), 0);
    assert_eq!(status_read_only().unwrap().ledger_events, 1);
}

#[test]
fn workflow_projection_uses_checkpoint_active_skill_id() {
    let connection = Connection::open_in_memory().unwrap();
    migrate(&connection).unwrap();

    project_workflow_checkpoint(
        &connection,
        "workflow.checkpoint",
        "workflow_id=workflow-skill phase=running active_skill_id=ralph skill_state=active",
        "session-test",
        42,
    )
    .unwrap();
    project_workflow_checkpoint(
        &connection,
        "workflow.checkpoint",
        "workflow_id=workflow-legacy phase=model-pending",
        "session-test",
        43,
    )
    .unwrap();

    let actual: Option<String> = connection
        .query_row(
            "SELECT active_skill_id FROM workflows WHERE workflow_id = 'workflow-skill'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let absent: Option<String> = connection
        .query_row(
            "SELECT active_skill_id FROM workflows WHERE workflow_id = 'workflow-legacy'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(actual.as_deref(), Some("ralph"));
    assert_eq!(absent, None);
}

#[test]
fn evidence_and_stop_gate_events_are_projected_as_rebuildable_truth_views() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-patch-projection-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let project_root = root.join("project");
    fs::create_dir_all(&project_root).unwrap();
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
    crate::test_support::initialize_runtime_state().unwrap();

    crate::test_support::record_runtime_event(
        "verification.evidence.recorded",
        "evidence",
        "workflow_id=workflow-test evidence_id=evidence-test artifact_hash=abc passed=true source_hash=def",
    )
    .unwrap();
    crate::test_support::record_runtime_event(
        "workflow.stop_gate.passed",
        "stop gate",
        "workflow_id=workflow-test proposal_id=proposal-test evidence_id=evidence-test applied_hash=def unresolved_approval=false",
    )
    .unwrap();
    let projected = projected_status();

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);
    assert_eq!(projected.evidence_records, 1);
    assert_eq!(projected.stop_gate_results, 1);
}
