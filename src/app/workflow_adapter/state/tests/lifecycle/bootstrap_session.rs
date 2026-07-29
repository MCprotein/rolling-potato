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
            let lease =
                crate::app::tui_adapter::canonical_selection_lease(&target.session_id).unwrap();
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
