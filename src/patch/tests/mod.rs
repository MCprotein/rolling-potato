use super::*;

#[test]
fn cli_approval_materializes_typed_verification_credential_once() {
    let token = "ab".repeat(32);
    let dispatch = ApprovalDispatch {
        report: "patch approve\n- status: applied-awaiting-verification".to_string(),
        verification_credential: Some(OneShotSecret::new(token.clone()).unwrap()),
    };

    let report = dispatch.into_test_report("proposal-one");

    assert_eq!(report.matches(&token).count(), 1);
    assert!(
        report.contains("verification command approval: rpotato patch verify proposal-one --token")
    );
}

#[test]
fn fix_test_requires_cargo_test_verification() {
    let error = validate_skill_verification("fix-test", "pwd").unwrap_err();

    assert_eq!(error.code, 3);
    assert!(error.message.contains("cargo test"));
    validate_skill_verification("fix-test", "cargo test").unwrap();
    validate_skill_verification("small-patch", "pwd").unwrap();
}

#[test]
fn skill_phase_mismatch_blocks_before_patch_apply() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("skill-phase-mismatch");
    let (target, mut workflow, proposal) = create_pending_workflow(&root, "pwd");
    let mut runtime = skill::SkillRuntimeState::new("small-patch", "explicit").unwrap();
    for state in [
        skill::SkillState::ContextReady,
        skill::SkillState::ModelRequested,
        skill::SkillState::ActionRecorded,
    ] {
        runtime.transition(state).unwrap();
    }
    runtime.store_in_workflow(&mut workflow);
    state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();

    let error =
        approve_report(&proposal.proposal_id, &proposal.approval_token, false, None).unwrap_err();
    let source = fs::read_to_string(&target).unwrap();
    clear_patch_test_env(&root);

    assert_eq!(error.code, 3);
    assert!(error.message.contains("skill side effect 차단"));
    assert!(error
        .message
        .contains("expected skill state: awaiting-approval"));
    assert_eq!(source, "pub const X: i32 = 1;\n");
}

#[test]
fn completed_workflow_requires_complete_skill_state() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("complete-skill-state");
    let (_target, mut workflow, _proposal) = create_pending_workflow(&root, "pwd");
    let mut runtime = skill::SkillRuntimeState::new("small-patch", "explicit").unwrap();
    for state in [
        skill::SkillState::ContextReady,
        skill::SkillState::ModelRequested,
        skill::SkillState::ActionRecorded,
        skill::SkillState::AwaitingApproval,
        skill::SkillState::AwaitingVerification,
        skill::SkillState::StopPassed,
    ] {
        runtime.transition(state).unwrap();
    }
    runtime.store_in_workflow(&mut workflow);
    workflow.phase = "complete".to_string();

    let error = validate_completed_workflow(&workflow).unwrap_err();
    clear_patch_test_env(&root);

    assert_eq!(error.code, 3);
    assert!(error.message.contains("skill state: stop-passed"));
}

#[test]
fn preview_creates_diff_record_without_modifying_target() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!("rpotato-patch-test-{}", std::process::id()));
    let project_root = root.join("project");
    fs::create_dir_all(project_root.join("src")).unwrap();
    let target = project_root.join("src/lib.rs");
    fs::write(&target, "fn answer() -> i32 {\n    1\n}\n").unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

    let report = preview_report("src/lib.rs", "    1", "    2").unwrap();
    let contents = fs::read_to_string(&target).unwrap();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");

    assert_eq!(contents, "fn answer() -> i32 {\n    1\n}\n");
    assert!(report.contains("status: diff-only"));
    assert!(report.contains("-    1"));
    assert!(report.contains("+    2"));
    assert!(report.contains("standalone preview는 diff 표시 전용"));
}

#[test]
fn approve_accepts_recorded_token_in_dry_run() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root =
        std::env::temp_dir().join(format!("rpotato-patch-approve-test-{}", std::process::id()));
    let (_target, _workflow, proposal) = create_pending_workflow(&root, "pwd");
    let approval =
        approve_report(&proposal.proposal_id, &proposal.approval_token, true, None).unwrap();
    clear_patch_test_env(&root);

    assert!(approval.contains("status: gate-passed"));
    assert!(approval.contains("boundary: approval gate만 확인했습니다"));
}

#[test]
fn approve_applies_recorded_patch() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root =
        std::env::temp_dir().join(format!("rpotato-patch-apply-test-{}", std::process::id()));
    let (target, _workflow, proposal) = create_pending_workflow(&root, "pwd");
    let approval =
        approve_report(&proposal.proposal_id, &proposal.approval_token, false, None).unwrap();
    let contents = fs::read_to_string(&target).unwrap();
    let rollback_dir = root
        .join("project")
        .join(".rpotato")
        .join("patches")
        .join(&proposal.proposal_id);
    let rollback_exists = fs::read_dir(&rollback_dir)
        .unwrap()
        .filter_map(Result::ok)
        .any(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("rollback"));
    clear_patch_test_env(&root);

    assert_eq!(contents, "pub const X: i32 = 2;\n");
    assert!(rollback_exists);
    assert!(approval.contains("status: applied-awaiting-verification"));
    assert!(approval.contains("verification command는 아직 실행하지 않았습니다"));
    assert!(!approval.contains("stop gate: 통과"));
}

#[test]
fn approval_without_active_skill_fails_before_any_source_effect() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("approval-without-skill");
    let (target, mut workflow, proposal) = create_pending_workflow(&root, "pwd");
    workflow.active_skill_id.clear();
    workflow.skill_invocation.clear();
    workflow.skill_state.clear();
    workflow.skill_completed_hooks.clear();
    workflow.skill_evidence.clear();
    workflow.skill_stop_criteria.clear();
    state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();
    let before_events = ledger::read_runtime_events().unwrap().len();

    let error =
        approve_report(&proposal.proposal_id, &proposal.approval_token, false, None).unwrap_err();

    assert!(error.message.contains("active built-in skill"));
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "pub const X: i32 = 1;\n"
    );
    assert_eq!(ledger::read_runtime_events().unwrap().len(), before_events);
    clear_patch_test_env(&root);
}

#[test]
fn prepared_skill_approval_commits_exact_e0_e9_and_single_current_revision() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("prepared-skill-approval");
    let (target, mut workflow, proposal) = create_pending_workflow(&root, "pwd");
    let mut skill = skill::SkillRuntimeState::new("small-patch", "explicit").unwrap();
    for state in [
        skill::SkillState::ContextReady,
        skill::SkillState::ModelRequested,
        skill::SkillState::ActionRecorded,
        skill::SkillState::AwaitingApproval,
    ] {
        skill.transition(state).unwrap();
    }
    skill.store_in_workflow(&mut workflow);
    workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();
    let before_workflow_revision = workflow.revision;
    let before_current_revision = state::current_state_lease_view().unwrap().revision;
    let before_events = ledger::read_runtime_events().unwrap().len();
    let intent_id = "intent-prepared-skill-approval";

    let report = approve_report_for_intent(
        &proposal.proposal_id,
        &proposal.approval_token,
        false,
        None,
        intent_id,
    )
    .unwrap();
    let after = state::load_workflow(&workflow.workflow_id).unwrap();
    let events = ledger::read_runtime_events().unwrap();
    let suffix = &events[before_events..];

    assert_eq!(
        fs::read_to_string(target).unwrap(),
        "pub const X: i32 = 2;\n"
    );
    assert_eq!(after.revision, before_workflow_revision + 2);
    assert_eq!(after.phase, "pending-verification-approval");
    assert_eq!(
        state::current_state_lease_view().unwrap().revision,
        before_current_revision + 1
    );
    assert_eq!(suffix.len(), 10);
    assert_eq!(
        suffix
            .iter()
            .map(|event| event.event_type.as_str())
            .collect::<Vec<_>>(),
        vec![
            "runtime.intent.accepted",
            "workflow.checkpoint",
            "patch.apply.approved",
            "hook.dispatched",
            "hook.dispatched",
            "hook.dispatched",
            "hook.dispatched",
            "patch.applied",
            "transcript.recorded",
            "workflow.checkpoint",
        ]
    );
    assert!(report.contains("exact prepared journal과 E0..E9"));
    assert!(!paths::project_transition_journal_file(&workflow.project_id, intent_id).exists());
    assert!(!paths::projection_lag_dir().exists());
    clear_patch_test_env(&root);
}

