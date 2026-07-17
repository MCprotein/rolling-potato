use super::*;

#[test]
fn deny_pending_patch_is_idempotent_and_returns_stored_receipt_first() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("deny-patch-idempotent");
    let (target, workflow, _proposal) = create_pending_workflow(&root, "pwd");

    let first = deny_pending_gate(&workflow.workflow_id, "intent-outcome-0001").unwrap();
    let ledger_after_first = fs::read_to_string(paths::runtime_ledger_file()).unwrap();
    let retry = deny_pending_gate(&workflow.workflow_id, "intent-outcome-0001").unwrap();
    let ledger_after_retry = fs::read_to_string(paths::runtime_ledger_file()).unwrap();
    let cancelled = state::load_workflow(&workflow.workflow_id).unwrap();
    let source = fs::read_to_string(&target).unwrap();
    clear_patch_test_env(&root);

    assert_eq!(first.status, TuiOutcomeStatus::Succeeded);
    assert_eq!(first.code, TuiOutcomeCode::DenyPatchAccepted);
    assert_eq!(first.effect, TuiEffect::Committed);
    assert_eq!(first.safe_message, retry.safe_message);
    assert_eq!(ledger_after_first, ledger_after_retry);
    assert_eq!(cancelled.phase, "cancelled");
    assert_eq!(cancelled.failure_reason, "user-denied-patch");
    assert_eq!(cancelled.approval_state, "denied");
    assert_eq!(cancelled.verification_approval_state, "not-issued");
    assert_eq!(source, "pub const X: i32 = 1;\n");
}

#[test]
fn denial_retry_requires_exact_intent_field_not_substring_match() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("deny-exact-intent-retry");
    let (_target, workflow, _proposal) = create_pending_workflow(&root, "pwd");

    deny_pending_gate(&workflow.workflow_id, "intent-deny-10").unwrap();
    let events_after_commit = ledger::read_runtime_events().unwrap();
    let conflict = deny_pending_gate(&workflow.workflow_id, "intent-deny-1").unwrap();

    assert_eq!(conflict.status, TuiOutcomeStatus::Blocked);
    assert_eq!(conflict.code, TuiOutcomeCode::DenyBlockedTerminalState);
    assert_eq!(conflict.effect, TuiEffect::NotDispatched);
    assert_eq!(ledger::read_runtime_events().unwrap(), events_after_commit);
    clear_patch_test_env(&root);
}

#[test]
fn tui_workflow_resume_revalidates_lease_and_persists_exact_intent_receipt() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("tui-resume-transaction");
    let (_target, workflow, _proposal) = create_pending_workflow(&root, "pwd");
    let lease = crate::tui::canonical_selection_lease(&workflow.workflow_id).unwrap();
    let intent_id = "intent-tui-resume-exact";

    resume_workflow_for_tui(&workflow.workflow_id, intent_id, &lease).unwrap();
    let events_after_commit = ledger::read_runtime_events().unwrap();
    assert!(ledger::event_details_match(
        "workflow.resume.accepted",
        &[
            ("intent_id", intent_id),
            ("workflow_id", workflow.workflow_id.as_str())
        ],
    )
    .unwrap());

    resume_workflow_for_tui(&workflow.workflow_id, intent_id, &lease).unwrap();
    assert_eq!(ledger::read_runtime_events().unwrap(), events_after_commit);

    let error = resume_workflow_for_tui(&workflow.workflow_id, "intent-tui-resume-stale", &lease)
        .unwrap_err();
    assert!(is_stale_selection_error(&error));
    assert_eq!(ledger::read_runtime_events().unwrap(), events_after_commit);
    clear_patch_test_env(&root);
}

