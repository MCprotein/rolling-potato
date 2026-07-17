use super::*;

#[test]
fn bootstrap_creation_crash_matrix_is_idempotent() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    for point in [
        "after-journal",
        "after-artifacts",
        "after-ledger",
        "after-current",
        "after-projection",
    ] {
        let root = workflow_test_root(&format!("bootstrap-writer-{point}"));
        let project = root.join("project");
        fs::create_dir_all(&project).unwrap();
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
        std::env::set_var("RPOTATO_TEST_STATE_TRANSITION_FAULT", point);

        let error = initialize().unwrap_err();
        assert!(error.message.contains(point));
        std::env::remove_var("RPOTATO_TEST_STATE_TRANSITION_FAULT");
        let first = initialize().unwrap();
        let first_current = fs::read(paths::current_state_file()).unwrap();
        let first_events = ledger::read_runtime_events().unwrap();
        let second = initialize().unwrap();

        assert_eq!(first.identity.project_id, second.identity.project_id);
        assert_eq!(
            fs::read(paths::current_state_file()).unwrap(),
            first_current
        );
        assert_eq!(ledger::read_runtime_events().unwrap(), first_events);
        assert_eq!(
            first_events
                .iter()
                .filter(|event| event.event_type == "runtime.init")
                .count(),
            1,
            "fault point: {point}"
        );
        assert_eq!(current_state_lease_view().unwrap().revision, 1);

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        let _ = fs::remove_dir_all(root);
    }

    let root = workflow_test_root("bootstrap-writer-race");
    let project = root.join("project");
    fs::create_dir_all(&project).unwrap();
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
    let first = std::thread::spawn(initialize);
    let second = std::thread::spawn(initialize);
    first.join().unwrap().unwrap();
    second.join().unwrap().unwrap();
    let events = ledger::read_runtime_events().unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.event_type == "runtime.init")
            .count(),
        1
    );
    assert_eq!(current_state_lease_view().unwrap().revision, 1);
    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn session_new_crash_race_restart_is_single_commit() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    for point in [
        "after-journal",
        "after-artifacts",
        "after-ledger",
        "after-current",
        "after-projection",
    ] {
        with_workflow_env(&format!("session-new-writer-{point}"), |_| {
            let before = current_state_lease_view().unwrap();
            let before_events = ledger::read_runtime_events().unwrap();
            let intent_id = format!("intent-session-new-crash-{point}");
            std::env::set_var("RPOTATO_TEST_STATE_TRANSITION_FAULT", point);
            let error = session_new_report_for_intent(&intent_id).unwrap_err();
            assert!(error.message.contains(point));
            std::env::remove_var("RPOTATO_TEST_STATE_TRANSITION_FAULT");

            let first = session_new_report_for_intent(&intent_id).unwrap();
            let current = fs::read(paths::current_state_file()).unwrap();
            let events = ledger::read_runtime_events().unwrap();
            let retry = session_new_report_for_intent(&intent_id).unwrap();

            assert_eq!(first, retry, "fault point: {point}");
            assert_eq!(fs::read(paths::current_state_file()).unwrap(), current);
            assert_eq!(ledger::read_runtime_events().unwrap(), events);
            assert_eq!(
                current_state_lease_view().unwrap().revision,
                before.revision + 1
            );
            assert_eq!(events.len(), before_events.len() + 1);
            assert_eq!(
                events
                    .iter()
                    .filter(|event| {
                        event.event_type == "session.new"
                            && tui_detail_value(&event.details, "intent_id")
                                == Some(intent_id.as_str())
                    })
                    .count(),
                1
            );
        });
    }

    with_workflow_env("session-new-writer-race", |_| {
        let identity = ledger::validated_current_identity().unwrap();
        let transition = transition::TransitionGuard::acquire_for(
            &identity.project_id,
            transition::CurrentStateIntent::RecordEvent,
        )
        .unwrap();
        let first =
            std::thread::spawn(|| session_new_report_for_intent("intent-session-new-race-first"));
        let second =
            std::thread::spawn(|| session_new_report_for_intent("intent-session-new-race-second"));
        std::thread::sleep(Duration::from_millis(100));
        drop(transition);
        let results = [first.join().unwrap(), second.join().unwrap()];
        assert_eq!(
            results.iter().filter(|result| result.is_ok()).count(),
            1,
            "session new race results: {results:?}"
        );
        assert_eq!(
            results
                .iter()
                .filter(|result| result
                    .as_ref()
                    .is_err_and(|error| error.message.contains("stale predecessor")))
                .count(),
            1,
            "session new race results: {results:?}"
        );
        assert_eq!(
            ledger::read_runtime_events()
                .unwrap()
                .iter()
                .filter(|event| event.event_type == "session.new")
                .count(),
            1
        );
    });
}

#[test]
fn session_resume_transaction_never_exposes_current_before_ledger() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    for point in [
        "after-journal",
        "after-artifacts",
        "after-ledger",
        "after-current",
        "after-projection",
    ] {
        with_workflow_env(&format!("session-resume-writer-{point}"), |_| {
            let target = ledger::validated_current_identity().unwrap();
            session_new_report_for_intent(&format!("intent-session-new-before-{point}")).unwrap();
            let before_current = fs::read(paths::current_state_file()).unwrap();
            let intent_id = format!("intent-session-resume-crash-{point}");
            let lease = crate::tui::canonical_selection_lease(&target.session_id).unwrap();
            std::env::set_var("RPOTATO_TEST_STATE_TRANSITION_FAULT", point);

            let error =
                session_resume_report_for_tui(&target.session_id, &intent_id, &lease).unwrap_err();
            assert!(error.message.contains(point));
            std::env::remove_var("RPOTATO_TEST_STATE_TRANSITION_FAULT");
            let events_after_fault = ledger::read_runtime_events().unwrap();
            let event_is_durable = events_after_fault.iter().any(|event| {
                event.event_type == "session.resume.selected"
                    && tui_detail_value(&event.details, "intent_id") == Some(intent_id.as_str())
            });
            if !event_is_durable {
                assert_eq!(
                    fs::read(paths::current_state_file()).unwrap(),
                    before_current
                );
            }

            let first = session_resume_report_for_tui(&target.session_id, &intent_id, &lease)
                .unwrap()
                .unwrap();
            let committed_current = fs::read(paths::current_state_file()).unwrap();
            let committed_events = ledger::read_runtime_events().unwrap();
            let retry = session_resume_report_for_tui(&target.session_id, &intent_id, &lease)
                .unwrap()
                .unwrap();
            let snapshot = parse_current_state(
                std::str::from_utf8(&committed_current).unwrap(),
                "session resume committed current",
            )
            .unwrap();

            assert_eq!(first, retry);
            assert_eq!(snapshot.session_id, target.session_id);
            assert_eq!(
                fs::read(paths::current_state_file()).unwrap(),
                committed_current
            );
            assert_eq!(ledger::read_runtime_events().unwrap(), committed_events);
            assert_eq!(
                committed_events
                    .iter()
                    .filter(|event| {
                        event.event_type == "session.resume.selected"
                            && tui_detail_value(&event.details, "intent_id")
                                == Some(intent_id.as_str())
                    })
                    .count(),
                1
            );
        });
    }
}

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
            let backups = fs::read_dir(paths::state_dir())
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