#[test]
fn workflow_pointer_crash_between_r1_r2_installs_recovers_to_r2() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("prepared-pointer-crash");
    let (target, mut workflow, proposal) = create_pending_workflow(&root, "pwd");
    let mut skill = skill::SkillRuntimeState::new("small-patch", "explicit").unwrap();
    for skill_state in [
        skill::SkillState::ContextReady,
        skill::SkillState::ModelRequested,
        skill::SkillState::ActionRecorded,
        skill::SkillState::AwaitingApproval,
    ] {
        skill.transition(skill_state).unwrap();
    }
    skill.store_in_workflow(&mut workflow);
    workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();
    let before_workflow_revision = workflow.revision;
    let before_current_revision = state::current_state_lease_view().unwrap().revision;
    let before_event_count = ledger::read_runtime_events().unwrap().len();
    let intent_id = "intent-prepared-pointer-crash";
    std::env::set_var("RPOTATO_TEST_APPROVAL_TRANSACTION_FAULT", "T3");

    let error = approve_report_for_intent(
        &proposal.proposal_id,
        &proposal.approval_token,
        false,
        None,
        intent_id,
    )
    .unwrap_err();
    std::env::remove_var("RPOTATO_TEST_APPROVAL_TRANSACTION_FAULT");

    assert!(
        error
            .message
            .contains("injected prepared approval transaction fault: T3"),
        "unexpected error: {}",
        error.message
    );
    let r1_pointer =
        fs::read_to_string(paths::project_workflow_file(&workflow.workflow_id)).unwrap();
    assert!(r1_pointer.contains(&format!(
        "\"committed_revision\": {}",
        before_workflow_revision + 1
    )));
    let r1_snapshot = fs::read_to_string(paths::project_workflow_snapshot_file(
        &workflow.workflow_id,
        before_workflow_revision + 1,
    ))
    .unwrap();
    assert!(r1_snapshot.contains("\"phase\": \"approved\""));
    assert!(paths::project_transition_journal_file(&workflow.project_id, intent_id).exists());

    let repair_required = transition::recover_pending_source_bundles().unwrap_err();
    assert!(
        repair_required
            .message
            .contains("projection.repair-required"),
        "unexpected first recovery result: {}",
        repair_required.message
    );
    assert_eq!(transition::recover_pending_source_bundles().unwrap(), 1);
    let r2 = state::load_workflow(&workflow.workflow_id).unwrap();
    let current_after = state::current_state_lease_view().unwrap();
    let events_after = ledger::read_runtime_events().unwrap();
    assert_eq!(r2.revision, before_workflow_revision + 2);
    assert_eq!(r2.phase, "pending-verification-approval");
    assert_eq!(current_after.revision, before_current_revision + 1);
    assert_eq!(events_after.len(), before_event_count + 10);
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "pub const X: i32 = 2;\n"
    );
    assert!(!paths::project_transition_journal_file(&workflow.project_id, intent_id).exists());
    assert!(
        fs::read_dir(paths::projection_lag_dir())
            .unwrap()
            .next()
            .is_none(),
        "projection lag marker cleanup must leave no durable entries"
    );

    assert_eq!(transition::recover_pending_source_bundles().unwrap(), 0);
    assert_eq!(state::load_workflow(&workflow.workflow_id).unwrap(), r2);
    assert_eq!(state::current_state_lease_view().unwrap(), current_after);
    assert_eq!(ledger::read_runtime_events().unwrap(), events_after);
    clear_patch_test_env(&root);
}

#[test]
fn same_approval_intent_after_cleanup_is_refresh_only_with_zero_delta() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("prepared-same-intent-retry");
    let (_target, mut workflow, proposal) = create_pending_workflow(&root, "pwd");
    let mut skill = skill::SkillRuntimeState::new("small-patch", "explicit").unwrap();
    for skill_state in [
        skill::SkillState::ContextReady,
        skill::SkillState::ModelRequested,
        skill::SkillState::ActionRecorded,
        skill::SkillState::AwaitingApproval,
    ] {
        skill.transition(skill_state).unwrap();
    }
    skill.store_in_workflow(&mut workflow);
    state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();
    let intent_id = "intent-prepared-same-retry";
    approve_report_for_intent(
        &proposal.proposal_id,
        &proposal.approval_token,
        false,
        None,
        intent_id,
    )
    .unwrap();
    let workflow_pointer = paths::project_workflow_file(&workflow.workflow_id);
    let workflow_before = fs::read_to_string(&workflow_pointer).unwrap();
    let current_before = fs::read_to_string(paths::current_state_file()).unwrap();
    let events_before = ledger::read_runtime_events().unwrap();

    let retry = approve_report_for_intent(
        &proposal.proposal_id,
        &proposal.approval_token,
        false,
        None,
        intent_id,
    )
    .unwrap();

    assert!(retry.contains("status: refresh-only"));
    assert!(retry.contains("code: secret.refresh-only"));
    assert!(!retry.contains("verification command approval:"));
    assert_eq!(
        fs::read_to_string(&workflow_pointer).unwrap(),
        workflow_before
    );
    assert_eq!(
        fs::read_to_string(paths::current_state_file()).unwrap(),
        current_before
    );
    assert_eq!(ledger::read_runtime_events().unwrap(), events_before);
    clear_patch_test_env(&root);
}

#[test]
fn prepared_approval_t1_t10_faults_recover_exactly_once() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    for stage in [
        "T1",
        "T2",
        "T3-before-pointer",
        "T3",
        "T4",
        "T5",
        "T6",
        "T7",
        "T8-before-pointer",
        "T8",
        "T9",
        "T10",
    ] {
        let root = patch_test_root(&format!("prepared-recover-{stage}"));
        let (target, workflow, proposal) = create_pending_workflow(&root, "pwd");
        let before_workflow_revision = workflow.revision;
        let before_current_revision = state::current_state_lease_view().unwrap().revision;
        let before_event_count = ledger::read_runtime_events().unwrap().len();
        let intent_id = format!("intent-prepared-recover-{}", stage.to_ascii_lowercase());
        std::env::set_var("RPOTATO_TEST_APPROVAL_TRANSACTION_FAULT", stage);

        let error = approve_report_for_intent(
            &proposal.proposal_id,
            &proposal.approval_token,
            false,
            None,
            &intent_id,
        )
        .unwrap_err();
        std::env::remove_var("RPOTATO_TEST_APPROVAL_TRANSACTION_FAULT");
        assert!(
            error.message.contains(stage),
            "stage: {stage}, error: {}",
            error.message
        );
        assert!(paths::project_transition_journal_file(&workflow.project_id, &intent_id).exists());

        if stage == "T10" {
            assert_eq!(
                transition::recover_pending_source_bundles().unwrap(),
                1,
                "stage: {stage}"
            );
        } else {
            let interrupted = transition::recover_pending_source_bundles().unwrap_err();
            assert!(
                interrupted.message.contains("projection.repair-required"),
                "stage: {stage}, error: {}",
                interrupted.message
            );
            let journal = paths::project_transition_journal_file(&workflow.project_id, &intent_id);
            let bundle =
                transition::parse_prepared_source_bundle(&fs::read_to_string(&journal).unwrap())
                    .unwrap();
            assert!(transition::projection_lag_path(&bundle).unwrap().exists());
            assert_eq!(
                transition::recover_pending_source_bundles().unwrap(),
                1,
                "stage: {stage}"
            );
        }
        let recovered = state::load_workflow(&workflow.workflow_id).unwrap();
        let current = state::current_state_lease_view().unwrap();
        let events = ledger::read_runtime_events().unwrap();
        assert_eq!(
            recovered.revision,
            before_workflow_revision + 2,
            "stage: {stage}"
        );
        assert_eq!(
            recovered.phase, "pending-verification-approval",
            "stage: {stage}"
        );
        assert_eq!(
            current.revision,
            before_current_revision + 1,
            "stage: {stage}"
        );
        assert_eq!(events.len(), before_event_count + 10, "stage: {stage}");
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "pub const X: i32 = 2;\n",
            "stage: {stage}"
        );
        assert!(!paths::project_transition_journal_file(&workflow.project_id, &intent_id).exists());
        assert_eq!(
            transition::recover_pending_source_bundles().unwrap(),
            0,
            "stage: {stage}"
        );
        assert_eq!(
            state::load_workflow(&workflow.workflow_id).unwrap(),
            recovered
        );
        assert_eq!(state::current_state_lease_view().unwrap(), current);
        assert_eq!(ledger::read_runtime_events().unwrap(), events);
        if stage == "T5" {
            let rotation = rotate_workflow_token_report(&proposal.proposal_id).unwrap();
            let replacement = report_value(&rotation, "새 approval token").unwrap();
            let verified = verify_report(&proposal.proposal_id, &replacement).unwrap();
            assert!(verified.contains("패치 작업 완료"));
            assert_eq!(
                state::load_workflow(&workflow.workflow_id).unwrap().phase,
                "complete"
            );
        }
        clear_patch_test_env(&root);
    }
}

