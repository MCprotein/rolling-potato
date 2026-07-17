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
