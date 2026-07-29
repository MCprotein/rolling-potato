#[test]
fn sqlite_only_session_is_removed_and_cannot_resume() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    with_workflow_env("sqlite-session-authority", |_| {
        let identity = ledger::validated_current_identity().unwrap();
        let connection = rusqlite::Connection::open(paths::observability_db_file()).unwrap();
        connection
                .execute(
                    "INSERT INTO sessions (session_id, project_id, project_root, started_at_ms) VALUES (?1, ?2, ?3, 1)",
                    rusqlite::params!["session-sqlite-only", identity.project_id, identity.project_root],
                )
                .unwrap();
        drop(connection);

        let sessions = observability::session_history(20).unwrap();
        assert!(sessions
            .iter()
            .all(|session| session.session_id != "session-sqlite-only"));
        let error = session_resume_report("session-sqlite-only").unwrap_err();
        assert_eq!(error.code, 3);
        assert!(error.message.contains("canonical runtime ledger"));

        let connection = rusqlite::Connection::open(paths::observability_db_file()).unwrap();
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE session_id = 'session-sqlite-only'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    });
}

#[test]
fn session_list_does_not_create_current_state_when_history_is_empty() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-session-list-empty-test-{}",
        std::process::id()
    ));
    let project_root = root.join("project");
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);

    let report = session_list_report().unwrap();
    let current_state_exists = paths::current_state_file().exists();

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");

    assert!(report.contains("sessions: 없음"));
    assert!(!current_state_exists);
}

#[test]
fn session_resume_selects_existing_history_entry() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-session-resume-test-{}",
        std::process::id()
    ));
    let project_root = root.join("project");
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);

    let new_report = session_new_report().unwrap();
    let session_id = new_report
        .lines()
        .find_map(|line| line.strip_prefix("- session id: "))
        .unwrap()
        .to_string();
    let list_report = session_list_report().unwrap();
    let resume_report = session_resume_report(&session_id).unwrap();
    let current_state = fs::read_to_string(paths::current_state_file()).unwrap();

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");

    assert!(list_report.contains(&session_id));
    assert!(resume_report.contains("session resume 결과"));
    assert!(current_state.contains(&format!("\"session_id\":\"{session_id}\"")));
    assert!(current_state.contains("\"resume_source\":\"session-history\""));
}

#[test]
fn tui_session_selection_revalidates_lease_under_lock_and_reuses_receipt() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    with_workflow_env("tui-session-selection-lease", |_| {
        let initial = ledger::validated_current_identity().unwrap();
        session_new_report().unwrap();
        let intent_id = "intent-session-select-exact-0001";
        let lease =
            crate::app::tui_adapter::canonical_selection_lease(&initial.session_id).unwrap();

        let first = session_resume_report_for_tui(&initial.session_id, intent_id, &lease)
            .unwrap()
            .unwrap();
        let after_first = fs::read_to_string(paths::current_state_file()).unwrap();
        let events_after_first = ledger::read_runtime_events().unwrap();
        let first_receipts = events_after_first
            .iter()
            .filter(|event| {
                event.event_type == "session.resume.selected"
                    && event.details.contains(&format!("intent_id={intent_id}"))
            })
            .count();

        let retry = session_resume_report_for_tui(&initial.session_id, intent_id, &lease)
            .unwrap()
            .unwrap();
        let after_retry = fs::read_to_string(paths::current_state_file()).unwrap();
        let retry_receipts = ledger::read_runtime_events()
            .unwrap()
            .into_iter()
            .filter(|event| {
                event.event_type == "session.resume.selected"
                    && event.details.contains(&format!("intent_id={intent_id}"))
            })
            .count();

        assert_eq!(first, retry);
        assert_eq!(after_first, after_retry);
        assert_eq!(first_receipts, 1);
        assert_eq!(retry_receipts, 1);

        let stale_lease =
            crate::app::tui_adapter::canonical_selection_lease(&initial.session_id).unwrap();
        record_event("test.selection.predecessor", "advance predecessor", "safe").unwrap();
        let before_stale_events = ledger::read_runtime_events().unwrap().len();
        assert!(session_resume_report_for_tui(
            &initial.session_id,
            "intent-session-select-stale-0002",
            &stale_lease,
        )
        .unwrap()
        .is_none());
        assert_eq!(
            ledger::read_runtime_events().unwrap().len(),
            before_stale_events
        );
    });
}