#[test]
fn second_intent_after_t1_recovers_or_blocks_before_competing_journal() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("prepared-second-intent-after-t1");
    let (_target, mut workflow, proposal) = create_pending_workflow(&root, "pwd");
    let mut skill = skill::SkillRuntimeState::new("small-patch", "explicit").unwrap();
    for skill_state in [
        skill::SkillState::ContextReady,
        skill::SkillState::ModelRequested,
        skill::SkillState::ActionRecorded,
        skill::SkillState::AwaitingApproval,
    ] {
        skill.transition(skill_state).unwrap();
    }
    skill.store_in_workflow(&mut workflow);
    state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();
    let first_intent = "intent-prepared-first-t1";
    let second_intent = "intent-prepared-second";
    std::env::set_var("RPOTATO_TEST_APPROVAL_TRANSACTION_FAULT", "T1");
    approve_report_for_intent(
        &proposal.proposal_id,
        &proposal.approval_token,
        false,
        None,
        first_intent,
    )
    .unwrap_err();
    std::env::remove_var("RPOTATO_TEST_APPROVAL_TRANSACTION_FAULT");

    let second = approve_report_for_intent(
        &proposal.proposal_id,
        &proposal.approval_token,
        false,
        None,
        second_intent,
    )
    .unwrap_err();

    assert!(second.message.contains("projection.repair-required"));
    assert!(paths::project_transition_journal_file(&workflow.project_id, first_intent).exists());
    assert!(!paths::project_transition_journal_file(&workflow.project_id, second_intent).exists());
    let prepared = fs::read_dir(paths::project_transition_journal_dir(&workflow.project_id))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.ends_with(".prepared.json") || name.ends_with(".prepared.json.tmp")
        })
        .count();
    assert_eq!(prepared, 1);
    clear_patch_test_env(&root);
}

#[test]
fn source_install_unsupported_platform_blocks_before_all_effects() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("unsupported-platform-zero-effects");
    let _ = fs::remove_dir_all(&root);
    std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

    let error = ensure_source_install_platform_supported(false, "windows", false).unwrap_err();

    assert!(error
        .message
        .contains("source-install.unsupported-platform"));
    assert!(!root.exists());
    assert!(ensure_source_install_platform_supported(false, "windows", true).is_ok());
    let source = include_str!("../../patch.rs");
    let dispatch = source
        .split_once("fn approve_dispatch_for_intent(")
        .unwrap()
        .1
        .split_once("fn ensure_source_install_platform_supported(")
        .unwrap()
        .0;
    assert!(
        dispatch
            .find("ensure_source_install_platform_supported")
            .unwrap()
            < dispatch.find("let proposal_path").unwrap()
    );
    clear_patch_test_env(&root);
}

#[test]
fn t10_lag_install_failure_preserves_committed_journal() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("t10-lag-install-failure");
    let (_target, workflow, proposal) = create_prepared_pending_workflow(&root, "pwd");
    let intent_id = "intent-t10-lag-install-failure";
    std::env::set_var("RPOTATO_TEST_APPROVAL_PROJECTION_FAULT", "converge");
    std::env::set_var("RPOTATO_TEST_PROJECTION_LAG_FAULT", "temp-fsync");

    let error = approve_report_for_intent(
        &proposal.proposal_id,
        &proposal.approval_token,
        false,
        None,
        intent_id,
    )
    .unwrap_err();
    std::env::remove_var("RPOTATO_TEST_APPROVAL_PROJECTION_FAULT");
    std::env::remove_var("RPOTATO_TEST_PROJECTION_LAG_FAULT");

    let journal = paths::project_transition_journal_file(&workflow.project_id, intent_id);
    let bundle =
        transition::parse_prepared_source_bundle(&fs::read_to_string(&journal).unwrap()).unwrap();
    let lag = transition::projection_lag_path(&bundle).unwrap();
    assert!(error.message.contains("projection.lag-install-failed"));
    assert!(journal.exists());
    assert!(!lag.exists());
    assert_eq!(
        fs::read_to_string(lag.with_extension("json.tmp")).unwrap(),
        bundle.additional_members.last().unwrap().bytes_utf8
    );
    assert!(transition::recover_pending_source_bundles()
        .unwrap_err()
        .message
        .contains("projection.repair-required"));
    assert_eq!(transition::recover_pending_source_bundles().unwrap(), 1);
    assert!(!journal.exists());
    assert!(!lag.exists());
    clear_patch_test_env(&root);
}

#[test]
fn projection_lag_crash_after_lag_removal_before_journal_cleanup_converges() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("lag-remove-before-journal-cleanup");
    let (_target, workflow, proposal) = create_prepared_pending_workflow(&root, "pwd");
    let intent_id = "intent-lag-remove-before-journal-cleanup";
    std::env::set_var("RPOTATO_TEST_APPROVAL_PROJECTION_FAULT", "converge");
    approve_report_for_intent(
        &proposal.proposal_id,
        &proposal.approval_token,
        false,
        None,
        intent_id,
    )
    .unwrap_err();
    std::env::remove_var("RPOTATO_TEST_APPROVAL_PROJECTION_FAULT");
    let journal = paths::project_transition_journal_file(&workflow.project_id, intent_id);
    let bundle =
        transition::parse_prepared_source_bundle(&fs::read_to_string(&journal).unwrap()).unwrap();
    let lag = transition::projection_lag_path(&bundle).unwrap();
    assert!(lag.exists());
    std::env::set_var("RPOTATO_TEST_PROJECTION_LAG_FAULT", "journal-remove");

    let interrupted = transition::recover_pending_source_bundles().unwrap_err();
    std::env::remove_var("RPOTATO_TEST_PROJECTION_LAG_FAULT");

    assert!(interrupted.message.contains("journal-remove"));
    assert!(journal.exists());
    assert!(!lag.exists());
    assert_eq!(transition::recover_pending_source_bundles().unwrap(), 1);
    assert!(!journal.exists());
    assert_eq!(transition::recover_pending_source_bundles().unwrap(), 0);
    clear_patch_test_env(&root);
}

#[test]
fn projection_success_receipt_requires_lag_and_journal_parent_fsyncs() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    for (case, projection_fault, lag_fault) in [
        ("lag-parent", true, "parent-fsync"),
        ("journal-parent", false, "journal-parent-fsync"),
    ] {
        let root = patch_test_root(&format!("success-receipt-fsync-{case}"));
        let (_target, workflow, proposal) = create_prepared_pending_workflow(&root, "pwd");
        let intent_id = format!("intent-success-receipt-fsync-{case}");
        if projection_fault {
            std::env::set_var("RPOTATO_TEST_APPROVAL_PROJECTION_FAULT", "converge");
        }
        std::env::set_var("RPOTATO_TEST_PROJECTION_LAG_FAULT", lag_fault);

        let error = approve_report_for_intent(
            &proposal.proposal_id,
            &proposal.approval_token,
            false,
            None,
            &intent_id,
        )
        .unwrap_err();
        std::env::remove_var("RPOTATO_TEST_APPROVAL_PROJECTION_FAULT");
        std::env::remove_var("RPOTATO_TEST_PROJECTION_LAG_FAULT");

        let journal = paths::project_transition_journal_file(&workflow.project_id, &intent_id);
        assert!(error.message.contains(lag_fault), "case: {case}");
        assert!(journal.exists(), "case: {case}");
        assert_eq!(transition::recover_pending_source_bundles().unwrap(), 1);
        let retry = approve_report_for_intent(
            &proposal.proposal_id,
            &proposal.approval_token,
            false,
            None,
            &intent_id,
        )
        .unwrap();
        assert!(retry.contains("status: refresh-only"), "case: {case}");
        clear_patch_test_env(&root);
    }
}