#[test]
fn tui_approval_rejects_current_lease_selected_for_a_different_object() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("tui-approval-selected-object");
    let (target, workflow, proposal) = create_pending_workflow(&root, "pwd");
    let wrong_lease = crate::tui::canonical_selection_lease("workflow-unrelated").unwrap();
    let before_events = ledger::read_runtime_events().unwrap();
    let before_workflow = state::load_workflow(&workflow.workflow_id).unwrap();

    let error = match approve_for_tui(
        &proposal.proposal_id,
        &proposal.approval_token,
        "intent-tui-wrong-selected-object",
        &wrong_lease,
    ) {
        Ok(_) => panic!("wrong selected object approved a proposal"),
        Err(error) => error,
    };

    assert!(is_stale_selection_error(&error));
    assert_eq!(ledger::read_runtime_events().unwrap(), before_events);
    assert_eq!(
        state::load_workflow(&workflow.workflow_id).unwrap(),
        before_workflow
    );
    assert_eq!(
        fs::read_to_string(target).unwrap(),
        "pub const X: i32 = 1;\n"
    );
    clear_patch_test_env(&root);
}

#[test]
fn resume_entrypoints_block_tampered_and_oversized_pending_approval_proposals() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    for case in ["tampered", "oversized"] {
        let root = patch_test_root(&format!("tui-resume-proposal-{case}"));
        let (_target, workflow, proposal) = create_pending_workflow(&root, "pwd");
        let lease = crate::tui::canonical_selection_lease(&workflow.workflow_id).unwrap();
        let path =
            paths::project_patch_proposals_dir().join(format!("{}.txt", proposal.proposal_id));
        let original = fs::read_to_string(&path).unwrap();
        if case == "tampered" {
            fs::write(
                &path,
                original.replacen(
                    &format!("workflow_id={}", workflow.workflow_id),
                    "workflow_id=workflow-unrelated",
                    1,
                ),
            )
            .unwrap();
        } else {
            let mut oversized = original.into_bytes();
            oversized.resize(MAX_PROPOSAL_RECORD_BYTES + 1, b'x');
            fs::write(&path, oversized).unwrap();
        }
        let before_events = ledger::read_runtime_events().unwrap();
        let direct_error = resume_workflow_report(&workflow.workflow_id).unwrap_err();
        assert!(
            direct_error.message.contains("byte budget 초과")
                || direct_error.message.contains("binding이 일치하지 않습니다")
        );
        assert_eq!(ledger::read_runtime_events().unwrap(), before_events);
        let error = resume_workflow_for_tui(
            &workflow.workflow_id,
            &format!("intent-tui-resume-{case}"),
            &lease,
        )
        .unwrap_err();
        assert!(!is_stale_selection_error(&error) || case == "tampered");
        assert_eq!(ledger::read_runtime_events().unwrap(), before_events);
        clear_patch_test_env(&root);
    }
}

#[test]
fn tui_resume_revalidates_proposal_during_pending_verification_approval() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("tui-resume-verification-proposal-binding");
    let (_target, workflow, proposal) = create_pending_workflow(&root, "pwd");
    approve_report(&proposal.proposal_id, &proposal.approval_token, false, None).unwrap();
    let current = state::load_workflow(&workflow.workflow_id).unwrap();
    assert_eq!(current.phase, "pending-verification-approval");
    let lease = crate::tui::canonical_selection_lease(&workflow.workflow_id).unwrap();
    let path = paths::project_patch_proposals_dir().join(format!("{}.txt", proposal.proposal_id));
    let tampered = fs::read_to_string(&path).unwrap().replacen(
        &format!("workflow_id={}", workflow.workflow_id),
        "workflow_id=workflow-unrelated",
        1,
    );
    fs::write(&path, tampered).unwrap();
    let before_events = ledger::read_runtime_events().unwrap();

    assert!(resume_workflow_for_tui(
        &workflow.workflow_id,
        "intent-tui-resume-verification-tamper",
        &lease,
    )
    .is_err());
    assert_eq!(ledger::read_runtime_events().unwrap(), before_events);
    clear_patch_test_env(&root);
}

