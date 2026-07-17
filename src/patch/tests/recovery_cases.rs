use super::*;

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