#[test]
fn projection_lag_journal_cleanup_state_matrix_is_closed() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("prepared-projection-repair");
    let (_target, mut workflow, proposal) = create_pending_workflow(&root, "pwd");
    let mut skill = skill::SkillRuntimeState::new("small-patch", "explicit").unwrap();
    for skill_state in [
        skill::SkillState::ContextReady,
        skill::SkillState::ModelRequested,
        skill::SkillState::ActionRecorded,
        skill::SkillState::AwaitingApproval,
    ] {
        skill.transition(skill_state).unwrap();
    }
    skill.store_in_workflow(&mut workflow);
    state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();
    let intent_id = "intent-prepared-projection-repair";
    std::env::set_var("RPOTATO_TEST_APPROVAL_PROJECTION_FAULT", "converge");

    let error = approve_report_for_intent(
        &proposal.proposal_id,
        &proposal.approval_token,
        false,
        None,
        intent_id,
    )
    .unwrap_err();
    std::env::remove_var("RPOTATO_TEST_APPROVAL_PROJECTION_FAULT");

    assert!(error.message.contains("projection.repair-required"));
    let journal = paths::project_transition_journal_file(&workflow.project_id, intent_id);
    let bundle =
        transition::parse_prepared_source_bundle(&fs::read_to_string(&journal).unwrap()).unwrap();
    let final_event_id = &bundle.semantic_events[9].event_id;
    let lag = paths::projection_lag_file(intent_id, final_event_id);
    let lag_member = bundle.additional_members.last().unwrap();
    assert_eq!(fs::read_to_string(&lag).unwrap(), lag_member.bytes_utf8);
    assert!(journal.exists());
    let workflow_pointer = paths::project_workflow_file(&workflow.workflow_id);
    let workflow_before = fs::read_to_string(&workflow_pointer).unwrap();
    let current_before = fs::read_to_string(paths::current_state_file()).unwrap();
    let events_before = ledger::read_runtime_events().unwrap();

    fs::remove_file(&lag).unwrap();
    std::env::set_var("RPOTATO_TEST_APPROVAL_PROJECTION_FAULT", "converge");
    let interrupted_repair = transition::recover_pending_source_bundles().unwrap_err();
    std::env::remove_var("RPOTATO_TEST_APPROVAL_PROJECTION_FAULT");
    assert!(interrupted_repair
        .message
        .contains("projection.repair-required"));
    assert!(journal.exists());
    assert_eq!(fs::read_to_string(&lag).unwrap(), lag_member.bytes_utf8);
    assert_eq!(
        fs::read_to_string(&workflow_pointer).unwrap(),
        workflow_before
    );
    assert_eq!(
        fs::read_to_string(paths::current_state_file()).unwrap(),
        current_before
    );
    assert_eq!(ledger::read_runtime_events().unwrap(), events_before);

    assert_eq!(transition::recover_pending_source_bundles().unwrap(), 1);
    assert!(!journal.exists());
    assert!(!lag.exists());
    assert_eq!(
        fs::read_to_string(&workflow_pointer).unwrap(),
        workflow_before
    );
    assert_eq!(
        fs::read_to_string(paths::current_state_file()).unwrap(),
        current_before
    );
    assert_eq!(ledger::read_runtime_events().unwrap(), events_before);

    fs::create_dir_all(lag.parent().unwrap()).unwrap();
    fs::write(&lag, lag_member.bytes_utf8.as_bytes()).unwrap();
    let orphan = transition::recover_pending_source_bundles().unwrap_err();
    assert!(orphan
        .message
        .contains("orphan 또는 ambiguous projection lag"));
    assert_eq!(fs::read_to_string(&lag).unwrap(), lag_member.bytes_utf8);
    fs::remove_file(&lag).unwrap();
    clear_patch_test_env(&root);
}

#[test]
fn projection_lag_reference_and_member_mutation_matrix_fails_closed() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("projection-lag-mutation-matrix");
    let (_target, workflow, proposal) = create_prepared_pending_workflow(&root, "pwd");
    let intent_id = "intent-projection-lag-mutation-matrix";
    std::env::set_var("RPOTATO_TEST_APPROVAL_PROJECTION_FAULT", "converge");
    approve_report_for_intent(
        &proposal.proposal_id,
        &proposal.approval_token,
        false,
        None,
        intent_id,
    )
    .unwrap_err();
    std::env::remove_var("RPOTATO_TEST_APPROVAL_PROJECTION_FAULT");
    let journal = paths::project_transition_journal_file(&workflow.project_id, intent_id);
    let body = fs::read_to_string(&journal).unwrap();
    let bundle = transition::parse_prepared_source_bundle(&body).unwrap();
    let lag_member = bundle.additional_members.last().unwrap();
    let event_id = lag_member.binding.event_id.as_deref().unwrap();
    let mutations = [
        body.replacen("\"member_index\":10", "\"member_index\":9", 1),
        body.replacen(
            "project-session-ledger",
            "project-session-ledger-mutated",
            1,
        ),
        body.replacen(event_id, "event-mutated", 1),
        body.replacen(&lag_member.path, "state/projection-lag/wrong.json", 1),
        body.replacen(
            &lag_member.binding.artifact_id.clone().unwrap(),
            "projection-lag-deadbeef",
            1,
        ),
    ];
    for (index, mutation) in mutations.iter().enumerate() {
        assert_ne!(mutation, &body, "mutation {index} changed no bytes");
        assert!(
            transition::parse_prepared_source_bundle(mutation).is_err(),
            "mutation {index}"
        );
    }
    clear_patch_test_env(&root);
}

#[test]
fn projection_lag_restart_validates_reference_member_installed_bytes_and_head() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("projection-lag-restart-validation");
    let (_target, workflow, proposal) = create_prepared_pending_workflow(&root, "pwd");
    let intent_id = "intent-projection-lag-restart-validation";
    std::env::set_var("RPOTATO_TEST_APPROVAL_PROJECTION_FAULT", "converge");
    approve_report_for_intent(
        &proposal.proposal_id,
        &proposal.approval_token,
        false,
        None,
        intent_id,
    )
    .unwrap_err();
    std::env::remove_var("RPOTATO_TEST_APPROVAL_PROJECTION_FAULT");
    let journal = paths::project_transition_journal_file(&workflow.project_id, intent_id);
    let bundle =
        transition::parse_prepared_source_bundle(&fs::read_to_string(&journal).unwrap()).unwrap();
    let lag = transition::projection_lag_path(&bundle).unwrap();
    let current_before = fs::read(paths::current_state_file()).unwrap();
    let workflow_before = fs::read(paths::project_workflow_file(&workflow.workflow_id)).unwrap();
    let events_before = ledger::read_runtime_events().unwrap();
    let installed = fs::read_to_string(&lag).unwrap();
    fs::write(
        &lag,
        installed.replacen(
            "project-session-ledger",
            "project-session-ledger-mutated",
            1,
        ),
    )
    .unwrap();

    let error = transition::recover_pending_source_bundles().unwrap_err();

    assert!(error.message.contains("projection lag"));
    assert_eq!(
        fs::read(paths::current_state_file()).unwrap(),
        current_before
    );
    assert_eq!(
        fs::read(paths::project_workflow_file(&workflow.workflow_id)).unwrap(),
        workflow_before
    );
    assert_eq!(ledger::read_runtime_events().unwrap(), events_before);
    assert!(journal.exists());
    clear_patch_test_env(&root);
}

