use super::*;

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
