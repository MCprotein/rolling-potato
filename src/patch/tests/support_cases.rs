use super::*;

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