#[test]
fn projection_lag_orphan_without_journal_blocks() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("projection-lag-orphan-without-journal");
    set_patch_test_env(&root);
    fs::create_dir_all(root.join("project")).unwrap();
    state::initialize().unwrap();
    let lag = paths::projection_lag_file("intent-orphan", "event-orphan");
    fs::create_dir_all(lag.parent().unwrap()).unwrap();
    fs::write(&lag, b"{}" as &[u8]).unwrap();

    let error = transition::recover_pending_source_bundles().unwrap_err();

    assert!(error
        .message
        .contains("orphan 또는 ambiguous projection lag"));
    assert_eq!(fs::read(&lag).unwrap(), b"{}" as &[u8]);
    clear_patch_test_env(&root);
}

#[cfg(target_os = "linux")]
#[test]
fn canonical_non_utf8_source_path_fails_before_any_effect() {
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::fs::symlink;

    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("canonical-non-utf8-source");
    let project = root.join("project");
    fs::create_dir_all(&project).unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    state::initialize().unwrap();
    let non_utf8 = project.join(std::ffi::OsString::from_vec(
        b"source-non-utf8-\xff.rs".to_vec(),
    ));
    fs::write(&non_utf8, b"pub const VALUE: i32 = 1;\n").unwrap();
    symlink(&non_utf8, project.join("source-link.rs")).unwrap();
    let current_before = fs::read(paths::current_state_file()).unwrap();
    let ledger_before = fs::read(paths::runtime_ledger_file()).unwrap();
    let journal_before = fs::read_dir(paths::project_transition_journal_dir(
        &ledger::validated_current_identity().unwrap().project_id,
    ))
    .unwrap()
    .count();

    let error = resolve_target_for("patch approve", "source-link.rs").unwrap_err();

    assert!(error
        .message
        .contains("canonical project-relative path가 UTF-8이 아닙니다"));
    assert_eq!(
        fs::read(paths::current_state_file()).unwrap(),
        current_before
    );
    assert_eq!(
        fs::read(paths::runtime_ledger_file()).unwrap(),
        ledger_before
    );
    assert_eq!(
        fs::read_dir(paths::project_transition_journal_dir(
            &ledger::validated_current_identity().unwrap().project_id,
        ))
        .unwrap()
        .count(),
        journal_before
    );
    clear_patch_test_env(&root);
}

#[test]
fn prepared_bundle_member_tamper_blocks_recovery_before_effects() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("prepared-member-tamper");
    let (target, mut workflow, proposal) = create_pending_workflow(&root, "pwd");
    let mut skill = skill::SkillRuntimeState::new("small-patch", "explicit").unwrap();
    for skill_state in [
        skill::SkillState::ContextReady,
        skill::SkillState::ModelRequested,
        skill::SkillState::ActionRecorded,
        skill::SkillState::AwaitingApproval,
    ] {
        skill.transition(skill_state).unwrap();
    }
    skill.store_in_workflow(&mut workflow);
    state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();
    let intent_id = "intent-prepared-member-tamper";
    std::env::set_var("RPOTATO_TEST_APPROVAL_TRANSACTION_FAULT", "T1");
    approve_report_for_intent(
        &proposal.proposal_id,
        &proposal.approval_token,
        false,
        None,
        intent_id,
    )
    .unwrap_err();
    std::env::remove_var("RPOTATO_TEST_APPROVAL_TRANSACTION_FAULT");
    let journal = paths::project_transition_journal_file(&workflow.project_id, intent_id);
    let mut bundle =
        transition::parse_prepared_source_bundle(&fs::read_to_string(&journal).unwrap()).unwrap();
    bundle.additional_members[2].bytes_utf8 = bundle.additional_members[2].bytes_utf8.replacen(
        "\"phase\": \"approved\"",
        "\"phase\": \"tampered\"",
        1,
    );
    fs::write(
        &journal,
        transition::render_prepared_source_bundle(&bundle).unwrap(),
    )
    .unwrap();
    let source_before = fs::read_to_string(&target).unwrap();
    let workflow_pointer = paths::project_workflow_file(&workflow.workflow_id);
    let workflow_before = fs::read(&workflow_pointer).unwrap();
    let current_before = fs::read(paths::current_state_file()).unwrap();
    let events_before = ledger::read_runtime_events().unwrap();

    let error = transition::recover_pending_source_bundles().unwrap_err();

    assert!(error.message.contains("workflow") || error.message.contains("corrupt"));
    assert!(journal.exists());
    assert_eq!(fs::read_to_string(&target).unwrap(), source_before);
    assert_eq!(fs::read(&workflow_pointer).unwrap(), workflow_before);
    assert_eq!(
        fs::read(paths::current_state_file()).unwrap(),
        current_before
    );
    assert_eq!(ledger::read_runtime_events().unwrap(), events_before);
    clear_patch_test_env(&root);
}

#[test]
fn approve_blocks_changed_target_before_apply() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-patch-changed-target-test-{}",
        std::process::id()
    ));
    let (target, _workflow, proposal) = create_pending_workflow(&root, "pwd");
    fs::write(&target, "pub const X: i32 = 3;\n").unwrap();
    let err =
        approve_report(&proposal.proposal_id, &proposal.approval_token, false, None).unwrap_err();
    let contents = fs::read_to_string(&target).unwrap();
    clear_patch_test_env(&root);

    assert_eq!(err.code, 3);
    assert!(err.message.contains("preview 이후 변경"));
    assert_eq!(contents, "pub const X: i32 = 3;\n");
}

#[test]
fn approve_rejects_inline_verification_command_before_apply() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-patch-verify-block-test-{}",
        std::process::id()
    ));
    let (target, _workflow, proposal) = create_pending_workflow(&root, "pwd");
    let err = approve_report(
        &proposal.proposal_id,
        &proposal.approval_token,
        false,
        Some("echo hi"),
    )
    .unwrap_err();
    let contents = fs::read_to_string(&target).unwrap();
    clear_patch_test_env(&root);

    assert_eq!(err.code, 3);
    assert!(err
        .message
        .contains("verification command 승인은 분리되어 있습니다"));
    assert_eq!(contents, "pub const X: i32 = 1;\n");
}

#[cfg(unix)]
#[test]
fn verification_runs_only_after_separate_approval() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-patch-verify-run-test-{}",
        std::process::id()
    ));
    let (target, _workflow, proposal) = create_pending_workflow(&root, "pwd");
    let approval =
        approve_report(&proposal.proposal_id, &proposal.approval_token, false, None).unwrap();
    let pending = state::load_workflow(&_workflow.workflow_id).unwrap();
    let verify_token = verification_token(&approval);
    let apply_token_rejected =
        verify_report(&proposal.proposal_id, &proposal.approval_token).unwrap_err();
    let verify_token_rejected =
        approve_report(&proposal.proposal_id, &verify_token, false, None).unwrap_err();
    let verified = verify_report(&proposal.proposal_id, &verify_token).unwrap();
    let contents = fs::read_to_string(&target).unwrap();
    clear_patch_test_env(&root);

    assert_eq!(contents, "pub const X: i32 = 2;\n");
    assert_eq!(pending.phase, "pending-verification-approval");
    assert!(pending.evidence_id.is_empty());
    assert_eq!(apply_token_rejected.code, 3);
    assert_eq!(verify_token_rejected.code, 3);
    assert!(verified.contains("검증: 통과"));
    assert!(
        crate::runtime_core::reporting::korean_guard::validate(&verified),
        "guard rejected report: {verified}"
    );
}