#[test]
fn terminal_denial_crash_matrix_recovers_one_exact_commit() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    for point in [
        "A1-after-journal",
        "A2-after-intent",
        "A3-after-source",
        "A4-after-snapshot",
        "A5-after-pointer",
        "A6-after-ledger",
        "A7-after-current",
        "A8-after-projection",
    ] {
        let root = patch_test_root(&format!("terminal-denial-{point}"));
        let (target, workflow, _proposal) = create_pending_workflow(&root, "pwd");
        let before_events = ledger::read_runtime_events().unwrap().len();
        let before_current = state::current_state_lease_view().unwrap().revision;
        let before_workflow = workflow.revision;
        std::env::set_var("RPOTATO_TEST_TERMINAL_ACTION_FAULT", point);
        let error = match deny_pending_gate(&workflow.workflow_id, "intent-terminal-crash") {
            Ok(_) => panic!("fault must interrupt terminal transaction"),
            Err(error) => error,
        };
        assert!(error.message.contains(point));
        std::env::remove_var("RPOTATO_TEST_TERMINAL_ACTION_FAULT");

        assert_eq!(transition::recover_pending_source_bundles().unwrap(), 1);
        let terminal = state::load_workflow(&workflow.workflow_id).unwrap();
        assert_eq!(terminal.phase, "cancelled", "point: {point}");
        assert_eq!(terminal.failure_reason, "user-denied-patch");
        assert_eq!(terminal.revision, before_workflow + 1);
        assert_eq!(
            state::current_state_lease_view().unwrap().revision,
            before_current + 1
        );
        assert_eq!(
            ledger::read_runtime_events().unwrap().len(),
            before_events + 3
        );
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "pub const X: i32 = 1;\n"
        );
        let after_events = ledger::read_runtime_events().unwrap();
        let after_current = fs::read(paths::current_state_file()).unwrap();
        assert_eq!(transition::recover_pending_source_bundles().unwrap(), 0);
        assert_eq!(ledger::read_runtime_events().unwrap(), after_events);
        assert_eq!(
            fs::read(paths::current_state_file()).unwrap(),
            after_current
        );
        clear_patch_test_env(&root);
    }
}

#[test]
fn deny_pending_verification_rolls_back_exact_source_once() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("deny-verification");
    let (target, workflow, proposal) = create_pending_workflow(&root, "pwd");
    approve_report(&proposal.proposal_id, &proposal.approval_token, false, None).unwrap();

    let outcome = deny_pending_gate(&workflow.workflow_id, "intent-outcome-0001").unwrap();
    let cancelled = state::load_workflow(&workflow.workflow_id).unwrap();
    let source = fs::read_to_string(&target).unwrap();
    clear_patch_test_env(&root);

    assert_eq!(outcome.status, TuiOutcomeStatus::Succeeded);
    assert_eq!(outcome.code, TuiOutcomeCode::DenyVerificationRolledBack);
    assert_eq!(outcome.effect, TuiEffect::RolledBack);
    assert_eq!(cancelled.phase, "cancelled");
    assert_eq!(source, "pub const X: i32 = 1;\n");
}

#[test]
fn deny_non_pending_and_terminal_phases_do_not_mutate_workflow() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("deny-phase-blocks");
    let (_target, mut workflow, _proposal) = create_pending_workflow(&root, "pwd");
    workflow.phase = "approved".to_string();
    workflow.approval_state = "approved".to_string();
    workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();
    let approved_revision = workflow.revision;

    let not_pending = deny_pending_gate(&workflow.workflow_id, "intent-outcome-0001").unwrap();
    let after_not_pending = state::load_workflow(&workflow.workflow_id).unwrap();
    cancel_workflow_report(&workflow.workflow_id).unwrap();
    let terminal_before = state::load_workflow(&workflow.workflow_id).unwrap();
    let terminal = deny_pending_gate(&workflow.workflow_id, "intent-outcome-0002").unwrap();
    let terminal_after = state::load_workflow(&workflow.workflow_id).unwrap();
    clear_patch_test_env(&root);

    assert_eq!(not_pending.code, TuiOutcomeCode::DenyBlockedNotPending);
    assert_eq!(not_pending.effect, TuiEffect::NotDispatched);
    assert_eq!(after_not_pending.revision, approved_revision);
    assert_eq!(terminal.code, TuiOutcomeCode::DenyBlockedTerminalState);
    assert_eq!(terminal.effect, TuiEffect::NotDispatched);
    assert_eq!(terminal_before, terminal_after);
}

