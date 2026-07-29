#[test]
fn low_level_writer_recovery_is_idempotent() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    for point in [
        "after-journal",
        "after-artifacts",
        "after-ledger",
        "after-current",
        "after-projection",
    ] {
        with_workflow_env(&format!("ordinary-state-transition-{point}"), |_| {
            let before_current = current_state_lease_view().unwrap();
            let before_events = ledger::read_runtime_events().unwrap();
            std::env::set_var("RPOTATO_TEST_STATE_TRANSITION_FAULT", point);

            let error = record_event(
                "test.state-transition.crash",
                "state transition crash matrix",
                &format!("point={point}"),
            )
            .unwrap_err();

            std::env::remove_var("RPOTATO_TEST_STATE_TRANSITION_FAULT");
            assert!(error.message.contains(point));
            let identity = ledger::validated_current_identity().unwrap();
            let journal_dir = paths::project_transition_journal_dir(&identity.project_id);
            assert_eq!(
                fs::read_dir(&journal_dir)
                    .unwrap()
                    .filter_map(Result::ok)
                    .filter(|entry| {
                        entry
                            .file_name()
                            .to_str()
                            .is_some_and(|name| name.ends_with(".prepared.json"))
                    })
                    .count(),
                1,
                "point: {point}"
            );

            assert_eq!(
                transition::recover_pending_source_bundles().unwrap(),
                1,
                "point: {point}"
            );
            let after_current = current_state_lease_view().unwrap();
            let after_events = ledger::read_runtime_events().unwrap();
            assert_eq!(after_current.revision, before_current.revision + 1);
            assert_eq!(after_events.len(), before_events.len() + 1);
            assert_eq!(
                after_events
                    .iter()
                    .filter(|event| event.event_type == "test.state-transition.crash")
                    .count(),
                1
            );
            assert_eq!(transition::recover_pending_source_bundles().unwrap(), 0);
            assert_eq!(current_state_lease_view().unwrap(), after_current);
            assert_eq!(ledger::read_runtime_events().unwrap(), after_events);
        });
    }
}
#[test]
fn workflow_checkpoint_writer_crash_matrix() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    for point in [
        "after-transaction",
        "after-snapshot",
        "after-ledger",
        "after-pointer",
    ] {
        with_workflow_env(point, |_| {
            std::env::set_var("RPOTATO_TEST_CHECKPOINT_FAULT", point);
            let error = create_workflow("recover me").unwrap_err();
            assert!(
                error.message.contains("injected checkpoint fault"),
                "fault point {point}: {}",
                error.message
            );
            std::env::remove_var("RPOTATO_TEST_CHECKPOINT_FAULT");

            let workflow_id = active_workflow_id().unwrap().unwrap();
            let workflow = load_workflow(&workflow_id).unwrap();
            let checkpoints = ledger::workflow_checkpoints(&workflow_id).unwrap();
            let pointer = fs::read(paths::project_workflow_file(&workflow_id)).unwrap();
            let current = fs::read(paths::current_state_file()).unwrap();
            let events = ledger::read_runtime_events().unwrap();
            assert_eq!(workflow.revision, 1, "fault point: {point}");
            assert_eq!(checkpoints.len(), 1, "fault point: {point}");
            assert!(!paths::project_workflow_transaction_file(&workflow_id).exists());
            assert_eq!(active_workflow_id().unwrap(), Some(workflow_id.clone()));
            assert_eq!(load_workflow(&workflow_id).unwrap(), workflow);
            assert_eq!(
                fs::read(paths::project_workflow_file(&workflow_id)).unwrap(),
                pointer
            );
            assert_eq!(fs::read(paths::current_state_file()).unwrap(), current);
            assert_eq!(ledger::read_runtime_events().unwrap(), events);
        });
    }
}

#[test]
fn workflow_recovery_replays_only_prepared_suffix() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    for point in [
        "after-transaction",
        "after-snapshot",
        "after-ledger",
        "after-pointer",
    ] {
        with_workflow_env(&format!("workflow-replay-{point}"), |_| {
            let first = create_workflow("prepared suffix replay").unwrap();
            let mut next = first.clone();
            next.result_summary = format!("prepared-{point}");
            std::env::set_var("RPOTATO_TEST_CHECKPOINT_FAULT", point);
            let error = checkpoint_workflow(next, first.revision).unwrap_err();
            assert!(error.message.contains(point));
            std::env::remove_var("RPOTATO_TEST_CHECKPOINT_FAULT");

            let recovered = load_workflow(&first.workflow_id).unwrap();
            let pointer = fs::read(paths::project_workflow_file(&first.workflow_id)).unwrap();
            let snapshot = fs::read(paths::project_workflow_snapshot_file(
                &first.workflow_id,
                recovered.revision,
            ))
            .unwrap();
            let events = ledger::read_runtime_events().unwrap();
            assert_eq!(recovered.revision, 2);
            assert_eq!(recovered.result_summary, format!("prepared-{point}"));
            assert_eq!(load_workflow(&first.workflow_id).unwrap(), recovered);
            assert_eq!(
                fs::read(paths::project_workflow_file(&first.workflow_id)).unwrap(),
                pointer
            );
            assert_eq!(
                fs::read(paths::project_workflow_snapshot_file(
                    &first.workflow_id,
                    recovered.revision
                ))
                .unwrap(),
                snapshot
            );
            assert_eq!(ledger::read_runtime_events().unwrap(), events);
            assert!(!paths::project_workflow_transaction_file(&first.workflow_id).exists());
        });
    }

    with_workflow_env("workflow-replay-tamper", |_| {
        let first = create_workflow("tampered prepared suffix").unwrap();
        let mut next = first.clone();
        next.result_summary = "must-not-install".to_string();
        std::env::set_var("RPOTATO_TEST_CHECKPOINT_FAULT", "after-transaction");
        checkpoint_workflow(next, first.revision).unwrap_err();
        std::env::remove_var("RPOTATO_TEST_CHECKPOINT_FAULT");
        let identity = ledger::validated_current_identity().unwrap();
        let journal_dir = paths::project_transition_journal_dir(&identity.project_id);
        let journal = fs::read_dir(&journal_dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| {
                path.file_name()
                    .and_then(|value| value.to_str())
                    .is_some_and(|name| name.ends_with(".prepared.json"))
            })
            .unwrap();
        let mut bytes = fs::read(&journal).unwrap();
        let index = bytes.len() / 2;
        bytes[index] ^= 1;
        fs::write(&journal, &bytes).unwrap();
        let before_events = ledger::read_runtime_events().unwrap();
        let pointer = fs::read(paths::project_workflow_file(&first.workflow_id)).unwrap();

        assert!(load_workflow(&first.workflow_id).is_err());
        assert_eq!(ledger::read_runtime_events().unwrap(), before_events);
        assert_eq!(
            fs::read(paths::project_workflow_file(&first.workflow_id)).unwrap(),
            pointer
        );
        assert!(journal.exists());
    });
}