#[cfg(unix)]
#[test]
fn verification_approval_commits_prepared_audit_before_command() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("verification-prepared-audit");
    let (_target, workflow, proposal) = create_pending_workflow(&root, "pwd");
    let approval =
        approve_report(&proposal.proposal_id, &proposal.approval_token, false, None).unwrap();
    let verify_token = verification_token(&approval);
    let before = ledger::read_runtime_events().unwrap().len();

    std::env::set_var(
        "RPOTATO_TEST_VERIFICATION_FAULT",
        "after-started-checkpoint",
    );
    verify_report(&proposal.proposal_id, &verify_token).unwrap_err();
    std::env::remove_var("RPOTATO_TEST_VERIFICATION_FAULT");

    let started = state::load_workflow(&workflow.workflow_id).unwrap();
    let events = ledger::read_runtime_events().unwrap();
    let committed = &events[before..];
    assert_eq!(started.phase, "verification-started");
    assert_eq!(started.verification_approval_state, "approved");
    assert_eq!(committed.len(), 3);
    assert_eq!(
        committed
            .iter()
            .map(|event| event.event_type.as_str())
            .collect::<Vec<_>>(),
        [
            "runtime.intent.accepted",
            "workflow.checkpoint",
            "patch.verification.approved",
        ]
    );
    assert!(committed[0]
        .details
        .contains("intent_kind=approve-verification"));
    assert!(committed[2].details.contains("gate=verification-command"));
    assert!(!paths::project_transition_journal_file(
        &workflow.project_id,
        &format!("intent-verify-{}", proposal.proposal_id)
    )
    .exists());
    clear_patch_test_env(&root);
}

#[cfg(unix)]
#[test]
fn prepared_verification_approval_faults_recover_without_running_command() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    for stage in ["V1", "V2", "V3-before-pointer", "V3", "V4", "V5", "V6"] {
        let root = patch_test_root(&format!("verification-recover-{stage}"));
        let (target, workflow, proposal) = create_pending_workflow(&root, "pwd");
        let approval =
            approve_report(&proposal.proposal_id, &proposal.approval_token, false, None).unwrap();
        let verify_token = verification_token(&approval);
        let before_events = ledger::read_runtime_events().unwrap().len();
        let before_current = state::current_state_lease_view().unwrap().revision;
        let intent_id = format!("intent-verify-{}", proposal.proposal_id);
        std::env::set_var("RPOTATO_TEST_VERIFICATION_APPROVAL_FAULT", stage);

        let error = verify_report(&proposal.proposal_id, &verify_token).unwrap_err();
        std::env::remove_var("RPOTATO_TEST_VERIFICATION_APPROVAL_FAULT");
        assert!(error.message.contains(stage), "stage: {stage}");
        assert!(paths::project_transition_journal_file(&workflow.project_id, &intent_id).exists());

        assert_eq!(
            transition::recover_pending_source_bundles().unwrap(),
            1,
            "stage: {stage}"
        );
        let recovered = state::load_workflow(&workflow.workflow_id).unwrap();
        let current = state::current_state_lease_view().unwrap();
        let events = ledger::read_runtime_events().unwrap();
        assert_eq!(recovered.phase, "verification-started", "stage: {stage}");
        assert_eq!(
            recovered.verification_approval_state, "approved",
            "stage: {stage}"
        );
        assert!(recovered.evidence_id.is_empty(), "stage: {stage}");
        assert_eq!(current.revision, before_current + 1, "stage: {stage}");
        assert_eq!(events.len(), before_events + 3, "stage: {stage}");
        assert_eq!(
            events[before_events..]
                .iter()
                .map(|event| event.event_type.as_str())
                .collect::<Vec<_>>(),
            [
                "runtime.intent.accepted",
                "workflow.checkpoint",
                "patch.verification.approved",
            ],
            "stage: {stage}"
        );
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "pub const X: i32 = 2;\n"
        );
        assert!(!paths::project_transition_journal_file(&workflow.project_id, &intent_id).exists());
        assert_eq!(transition::recover_pending_source_bundles().unwrap(), 0);
        assert_eq!(ledger::read_runtime_events().unwrap(), events);
        clear_patch_test_env(&root);
    }
}

#[cfg(unix)]
#[test]
fn intermediate_approval_phases_cannot_resume_without_prepared_journal() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();

    let approved_root = patch_test_root("resume-approved-without-journal");
    let (approved_target, mut approved, _proposal) = create_pending_workflow(&approved_root, "pwd");
    approved.phase = "approved".to_string();
    approved.approval_state = "approved".to_string();
    approved = state::checkpoint_workflow(approved.clone(), approved.revision).unwrap();
    let approved_error = resume_workflow_report(&approved.workflow_id).unwrap_err();
    assert!(approved_error
        .message
        .contains("exact E0..E9 prepared journal"));
    assert_eq!(
        fs::read_to_string(&approved_target).unwrap(),
        "pub const X: i32 = 1;\n"
    );
    clear_patch_test_env(&approved_root);

    let verification_root = patch_test_root("resume-verification-without-journal");
    let (verification_target, workflow, proposal) =
        create_pending_workflow(&verification_root, "pwd");
    approve_report(&proposal.proposal_id, &proposal.approval_token, false, None).unwrap();
    let mut verification = state::load_workflow(&workflow.workflow_id).unwrap();
    verification.phase = "verification-approved".to_string();
    verification.verification_approval_state = "approved".to_string();
    verification = state::checkpoint_workflow(verification.clone(), verification.revision).unwrap();
    let verification_error = resume_workflow_report(&verification.workflow_id).unwrap_err();
    assert!(verification_error
        .message
        .contains("prepared verification journal"));
    assert_eq!(
        fs::read_to_string(&verification_target).unwrap(),
        "pub const X: i32 = 2;\n"
    );
    clear_patch_test_env(&verification_root);
}

#[test]
fn proposal_summary_reads_preview_record() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-patch-tui-read-test-{}",
        std::process::id()
    ));
    let project_root = root.join("project");
    fs::create_dir_all(project_root.join("src")).unwrap();
    fs::write(project_root.join("src/lib.rs"), "pub const X: i32 = 1;\n").unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

    let report = preview_report("src/lib.rs", "1", "2").unwrap();
    let proposal_id = report_value(&report, "proposal id").unwrap();
    let summaries = proposal_summaries(5).unwrap();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");

    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].proposal_id, proposal_id);
    assert_eq!(summaries[0].status, "pending-approval");
}

#[test]
fn approval_nonce_is_random_hash_only_and_not_reconstructable() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root =
        std::env::temp_dir().join(format!("rpotato-patch-random-token-{}", std::process::id()));
    let project_root = root.join("project");
    fs::create_dir_all(project_root.join("src")).unwrap();
    fs::write(project_root.join("src/lib.rs"), "pub const X: i32 = 1;\n").unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

    let first_token = issue_approval_token().unwrap();
    let (_target, workflow, proposal) = create_pending_workflow(&root, "pwd");
    let second_token = proposal.approval_token.clone();
    let proposal_id = proposal.proposal_id;
    let record =
        fs::read_to_string(paths::project_patch_proposals_dir().join(format!("{proposal_id}.txt")))
            .unwrap();
    let detail =
        proposal_detail_for_workflow_bounded(&workflow, &proposal_id, MAX_PROPOSAL_RECORD_BYTES)
            .unwrap();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(root);

    assert_eq!(first_token.len(), 64);
    assert_eq!(second_token.len(), 64);
    assert_ne!(first_token, second_token);
    assert!(!record.contains(&first_token));
    assert!(!record.contains(&second_token));
    assert!(record.contains(&format!(
        "approval_token_hash={}",
        sha256_text(&second_token)
    )));
    assert!(detail.diff.contains("pub const X"));
}

#[test]
fn bad_token_does_not_consume_valid_nonce() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-patch-bad-then-good-{}",
        std::process::id()
    ));
    let (target, _workflow, proposal) = create_pending_workflow(&root, "pwd");
    let rejected = approve_report(&proposal.proposal_id, "wrong-token", false, None).unwrap_err();
    let accepted =
        approve_report(&proposal.proposal_id, &proposal.approval_token, false, None).unwrap();
    let contents = fs::read_to_string(&target).unwrap();
    clear_patch_test_env(&root);

    assert_eq!(rejected.code, 3);
    assert!(accepted.contains("status: applied-awaiting-verification"));
    assert_eq!(contents, "pub const X: i32 = 2;\n");
}

#[test]
fn standalone_preview_never_overwrites_existing_artifact() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("standalone-overwrite");
    set_patch_test_env(&root);
    let target = root.join("project/src/lib.rs");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, "pub const X: i32 = 1;\n").unwrap();

    preview_report("src/lib.rs", "1", "2").unwrap();
    let error = preview_report("src/lib.rs", "1", "2").unwrap_err();
    let source = fs::read_to_string(&target).unwrap();

    clear_patch_test_env(&root);
    assert_eq!(error.code, 3);
    assert!(error.message.contains("이미 존재"));
    assert_eq!(source, "pub const X: i32 = 1;\n");
}