#[test]
fn approved_checkpoint_can_be_cancelled_before_apply_without_rollback_record() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("approved-before-apply-cancel");
    let (target, mut workflow, _proposal) = create_pending_workflow(&root, "pwd");
    workflow.phase = "approved".to_string();
    workflow.approval_state = "approved".to_string();
    workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();

    let report = cancel_workflow_report(&workflow.workflow_id).unwrap();
    let cancelled = state::load_workflow(&workflow.workflow_id).unwrap();
    let source = fs::read_to_string(&target).unwrap();
    clear_patch_test_env(&root);

    assert!(report.contains("workflow 취소 완료"));
    assert_eq!(cancelled.phase, "cancelled");
    assert_eq!(source, "pub const X: i32 = 1;\n");
}

#[test]
fn approve_reloads_cancelled_workflow_after_prelock_race() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("approve-cancel-race");
    let (target, mut workflow, proposal) = create_pending_workflow(&root, "pwd");
    workflow.phase = "approved".to_string();
    workflow.approval_state = "approved".to_string();
    workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();
    let barrier = root.join("approve-prelock");
    std::env::set_var("RPOTATO_TEST_APPROVAL_PRELOCK_BARRIER", &barrier);
    let proposal_id = proposal.proposal_id.clone();
    let token = proposal.approval_token.clone();
    let approve =
        std::thread::spawn(move || approve_report(&proposal_id, &token, false, None).unwrap_err());
    let ready = PathBuf::from(format!("{}.ready", barrier.display()));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !ready.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "approve prelock barrier timeout"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    let cancelled_report = cancel_workflow_report(&workflow.workflow_id).unwrap();
    fs::write(
        PathBuf::from(format!("{}.release", barrier.display())),
        b"release",
    )
    .unwrap();
    let approve_error = approve.join().unwrap();
    std::env::remove_var("RPOTATO_TEST_APPROVAL_PRELOCK_BARRIER");
    let cancelled = state::load_workflow(&workflow.workflow_id).unwrap();
    let source = fs::read_to_string(&target).unwrap();
    clear_patch_test_env(&root);

    assert!(cancelled_report.contains("workflow 취소 완료"));
    assert_eq!(approve_error.code, 3);
    assert!(approve_error.message.contains("phase: cancelled"));
    assert_eq!(cancelled.phase, "cancelled");
    assert_eq!(source, "pub const X: i32 = 1;\n");
}

#[test]
fn cancel_is_idempotent_after_source_was_already_restored() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("already-restored-cancel");
    let (target, workflow, proposal) = create_pending_workflow(&root, "pwd");
    let approval =
        approve_report(&proposal.proposal_id, &proposal.approval_token, false, None).unwrap();
    assert!(approval.contains("applied-awaiting-verification"));
    fs::write(&target, "pub const X: i32 = 1;\n").unwrap();
    let record = load_proposal_record(
        &proposal.proposal_id,
        &paths::project_patch_proposals_dir().join(format!("{}.txt", proposal.proposal_id)),
    )
    .unwrap();
    let rollback_path = rollback_path_for_record(&record).unwrap();
    fs::remove_file(rollback_path).unwrap();

    let report = cancel_workflow_report(&workflow.workflow_id).unwrap();
    let cancelled = state::load_workflow(&workflow.workflow_id).unwrap();
    let source = fs::read_to_string(&target).unwrap();
    clear_patch_test_env(&root);

    assert!(report.contains("workflow 취소 완료"));
    assert_eq!(cancelled.phase, "cancelled");
    assert_eq!(source, "pub const X: i32 = 1;\n");
}

#[test]
fn source_replace_fault_windows_recover_committed_prepared_bytes() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    for point in ["after-guard", "after-install"] {
        let root = patch_test_root(&format!("source-fault-{point}"));
        let (target, _workflow, proposal) = create_pending_workflow(&root, "pwd");
        std::env::set_var("RPOTATO_TEST_SOURCE_REPLACE_FAULT", point);
        let error = approve_report(&proposal.proposal_id, &proposal.approval_token, false, None)
            .unwrap_err();
        std::env::remove_var("RPOTATO_TEST_SOURCE_REPLACE_FAULT");
        let repair_required = transition::recover_pending_source_bundles().unwrap_err();
        assert!(
            repair_required
                .message
                .contains("projection.repair-required"),
            "point: {point}, error: {}",
            repair_required.message
        );
        assert_eq!(transition::recover_pending_source_bundles().unwrap(), 1);
        let source = fs::read_to_string(&target).unwrap();
        clear_patch_test_env(&root);
        assert!(matches!(error.code, 1 | 3), "point: {point}");
        assert_eq!(source, "pub const X: i32 = 2;\n", "point: {point}");
    }
}

