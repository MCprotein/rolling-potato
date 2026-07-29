#[test]
fn active_workflow_pointer_recovery_is_single_and_idempotent() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    for point in [
        "after-journal",
        "after-artifacts",
        "after-ledger",
        "after-current",
        "after-projection",
    ] {
        with_workflow_env(&format!("active-pointer-recovery-{point}"), |_| {
            let workflow = create_workflow("recover active pointer").unwrap();
            let current_path = paths::current_state_file();
            let body = fs::read_to_string(&current_path).unwrap();
            let mut detached = parse_current_state(&body, "detached active pointer").unwrap();
            detached.active_workflow = None;
            detached.artifact_hash = sha256_text(&render_current_state_v2_payload(&detached));
            fs::write(&current_path, render_current_state_v2(&detached)).unwrap();
            std::env::set_var("RPOTATO_TEST_STATE_TRANSITION_FAULT", point);

            let error = active_workflow_id().unwrap_err();
            assert!(error.message.contains(point));
            std::env::remove_var("RPOTATO_TEST_STATE_TRANSITION_FAULT");
            assert_eq!(
                active_workflow_id().unwrap(),
                Some(workflow.workflow_id.clone())
            );
            let current = fs::read(&current_path).unwrap();
            let events = ledger::read_runtime_events().unwrap();
            assert_eq!(
                active_workflow_id().unwrap(),
                Some(workflow.workflow_id.clone())
            );
            assert_eq!(fs::read(&current_path).unwrap(), current);
            assert_eq!(ledger::read_runtime_events().unwrap(), events);
            assert_eq!(
                events
                    .iter()
                    .filter(|event| event.event_type == "workflow.pointer.recovered")
                    .count(),
                1
            );
        });
    }

    with_workflow_env("active-pointer-recovery-zero", |_| {
        let before = ledger::read_runtime_events().unwrap();
        assert_eq!(active_workflow_id().unwrap(), None);
        assert_eq!(ledger::read_runtime_events().unwrap(), before);
    });
}

#[test]
fn terminal_pointer_cleanup_crash_race_restart_is_idempotent() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    for point in [
        "after-journal",
        "after-artifacts",
        "after-ledger",
        "after-current",
        "after-projection",
    ] {
        with_workflow_env(&format!("terminal-cleanup-{point}"), |_| {
            let first = create_workflow("terminal cleanup").unwrap();
            let mut terminal = first.clone();
            terminal.phase = "cancelled".to_string();
            terminal.failure_reason = "cancelled-before-side-effect".to_string();
            let terminal = checkpoint_workflow(terminal, first.revision).unwrap();
            std::env::set_var("RPOTATO_TEST_STATE_TRANSITION_FAULT", point);

            let error = clear_terminal_workflow_pointer(&terminal).unwrap_err();
            assert!(error.message.contains(point));
            std::env::remove_var("RPOTATO_TEST_STATE_TRANSITION_FAULT");
            clear_terminal_workflow_pointer(&terminal).unwrap();
            let current = fs::read(paths::current_state_file()).unwrap();
            let events = ledger::read_runtime_events().unwrap();
            clear_terminal_workflow_pointer(&terminal).unwrap();
            let snapshot = parse_current_state(
                std::str::from_utf8(&current).unwrap(),
                "terminal cleanup committed current",
            )
            .unwrap();

            assert!(snapshot.active_workflow.is_none());
            assert_eq!(fs::read(paths::current_state_file()).unwrap(), current);
            assert_eq!(ledger::read_runtime_events().unwrap(), events);
            assert_eq!(
                events
                    .iter()
                    .filter(|event| event.event_type == "workflow.pointer.cleared")
                    .count(),
                1
            );
            assert!(clear_terminal_workflow_pointer(&first).is_err());
        });
    }

    with_workflow_env("terminal-cleanup-race", |_| {
        let first = create_workflow("terminal cleanup race").unwrap();
        let mut terminal = first.clone();
        terminal.phase = "cancelled".to_string();
        terminal.failure_reason = "cancelled-before-side-effect".to_string();
        let terminal = checkpoint_workflow(terminal, first.revision).unwrap();
        let identity = ledger::validated_current_identity().unwrap();
        let transition = transition::TransitionGuard::acquire_for(
            &identity.project_id,
            transition::CurrentStateIntent::RecordEvent,
        )
        .unwrap();
        let cleanup = std::thread::spawn(move || clear_terminal_workflow_pointer(&terminal));
        let create = std::thread::spawn(|| create_workflow("new workflow after terminal"));
        std::thread::sleep(Duration::from_millis(100));
        drop(transition);
        let cleanup_result = cleanup.join().unwrap();
        let created = create.join().unwrap().unwrap();
        let active = active_workflow_id().unwrap();
        assert_eq!(active, Some(created.workflow_id));
        if let Err(error) = cleanup_result {
            assert!(error.message.contains("pointer conflict"));
        }
    });

    with_workflow_env("stale-terminal-pointer", |_| {
        let first = create_workflow("stale terminal pointer").unwrap();
        let mut failed = first.clone();
        failed.phase = "failed".to_string();
        failed.failure_reason = "backend-call-failed".to_string();
        checkpoint_workflow(failed, first.revision).unwrap();

        assert_eq!(active_workflow_id().unwrap(), None);
        let events = ledger::read_runtime_events().unwrap();
        assert_eq!(active_workflow_id().unwrap(), None);
        assert_eq!(ledger::read_runtime_events().unwrap(), events);
        assert_eq!(
            events
                .iter()
                .filter(|event| event.event_type == "workflow.pointer.cleared")
                .count(),
            1
        );
    });
}