#[test]
fn token_rotate_checkpoints_new_hash_and_invalidates_old_token() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("token-rotate");
    let (_target, workflow, proposal) = create_pending_workflow(&root, "pwd");
    let old_token = proposal.approval_token.clone();

    let report = rotate_workflow_token_report(&proposal.proposal_id).unwrap();
    let new_token = report_value(&report, "새 approval token").unwrap();
    let rotated = state::load_workflow(&workflow.workflow_id).unwrap();
    let old_error = approve_report(&proposal.proposal_id, &old_token, true, None).unwrap_err();
    let accepted = approve_report(&proposal.proposal_id, &new_token, true, None).unwrap();

    clear_patch_test_env(&root);
    assert_eq!(rotated.approval_state, "pending-rotated");
    assert_eq!(rotated.approval_credential_hash, sha256_text(&new_token));
    assert_ne!(rotated.approval_credential_hash, sha256_text(&old_token));
    assert_eq!(old_error.code, 3);
    assert!(accepted.contains("gate-passed"));
}

#[test]
fn verification_token_rotate_invalidates_old_token() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("verification-token-rotate");
    let (_target, workflow, proposal) = create_pending_workflow(&root, "pwd");
    let approval =
        approve_report(&proposal.proposal_id, &proposal.approval_token, false, None).unwrap();
    let old_token = verification_token(&approval);

    let rotation = rotate_workflow_token_report(&proposal.proposal_id).unwrap();
    let new_token = report_value(&rotation, "새 approval token").unwrap();
    let rotated = state::load_workflow(&workflow.workflow_id).unwrap();
    let old_error = verify_report(&proposal.proposal_id, &old_token).unwrap_err();
    let verified = verify_report(&proposal.proposal_id, &new_token).unwrap();

    clear_patch_test_env(&root);
    assert_eq!(rotated.verification_approval_state, "pending-rotated");
    assert_eq!(
        rotated.verification_credential_hash,
        sha256_text(&new_token)
    );
    assert_ne!(
        rotated.verification_credential_hash,
        sha256_text(&old_token)
    );
    assert_eq!(old_error.code, 3);
    assert!(verified.contains("검증: 통과"));
}

#[test]
fn proposal_and_canonical_token_tamper_fail_closed() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    for mode in ["proposal", "token"] {
        let root = patch_test_root(&format!("tamper-{mode}"));
        let (target, mut workflow, proposal) = create_pending_workflow(&root, "pwd");
        if mode == "proposal" {
            let path =
                paths::project_patch_proposals_dir().join(format!("{}.txt", proposal.proposal_id));
            let mut body = fs::read_to_string(&path).unwrap();
            body.push_str("tampered trailing bytes\n");
            fs::write(path, body).unwrap();
        } else {
            workflow.approval_credential_hash = "0".repeat(64);
            workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();
            assert_eq!(workflow.approval_credential_hash, "0".repeat(64));
        }

        let error = approve_report(&proposal.proposal_id, &proposal.approval_token, false, None)
            .unwrap_err();
        let source = fs::read_to_string(&target).unwrap();
        clear_patch_test_env(&root);
        assert_eq!(error.code, 3);
        assert_eq!(source, "pub const X: i32 = 1;\n");
    }
}

#[test]
fn legacy_v2_plaintext_proposal_requires_safe_repreview_migration() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("legacy-v2");
    let (target, _workflow, proposal) = create_pending_workflow(&root, "pwd");
    let proposal_id = proposal.proposal_id;
    let token = proposal.approval_token;
    let path = paths::project_patch_proposals_dir().join(format!("{proposal_id}.txt"));
    let body = fs::read_to_string(&path)
        .unwrap()
        .replacen("record_version=4", "record_version=2", 1)
        .replacen(
            &format!("approval_token_hash={}", sha256_text(&token)),
            &format!("approval_token={token}"),
            1,
        );
    fs::write(&path, body).unwrap();

    let error = approve_report(&proposal_id, &token, false, None).unwrap_err();
    let source = fs::read_to_string(&target).unwrap();
    let scrubbed = fs::read_to_string(&path).unwrap();
    clear_patch_test_env(&root);
    assert_eq!(error.code, 3);
    assert!(error.message.contains("hash-only로 atomic scrub"));
    assert!(!scrubbed.contains(&format!("approval_token={token}")));
    assert_eq!(source, "pub const X: i32 = 1;\n");
}

#[test]
fn proposal_loader_rejects_duplicate_unknown_and_mixed_credential_fields() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("proposal-strict");
    set_patch_test_env(&root);
    let target = root.join("project/src/lib.rs");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, "pub const X: i32 = 1;\n").unwrap();
    let report = preview_report("src/lib.rs", "1", "2").unwrap();
    let proposal_id = report_value(&report, "proposal id").unwrap();
    let path = paths::project_patch_proposals_dir().join(format!("{proposal_id}.txt"));
    let original = fs::read_to_string(&path).unwrap();
    for malformed in [
        original.replacen("record_version=4", "record_version=4\nrecord_version=4", 1),
        original.replacen("path=", "unknown_key=x\npath=", 1),
        original.replacen(
            "approval_token_hash=",
            "approval_token=legacy\napproval_token_hash=",
            1,
        ),
    ] {
        fs::write(&path, malformed).unwrap();
        assert!(load_proposal_record(&proposal_id, &path).is_err());
    }
    clear_patch_test_env(&root);
}

#[test]
fn verification_started_crash_never_auto_reruns_command() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("verification-started");
    let (target, workflow, proposal) = create_pending_workflow(&root, "pwd");
    let approval =
        approve_report(&proposal.proposal_id, &proposal.approval_token, false, None).unwrap();
    let verify_token = verification_token(&approval);
    std::env::set_var(
        "RPOTATO_TEST_VERIFICATION_FAULT",
        "after-started-checkpoint",
    );
    let injected = verify_report(&proposal.proposal_id, &verify_token).unwrap_err();
    std::env::remove_var("RPOTATO_TEST_VERIFICATION_FAULT");
    let started = state::load_workflow(&workflow.workflow_id).unwrap();
    let resume = resume_workflow_report(&workflow.workflow_id).unwrap_err();
    let source = fs::read_to_string(&target).unwrap();

    clear_patch_test_env(&root);
    assert_eq!(injected.code, 1);
    assert_eq!(started.phase, "verification-started");
    assert_eq!(source, "pub const X: i32 = 2;\n");
    assert!(resume.message.contains("자동 재실행하지 않습니다"));
}

#[test]
fn verification_started_can_be_explicitly_cancelled_and_restores_source() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("verification-cancel");
    let (target, workflow, proposal) = create_pending_workflow(&root, "pwd");
    let approval =
        approve_report(&proposal.proposal_id, &proposal.approval_token, false, None).unwrap();
    let verify_token = verification_token(&approval);
    std::env::set_var(
        "RPOTATO_TEST_VERIFICATION_FAULT",
        "after-started-checkpoint",
    );
    verify_report(&proposal.proposal_id, &verify_token).unwrap_err();
    std::env::remove_var("RPOTATO_TEST_VERIFICATION_FAULT");

    let report = cancel_workflow_report(&workflow.workflow_id).unwrap();
    let cancelled = state::load_workflow(&workflow.workflow_id).unwrap();
    let source = fs::read_to_string(&target).unwrap();
    clear_patch_test_env(&root);
    assert!(report.contains("workflow 취소 완료"));
    assert_eq!(cancelled.phase, "cancelled");
    assert_eq!(source, "pub const X: i32 = 1;\n");
}

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

