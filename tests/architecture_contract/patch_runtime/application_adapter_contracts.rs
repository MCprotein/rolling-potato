fn assert_application_adapter_contracts() {
    let intent_execution_path = "src/app/intent_adapter/execution.rs";
    let intent_tests_path = "src/app/intent_adapter/tests.rs";
    let patch_test_modules = [
        "src/app/patch_adapter/tests/mod.rs",
        "src/app/patch_adapter/tests/approval_cases.rs",
        "src/app/patch_adapter/tests/recovery_cases.rs",
        "src/app/patch_adapter/tests/support_cases.rs",
        "src/app/patch_adapter/tests/terminal_cases.rs",
        "src/app/patch_adapter/tests/verification_cases.rs",
    ];
    let approval_transaction_adapter = "src/app/patch_adapter/approval_transaction.rs";
    let approval_recovery_adapter = "src/app/patch_adapter/approval_transaction/recovery.rs";
    let execution_adapter = "src/app/patch_adapter/execution.rs";
    let guard_adapter = "src/app/patch_adapter/guard.rs";
    let proposal_builder_adapter = "src/app/patch_adapter/proposal_builder.rs";
    let proposal_store_adapter = "src/app/patch_adapter/proposal_store.rs";
    let resume_adapter = "src/app/patch_adapter/resume.rs";
    let terminal_adapter = "src/app/patch_adapter/terminal.rs";
    let verification_adapter = "src/app/patch_adapter/verification.rs";
    let workflow_contract_adapter = "src/app/patch_adapter/workflow_contract.rs";
    let workflow_execution_adapter = "src/app/patch_adapter/workflow_execution.rs";
    let plugin_completion_adapter = "src/app/patch_adapter/workflow_execution/plugin_completion.rs";
    let skill_lifecycle_adapter = "src/app/patch_adapter/workflow_execution/skill_lifecycle.rs";
    let intent_facade = fs::read_to_string("src/app/intent_adapter.rs").unwrap();
    let intent_execution = fs::read_to_string(intent_execution_path).unwrap();
    let intent_tests = fs::read_to_string(intent_tests_path).unwrap();
    let patch_facade = fs::read_to_string("src/app/patch_adapter.rs").unwrap();
    let approval_transaction = fs::read_to_string(approval_transaction_adapter).unwrap();
    let approval_recovery = fs::read_to_string(approval_recovery_adapter).unwrap();
    let execution = fs::read_to_string(execution_adapter).unwrap();
    let guard = fs::read_to_string(guard_adapter).unwrap();
    let proposal_builder = fs::read_to_string(proposal_builder_adapter).unwrap();
    let proposal_store = fs::read_to_string(proposal_store_adapter).unwrap();
    let resume = fs::read_to_string(resume_adapter).unwrap();
    let terminal = fs::read_to_string(terminal_adapter).unwrap();
    let verification = fs::read_to_string(verification_adapter).unwrap();
    let workflow_contract = fs::read_to_string(workflow_contract_adapter).unwrap();
    let workflow_execution = fs::read_to_string(workflow_execution_adapter).unwrap();
    let patch_test_module = fs::read_to_string(patch_test_modules[0]).unwrap();
    let patch_approval_tests = fs::read_to_string(patch_test_modules[1]).unwrap();
    let patch_recovery_tests = fs::read_to_string(patch_test_modules[2]).unwrap();
    let patch_support_tests = fs::read_to_string(patch_test_modules[3]).unwrap();
    let patch_terminal_tests = fs::read_to_string(patch_test_modules[4]).unwrap();
    let patch_verification_tests = fs::read_to_string(patch_test_modules[5]).unwrap();
    let patch_harness = fs::read_to_string("tests/patch_loop.rs").unwrap();
    let patch_contract = fs::read_to_string("tests/patch/lifecycle.rs").unwrap();
    let patch_backend_runtime = fs::read_to_string("tests/patch/backend_runtime.rs").unwrap();
    let patch_concurrency = fs::read_to_string("tests/patch/concurrency.rs").unwrap();
    let patch_safety = fs::read_to_string("tests/patch/patch_safety.rs").unwrap();
    let patch_workflow_journeys = fs::read_to_string("tests/patch/workflow_journeys.rs").unwrap();
    assert!(
        intent_facade.contains("#[path = \"intent_adapter/tests.rs\"]"),
        "intent facade does not register its regression-test owner"
    );
    assert!(
        intent_facade.lines().any(|line| line == "mod execution;"),
        "intent facade does not register its execution owner"
    );
    for responsibility in [
        "pub(super) fn run_with_decision(",
        "plugin.capability.admitted",
        "action.candidate.prepared",
        "invalid-or-hostile-model-action",
    ] {
        assert!(
            intent_execution.contains(responsibility),
            "intent execution owner is missing: {responsibility}"
        );
    }
    assert!(
        !intent_facade.contains("pub(super) fn run_with_decision("),
        "intent facade still owns workflow execution"
    );
    for regression in [
        "fn explicit_skill_has_priority(",
        "fn model_action_parser_blocks_requested_side_effects(",
        "fn model_answer_fails_closed_on_non_korean_natural_language(",
        "fn review_outcomes_require_answer_bound_file_and_severity_evidence(",
    ] {
        assert!(
            intent_tests.contains(regression),
            "intent regression owner is missing: {regression}"
        );
        assert!(
            !intent_facade.contains(regression),
            "intent facade still owns regression test: {regression}"
        );
    }
    assert!(
        intent_facade.lines().count() < 600,
        "intent facade regrew beyond the v0.37.9 boundary"
    );
    assert!(
        intent_execution.lines().count() < 600,
        "intent execution module regrew beyond its ownership boundary"
    );
    assert!(
        intent_tests.lines().count() < 325,
        "intent regression module regrew beyond its ownership boundary"
    );
    assert!(
        patch_facade.lines().count() < 500,
        "patch facade regrew beyond the v0.37.9 boundary"
    );
    assert!(patch_facade
        .lines()
        .any(|line| line == "mod approval_transaction;"));
    assert!(approval_transaction
        .lines()
        .any(|line| line == "mod recovery;"));
    let approval_responsibility = "fn approve_prepared_skill_transaction(";
    assert!(
        !patch_facade.contains(approval_responsibility),
        "approval transaction responsibility escaped into patch facade: {approval_responsibility}"
    );
    assert!(
        approval_transaction.contains(approval_responsibility),
        "approval transaction adapter is missing responsibility: {approval_responsibility}"
    );
    for recovery_responsibility in [
        "fn recover_prepared_approval_bundle(",
        "fn recover_prepared_verification_bundle(",
        "fn validate_prepared_approval_semantics(",
    ] {
        assert!(
            approval_recovery.contains(recovery_responsibility),
            "approval recovery adapter is missing responsibility: {recovery_responsibility}"
        );
        assert!(
            !approval_transaction.contains(recovery_responsibility),
            "approval transaction orchestration still owns recovery: {recovery_responsibility}"
        );
    }
    assert!(approval_transaction.lines().count() < 550);
    assert!(approval_recovery.lines().count() < 450);
    assert!(patch_facade.lines().any(|line| line == "mod execution;"));
    for escaped_responsibility in [
        "fn apply_proposal(",
        "fn run_verification(",
        "fn restore_from_rollback(",
    ] {
        assert!(
            !patch_facade.contains(escaped_responsibility),
            "patch execution responsibility escaped into facade: {escaped_responsibility}"
        );
        assert!(
            execution.contains(escaped_responsibility),
            "patch execution adapter is missing responsibility: {escaped_responsibility}"
        );
    }
    assert!(execution.lines().count() < 300);
    assert!(patch_facade.lines().any(|line| line == "mod guard;"));
    for escaped_responsibility in [
        "struct ApprovalLock",
        "fn approval_transaction_fault(",
        "fn restore_bytes(",
    ] {
        assert!(
            !patch_facade.contains(escaped_responsibility),
            "patch guard responsibility escaped into facade: {escaped_responsibility}"
        );
        assert!(
            guard.contains(escaped_responsibility),
            "patch guard adapter is missing responsibility: {escaped_responsibility}"
        );
    }
    assert!(guard.lines().count() < 250);
    assert!(patch_facade
        .lines()
        .any(|line| line == "mod proposal_builder;"));
    for escaped_responsibility in [
        "fn build_preview(",
        "struct TargetPath",
        "fn fill_os_random(",
    ] {
        assert!(
            !patch_facade.contains(escaped_responsibility),
            "proposal builder responsibility escaped into patch facade: {escaped_responsibility}"
        );
        assert!(
            proposal_builder.contains(escaped_responsibility),
            "proposal builder adapter is missing responsibility: {escaped_responsibility}"
        );
    }
    assert!(proposal_builder.lines().count() < 250);
    assert!(patch_facade
        .lines()
        .any(|line| line == "mod proposal_store;"));
    for escaped_responsibility in [
        "fn read_proposal_contents_bounded(",
        "fn load_proposal_record(",
        "fn validate_token_hash(",
    ] {
        assert!(
            !patch_facade.contains(escaped_responsibility),
            "proposal store responsibility escaped into patch facade: {escaped_responsibility}"
        );
        assert!(
            proposal_store.contains(escaped_responsibility),
            "proposal store adapter is missing responsibility: {escaped_responsibility}"
        );
    }
    assert!(proposal_store.lines().count() < 350);
    assert!(patch_facade.lines().any(|line| line == "mod resume;"));
    for escaped_responsibility in [
        "fn proposal_summaries_bounded(",
        "fn preflight_resume_workflow(",
        "fn resume_workflow_for_tui(",
    ] {
        assert!(
            !patch_facade.contains(escaped_responsibility),
            "resume responsibility escaped into patch facade: {escaped_responsibility}"
        );
        assert!(
            resume.contains(escaped_responsibility),
            "resume adapter is missing responsibility: {escaped_responsibility}"
        );
    }
    assert!(resume.lines().count() < 400);
    assert!(patch_facade.lines().any(|line| line == "mod terminal;"));
    for escaped_responsibility in [
        "fn cancel_workflow_transaction(",
        "fn deny_pending_gate_transaction(",
        "fn prepare_terminal_rollback_source(",
    ] {
        assert!(
            !patch_facade.contains(escaped_responsibility),
            "terminal workflow responsibility escaped into patch facade: {escaped_responsibility}"
        );
        assert!(
            terminal.contains(escaped_responsibility),
            "terminal workflow adapter is missing responsibility: {escaped_responsibility}"
        );
    }
    assert!(terminal.lines().count() < 500);
    assert!(patch_facade.lines().any(|line| line == "mod verification;"));
    for escaped_responsibility in [
        "fn verify_report_for_intent(",
        "fn approve_prepared_verification_transaction(",
        "fn prepared_verification_members(",
    ] {
        assert!(
            !patch_facade.contains(escaped_responsibility),
            "verification responsibility escaped into patch facade: {escaped_responsibility}"
        );
        assert!(
            verification.contains(escaped_responsibility),
            "verification adapter is missing responsibility: {escaped_responsibility}"
        );
    }
    assert!(verification.lines().count() < 300);
    assert!(patch_facade
        .lines()
        .any(|line| line == "mod workflow_contract;"));
    for escaped_responsibility in [
        "fn stale_selection_error(",
        "fn validate_workflow_binding(",
        "fn success_report(",
    ] {
        assert!(
            !patch_facade.contains(escaped_responsibility),
            "workflow contract responsibility escaped into patch facade: {escaped_responsibility}"
        );
        assert!(
            workflow_contract.contains(escaped_responsibility),
            "workflow contract adapter is missing responsibility: {escaped_responsibility}"
        );
    }
    assert!(workflow_contract.lines().count() < 150);
    assert!(patch_facade
        .lines()
        .any(|line| line == "mod workflow_execution;"));
    let workflow_execution_responsibility = "fn continue_approved_workflow(";
    assert!(
        !patch_facade.contains(workflow_execution_responsibility),
        "workflow execution responsibility escaped into patch facade: {workflow_execution_responsibility}"
    );
    assert!(
        workflow_execution.contains(workflow_execution_responsibility),
        "workflow execution adapter is missing responsibility: {workflow_execution_responsibility}"
    );
    let plugin_completion = fs::read_to_string(plugin_completion_adapter).unwrap();
    let skill_lifecycle = fs::read_to_string(skill_lifecycle_adapter).unwrap();
    assert!(
        workflow_execution
            .lines()
            .any(|line| line == "mod plugin_completion;"),
        "workflow execution adapter does not register its plugin completion owner"
    );
    for escaped_responsibility in [
        "pub(in super::super) fn validate_completed_plugin_workflow(",
        "pub(in super::super) fn ensure_plugin_completion_event(",
        "pub(in super::super) fn ensure_plugin_completion_event_under_transition(",
        "pub(in super::super) fn plugin_completion_recovery_report(",
    ] {
        assert!(
            !workflow_execution.contains(escaped_responsibility),
            "plugin completion responsibility escaped into workflow execution orchestration: {escaped_responsibility}"
        );
        assert!(
            plugin_completion.contains(escaped_responsibility),
            "plugin completion adapter is missing responsibility: {escaped_responsibility}"
        );
    }
    assert!(
        workflow_execution
            .lines()
            .any(|line| line == "mod skill_lifecycle;"),
        "workflow execution adapter does not register its skill lifecycle owner"
    );
    for escaped_responsibility in [
        "pub(in super::super) fn workflow_skill_runtime(",
        "pub(super) fn validate_skill_phase_for_side_effect(",
        "pub(in super::super) fn validate_failing_test_before(",
        "pub(in super::super) fn validate_completed_workflow(",
        "pub(super) fn dispatch_workflow_skill_hook(",
        "pub(in super::super) fn finalize_verified_skill(",
    ] {
        assert!(
            !workflow_execution.contains(escaped_responsibility),
            "skill lifecycle responsibility escaped into workflow execution orchestration: {escaped_responsibility}"
        );
        assert!(
            skill_lifecycle.contains(escaped_responsibility),
            "skill lifecycle adapter is missing responsibility: {escaped_responsibility}"
        );
    }
    assert!(workflow_execution.lines().count() < 375);
    assert!(plugin_completion.lines().count() < 175);
    assert!(skill_lifecycle.lines().count() < 175);
    assert!(patch_facade.contains("#[path = \"patch_adapter/tests/mod.rs\"]"));
    assert!(!patch_facade.contains("mod tests {"));
    assert!(
        patch_test_module.lines().count() < 150,
        "shared patch test fixtures regrew beyond their boundary"
    );
    for module in [
        "mod approval_cases;",
        "mod recovery_cases;",
        "mod support_cases;",
        "mod terminal_cases;",
        "mod verification_cases;",
    ] {
        assert!(
            patch_test_module.lines().any(|line| line == module),
            "shared patch test module is missing child ownership: {module}"
        );
    }
    for (owner, source, marker) in [
        (
            patch_test_modules[1],
            &patch_approval_tests,
            "fn prepared_skill_approval_commits_exact_e0_e9_and_single_current_revision",
        ),
        (
            patch_test_modules[2],
            &patch_recovery_tests,
            "fn prepared_bundle_member_tamper_blocks_recovery_before_effects",
        ),
        (
            patch_test_modules[3],
            &patch_support_tests,
            "fn rollback_tamper_and_replace_failure_are_reported_truthfully",
        ),
        (
            patch_test_modules[4],
            &patch_terminal_tests,
            "fn terminal_denial_crash_matrix_recovers_one_exact_commit",
        ),
        (
            patch_test_modules[5],
            &patch_verification_tests,
            "fn verification_runs_only_after_separate_approval",
        ),
    ] {
        assert!(
            source.lines().count() < 700,
            "patch regression test owner regrew beyond its boundary: {owner}"
        );
        assert!(
            source.contains(marker),
            "patch regression test owner is missing responsibility: {owner} -> {marker}"
        );
    }
    assert!(
        patch_harness.lines().count() <= 5 && patch_harness.contains("patch/lifecycle.rs"),
        "patch integration harness is not a thin compatibility entrypoint"
    );
    for module in [
        "mod backend_runtime;",
        "mod concurrency;",
        "mod patch_safety;",
        "mod workflow_journeys;",
    ] {
        assert!(
            patch_contract.lines().any(|line| line == module),
            "patch lifecycle facade is missing contract owner: {module}"
        );
    }
    for fixture_boundary in [
        "const MAX_CONCURRENT_FIXTURES:",
        "fn acquire_fixture_permit()",
        "_permit: FixturePermit",
    ] {
        assert!(
            patch_contract.contains(fixture_boundary),
            "patch lifecycle fixture lost its bounded-concurrency guard: {fixture_boundary}"
        );
    }
    for (owner, source, marker, limit) in [
        (
            "tests/patch/backend_runtime.rs",
            &patch_backend_runtime,
            "fn backend_generation_cancel_keeps_sidecar_and_cleans_active_state",
            275,
        ),
        (
            "tests/patch/concurrency.rs",
            &patch_concurrency,
            "fn token_rotate_recovers_lost_delivery_and_invalidates_old_token_across_processes",
            150,
        ),
        (
            "tests/patch/patch_safety.rs",
            &patch_safety,
            "fn complete_resume_revalidates_deleted_evidence",
            300,
        ),
        (
            "tests/patch/workflow_journeys.rs",
            &patch_workflow_journeys,
            "fn happy_path_is_restart_safe_and_reports_korean",
            825,
        ),
    ] {
        assert!(
            source.contains(marker),
            "patch lifecycle owner is missing responsibility: {owner} -> {marker}"
        );
        assert!(
            source.lines().count() < limit,
            "patch lifecycle owner regrew beyond its boundary: {owner}"
        );
    }
    assert!(
        patch_contract.lines().count() < 425,
        "patch lifecycle facade regrew beyond fixture and module registration"
    );
}