#[test]
fn reconcile_writer_crash_matrix_preserves_evidence() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    for point in [
        "after-journal",
        "after-artifacts",
        "after-ledger",
        "after-current",
        "after-projection",
    ] {
        with_workflow_env(&format!("reconcile-writer-{point}"), |_| {
            let corrupt = format!("corrupt-current-evidence-{point}\n");
            fs::write(paths::current_state_file(), &corrupt).unwrap();
            std::env::set_var("RPOTATO_TEST_STATE_TRANSITION_FAULT", point);

            let error = reconcile_report().unwrap_err();
            assert!(error.message.contains(point));
            std::env::remove_var("RPOTATO_TEST_STATE_TRANSITION_FAULT");
            reconcile_report().unwrap();
            let current = fs::read(paths::current_state_file()).unwrap();
            let events = ledger::read_runtime_events().unwrap();
            let backups = fs::read_dir(paths::current_state_dir())
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry
                        .file_name()
                        .to_str()
                        .is_some_and(|name| name.starts_with("current-state.json.corrupt."))
                })
                .collect::<Vec<_>>();

            assert_eq!(backups.len(), 1, "fault point: {point}");
            assert_eq!(fs::read_to_string(backups[0].path()).unwrap(), corrupt);
            assert_eq!(
                events
                    .iter()
                    .filter(|event| event.event_type == "state.reconcile.corrupt_recovered")
                    .count(),
                1
            );
            reconcile_report().unwrap();
            assert_eq!(fs::read(paths::current_state_file()).unwrap(), current);
            assert_eq!(ledger::read_runtime_events().unwrap(), events);
        });
    }

    let root = workflow_test_root("reconcile-writer-missing");
    let project = root.join("project");
    fs::create_dir_all(&project).unwrap();
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
    let first = reconcile_report().unwrap();
    let current = fs::read(paths::current_state_file()).unwrap();
    let events = ledger::read_runtime_events().unwrap();
    let second = reconcile_report().unwrap();
    assert!(first.contains("created"));
    assert!(second.contains("current-state 정상"));
    assert_eq!(fs::read(paths::current_state_file()).unwrap(), current);
    assert_eq!(ledger::read_runtime_events().unwrap(), events);
    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);
}