#[cfg(unix)]
#[test]
fn source_recovery_rejects_parent_symlink_replacement_before_any_event() {
    use std::os::unix::fs::symlink;

    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("source-parent-symlink-race");
    let (target, workflow, proposal) = create_pending_workflow(&root, "pwd");
    let before_events = ledger::read_runtime_events().unwrap();
    std::env::set_var("RPOTATO_TEST_APPROVAL_TRANSACTION_FAULT", "T1");
    approve_report(&proposal.proposal_id, &proposal.approval_token, false, None).unwrap_err();
    std::env::remove_var("RPOTATO_TEST_APPROVAL_TRANSACTION_FAULT");

    let original_parent = target.parent().unwrap().with_file_name("src-original");
    fs::rename(target.parent().unwrap(), &original_parent).unwrap();
    let outside = root.join("outside");
    fs::create_dir_all(&outside).unwrap();
    let outside_target = outside.join("lib.rs");
    fs::write(&outside_target, "outside sentinel\n").unwrap();
    symlink(&outside, target.parent().unwrap()).unwrap();

    let error = transition::recover_pending_source_bundles().unwrap_err();

    assert!(error.message.contains("parent traversal"));
    assert_eq!(
        fs::read_to_string(&outside_target).unwrap(),
        "outside sentinel\n"
    );
    assert_eq!(
        fs::read_to_string(original_parent.join("lib.rs")).unwrap(),
        "pub const X: i32 = 1;\n"
    );
    assert_eq!(ledger::read_runtime_events().unwrap(), before_events);
    assert!(paths::project_transition_journal_file(
        &workflow.project_id,
        &format!("intent-approve-{}", proposal.proposal_id),
    )
    .exists());
    clear_patch_test_env(&root);
}

#[cfg(unix)]
#[test]
fn source_recovery_rejects_rollback_parent_symlink_before_any_event() {
    use std::os::unix::fs::symlink;

    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("rollback-parent-symlink-race");
    let (target, workflow, proposal) = create_pending_workflow(&root, "pwd");
    let record = load_proposal_record(
        &proposal.proposal_id,
        &paths::project_patch_proposals_dir().join(format!("{}.txt", proposal.proposal_id)),
    )
    .unwrap();
    let rollback = rollback_path_for_record(&record).unwrap();
    let before_events = ledger::read_runtime_events().unwrap();
    std::env::set_var("RPOTATO_TEST_APPROVAL_TRANSACTION_FAULT", "T1");
    approve_report(&proposal.proposal_id, &proposal.approval_token, false, None).unwrap_err();
    std::env::remove_var("RPOTATO_TEST_APPROVAL_TRANSACTION_FAULT");

    let outside = root.join("outside-rollback");
    fs::create_dir_all(&outside).unwrap();
    let outside_sentinel = outside.join("sentinel.txt");
    fs::write(&outside_sentinel, "outside sentinel\n").unwrap();
    fs::create_dir_all(rollback.parent().unwrap().parent().unwrap()).unwrap();
    symlink(&outside, rollback.parent().unwrap()).unwrap();

    let error = transition::recover_pending_source_bundles().unwrap_err();

    assert!(error.message.contains("rollback parent traversal"));
    assert_eq!(
        fs::read_to_string(&outside_sentinel).unwrap(),
        "outside sentinel\n"
    );
    assert!(!outside.join(rollback.file_name().unwrap()).exists());
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "pub const X: i32 = 1;\n"
    );
    assert_eq!(ledger::read_runtime_events().unwrap(), before_events);
    assert!(paths::project_transition_journal_file(
        &workflow.project_id,
        &format!("intent-approve-{}", proposal.proposal_id),
    )
    .exists());
    clear_patch_test_env(&root);
}