#[test]
fn incomplete_model_phases_resume_to_truthful_terminal_failure() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    for phase in ["model-pending", "action-recorded"] {
        let root = patch_test_root(&format!("resume-{phase}"));
        set_patch_test_env(&root);
        fs::create_dir_all(root.join("project")).unwrap();
        state::initialize().unwrap();
        let mut workflow = state::create_workflow("incomplete model phase").unwrap();
        if phase == "action-recorded" {
            workflow.phase = phase.to_string();
            workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();
        }

        let error = resume_workflow_report(&workflow.workflow_id).unwrap_err();
        let failed = state::load_workflow(&workflow.workflow_id).unwrap();
        clear_patch_test_env(&root);
        assert_eq!(error.code, 3);
        assert_eq!(failed.phase, "failed");
        assert_eq!(failed.failure_reason, format!("resume-incomplete-{phase}"));
        assert!(error
            .message
            .contains("backend 또는 command를 자동 재실행하지 않습니다"));
    }
}

#[test]
fn approval_lock_excludes_concurrent_side_effect_owner() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("approval-lock");
    fs::create_dir_all(root.join("project")).unwrap();
    set_patch_test_env(&root);
    let first = ApprovalLock::acquire("patch-proposal-lock-test").unwrap();
    let second = match ApprovalLock::acquire("patch-proposal-lock-test") {
        Ok(_) => panic!("second lock unexpectedly succeeded"),
        Err(error) => error,
    };
    drop(first);
    let third = ApprovalLock::acquire("patch-proposal-lock-test").unwrap();
    drop(third);
    clear_patch_test_env(&root);
    assert!(second.message.contains("patch approve lock 차단"));
}

#[test]
fn direct_approve_fails_closed_when_multiple_workflows_are_active() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("direct-multi-active");
    let (target, _workflow, proposal) = create_pending_workflow(&root, "pwd");
    state::create_workflow("second active").unwrap();

    let error =
        approve_report(&proposal.proposal_id, &proposal.approval_token, false, None).unwrap_err();
    let source = fs::read_to_string(&target).unwrap();
    clear_patch_test_env(&root);
    assert_eq!(error.code, 3);
    assert!(error
        .message
        .contains("여러 non-terminal canonical workflow"));
    assert_eq!(source, "pub const X: i32 = 1;\n");
}

#[test]
fn rollback_preserves_concurrent_user_edit() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = patch_test_root("rollback-concurrent-edit");
    let (target, _workflow, proposal) = create_pending_workflow(&root, "pwd");
    let record = load_proposal_record(
        &proposal.proposal_id,
        &paths::project_patch_proposals_dir().join(format!("{}.txt", proposal.proposal_id)),
    )
    .unwrap();
    fs::write(&target, &record.proposed_content).unwrap();
    let rollback_path = rollback_path_for_record(&record).unwrap();
    state::atomic_replace_bytes(&rollback_path, b"pub const X: i32 = 1;\n").unwrap();
    fs::write(&target, "pub const X: i32 = 99;\n").unwrap();

    let result = restore_from_rollback(&record, &rollback_path);
    let source = fs::read_to_string(&target).unwrap();
    clear_patch_test_env(&root);
    assert!(!result.restored);
    assert!(result.status.contains("restore-conflict"));
    assert_eq!(source, "pub const X: i32 = 99;\n");
}

#[test]
fn rollback_tamper_and_replace_failure_are_reported_truthfully() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    for fault in ["tamper-record", "replace-failure"] {
        let root = std::env::temp_dir().join(format!(
            "rpotato-patch-rollback-{fault}-{}",
            std::process::id()
        ));
        let (target, workflow, proposal) = create_pending_workflow(&root, "cargo test");

        let approval =
            approve_report(&proposal.proposal_id, &proposal.approval_token, false, None).unwrap();
        let verify_token = verification_token(&approval);
        std::env::set_var("RPOTATO_TEST_ROLLBACK_FAULT", fault);
        let error = verify_report(&proposal.proposal_id, &verify_token).unwrap_err();
        std::env::remove_var("RPOTATO_TEST_ROLLBACK_FAULT");
        let failed = state::load_workflow(&workflow.workflow_id).unwrap();
        let source = fs::read_to_string(&target).unwrap();
        let evidence = fs::read_to_string(
            paths::project_evidence_dir().join(format!("{}.json", failed.evidence_id)),
        )
        .unwrap();

        clear_patch_test_env(&root);

        assert_eq!(error.code, 3, "fault: {fault}");
        assert!(error.message.contains("rollback-failed"), "fault: {fault}");
        assert_eq!(failed.failure_reason, "verification-failed-rollback-failed");
        assert_eq!(source, "pub const X: i32 = 2;\n");
        assert!(evidence.contains(&format!("\"source_hash\": \"{}\"", sha256_text(&source))));
    }
}

#[test]
fn preview_blocks_ambiguous_find_text() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!("rpotato-patch-ambiguous-{}", std::process::id()));
    let project_root = root.join("project");
    fs::create_dir_all(&project_root).unwrap();
    fs::write(project_root.join("file.txt"), "same\nsame\n").unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

    let err = preview_report("file.txt", "same", "changed").unwrap_err();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");

    assert_eq!(err.code, 3);
    assert!(err.message.contains("여러 번"));
}

fn report_value(report: &str, key: &str) -> Option<String> {
    let prefix = format!("- {key}: ");
    report
        .lines()
        .find_map(|line| line.strip_prefix(&prefix).map(|value| value.to_string()))
}

fn verification_token(report: &str) -> String {
    report_value(report, "verification command approval")
        .and_then(|command| {
            command
                .split_once(" --token ")
                .map(|(_, token)| token.to_string())
        })
        .expect("verification approval token")
}

fn patch_test_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("rpotato-patch-{name}-{}", std::process::id()))
}

fn set_patch_test_env(root: &Path) {
    let _ = fs::remove_dir_all(root);
    std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
}

fn clear_patch_test_env(root: &Path) {
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(root);
}

fn create_pending_workflow(
    root: &Path,
    verification: &str,
) -> (PathBuf, state::WorkflowRecord, WorkflowProposal) {
    set_patch_test_env(root);
    let target = root.join("project/src/lib.rs");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, "pub const X: i32 = 1;\n").unwrap();
    state::initialize().unwrap();
    let mut workflow = state::create_workflow("change X").unwrap();
    let proposal = prepare_workflow_proposal(
        &workflow.workflow_id,
        &workflow.action_id,
        "src/lib.rs",
        "1",
        "2",
        verification,
    )
    .unwrap();
    workflow.source_path = proposal.relative_path.clone();
    workflow.source_hash = proposal.original_sha256.clone();
    workflow.before_hash = proposal.original_sha256.clone();
    workflow.after_hash = proposal.proposed_sha256.clone();
    workflow.proposal_id = proposal.proposal_id.clone();
    workflow.proposal_hash = proposal.proposal_hash.clone();
    workflow.approval_credential_hash = proposal.approval_credential_hash.clone();
    workflow.verification_plan = proposal.verification_command.clone();
    workflow.approval_state = "pending".to_string();
    workflow.phase = "pending-approval".to_string();
    let mut skill = skill::SkillRuntimeState::new("small-patch", "explicit").unwrap();
    for state in [
        skill::SkillState::ContextReady,
        skill::SkillState::ModelRequested,
        skill::SkillState::ActionRecorded,
        skill::SkillState::AwaitingApproval,
    ] {
        skill.transition(state).unwrap();
    }
    for hook in [
        "session_start",
        "user_request_received",
        "pre_context_pack",
        "post_context_pack",
        "pre_model_request",
        "post_model_response",
        "pre_action_parse",
        "post_action_parse",
        "pre_tool_call",
        "post_tool_result",
    ] {
        skill.record_hook(hook).unwrap();
    }
    skill.record_evidence("diff_review");
    skill.store_in_workflow(&mut workflow);
    workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();
    (target, workflow, proposal)
}

fn create_prepared_pending_workflow(
    root: &Path,
    verification: &str,
) -> (PathBuf, state::WorkflowRecord, WorkflowProposal) {
    let (target, mut workflow, proposal) = create_pending_workflow(root, verification);
    let mut skill = skill::SkillRuntimeState::new("small-patch", "explicit").unwrap();
    for skill_state in [
        skill::SkillState::ContextReady,
        skill::SkillState::ModelRequested,
        skill::SkillState::ActionRecorded,
        skill::SkillState::AwaitingApproval,
    ] {
        skill.transition(skill_state).unwrap();
    }
    skill.store_in_workflow(&mut workflow);
    state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();
    (target, workflow, proposal)
}
