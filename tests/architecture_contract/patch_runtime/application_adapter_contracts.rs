fn assert_application_adapter_contracts() {
    let patch_test_modules = [
        "src/app/patch_adapter/tests/mod.rs",
        "src/app/patch_adapter/tests/approval_cases.rs",
        "src/app/patch_adapter/tests/recovery_cases.rs",
        "src/app/patch_adapter/tests/support_cases.rs",
        "src/app/patch_adapter/tests/terminal_cases.rs",
        "src/app/patch_adapter/tests/verification_cases.rs",
    ];
    let approval_transaction_adapter = "src/app/patch_adapter/approval_transaction.rs";
    let approval_dispatch_adapter = "src/app/patch_adapter/approval_dispatch.rs";
    let approval_hook_event_adapter =
        "src/app/patch_adapter/approval_transaction/hook_event.rs";
    let approval_members_adapter = "src/app/patch_adapter/approval_transaction/members.rs";
    let approval_receipt_adapter = "src/app/patch_adapter/approval_transaction/receipt.rs";
    let approval_recovery_adapter = "src/app/patch_adapter/approval_transaction/recovery.rs";
    let approval_source_adapter = "src/app/patch_adapter/approval_transaction/source.rs";
    let approval_orchestration_adapter =
        "src/app/patch_adapter/approval_transaction/transaction.rs";
    let execution_adapter = "src/app/patch_adapter/execution.rs";
    let guard_adapter = "src/app/patch_adapter/guard.rs";
    let proposal_builder_adapter = "src/app/patch_adapter/proposal_builder.rs";
    let proposal_api_adapter = "src/app/patch_adapter/proposal_api.rs";
    let proposal_store_adapter = "src/app/patch_adapter/proposal_store.rs";
    let resume_adapter = "src/app/patch_adapter/resume.rs";
    let shared_adapter = "src/app/patch_adapter/shared.rs";
    let terminal_adapter = "src/app/patch_adapter/terminal.rs";
    let terminal_cancellation_adapter = "src/app/patch_adapter/terminal/cancellation.rs";
    let terminal_denial_adapter = "src/app/patch_adapter/terminal/denial.rs";
    let terminal_gates_adapter = "src/app/patch_adapter/terminal/gates.rs";
    let terminal_rollback_adapter = "src/app/patch_adapter/terminal/rollback.rs";
    let verification_adapter = "src/app/patch_adapter/verification.rs";
    let verification_evidence_adapter =
        "src/app/patch_adapter/verification_evidence.rs";
    let workflow_contract_adapter = "src/app/patch_adapter/workflow_contract.rs";
    let workflow_execution_adapter = "src/app/patch_adapter/workflow_execution.rs";
    let plugin_completion_adapter = "src/app/patch_adapter/workflow_execution/plugin_completion.rs";
    let skill_lifecycle_adapter = "src/app/patch_adapter/workflow_execution/skill_lifecycle.rs";
    let patch_facade = fs::read_to_string("src/app/patch_adapter.rs").unwrap();
    let approval_dispatch = fs::read_to_string(approval_dispatch_adapter).unwrap();
    let approval_transaction = fs::read_to_string(approval_transaction_adapter).unwrap();
    let approval_hook_event = fs::read_to_string(approval_hook_event_adapter).unwrap();
    let approval_members = fs::read_to_string(approval_members_adapter).unwrap();
    let approval_receipt = fs::read_to_string(approval_receipt_adapter).unwrap();
    let approval_recovery = fs::read_to_string(approval_recovery_adapter).unwrap();
    let approval_source = fs::read_to_string(approval_source_adapter).unwrap();
    let approval_orchestration = fs::read_to_string(approval_orchestration_adapter).unwrap();
    let execution = fs::read_to_string(execution_adapter).unwrap();
    let guard = fs::read_to_string(guard_adapter).unwrap();
    let proposal_builder = fs::read_to_string(proposal_builder_adapter).unwrap();
    let proposal_api = fs::read_to_string(proposal_api_adapter).unwrap();
    let proposal_store = fs::read_to_string(proposal_store_adapter).unwrap();
    let resume = fs::read_to_string(resume_adapter).unwrap();
    let shared = fs::read_to_string(shared_adapter).unwrap();
    let terminal = fs::read_to_string(terminal_adapter).unwrap();
    let terminal_cancellation = fs::read_to_string(terminal_cancellation_adapter).unwrap();
    let terminal_denial = fs::read_to_string(terminal_denial_adapter).unwrap();
    let terminal_gates = fs::read_to_string(terminal_gates_adapter).unwrap();
    let terminal_rollback = fs::read_to_string(terminal_rollback_adapter).unwrap();
    let verification = fs::read_to_string(verification_adapter).unwrap();
    let verification_evidence =
        fs::read_to_string(verification_evidence_adapter).unwrap();
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
    let patch_workflow_journey_owners = [
        "tests/patch/workflow_journeys/patch_lifecycle.rs",
        "tests/patch/workflow_journeys/skill_lifecycle.rs",
        "tests/patch/workflow_journeys/transcript_lifecycle.rs",
        "tests/patch/workflow_journeys/imported_plugins.rs",
    ];
    let patch_workflow_journey_sources =
        patch_workflow_journey_owners.map(|owner| fs::read_to_string(owner).unwrap());
    assert!(
        patch_facade.lines().count() < 150,
        "patch facade regrew beyond registration and re-export ownership"
    );
    for (module, owner, responsibilities) in [
        (
            "mod approval_dispatch;",
            &approval_dispatch,
            [
                "fn approve_dispatch_for_intent(",
                "pub fn approve_to_stdout(",
            ]
            .as_slice(),
        ),
        (
            "mod proposal_api;",
            &proposal_api,
            ["pub fn preview_report(", "pub fn prepare_workflow_proposal("].as_slice(),
        ),
        (
            "mod verification_evidence;",
            &verification_evidence,
            [
                "pub fn validate_skill_verification(",
                "pub fn record_failing_test_before(",
            ]
            .as_slice(),
        ),
    ] {
        assert!(
            patch_facade.lines().any(|line| line == module),
            "patch facade does not register owner: {module}"
        );
        for responsibility in responsibilities {
            assert!(
                !patch_facade.contains(responsibility),
                "patch facade still owns responsibility: {responsibility}"
            );
            assert!(
                owner.contains(responsibility),
                "patch owner is missing responsibility: {responsibility}"
            );
        }
    }
    assert!(patch_facade.lines().any(|line| line == "mod shared;"));
    for responsibility in [
        "fn display_none(",
        "fn sha256_text(",
        "fn read_decision_label(",
    ] {
        assert!(
            !patch_facade.contains(responsibility),
            "patch facade still owns shared value helper: {responsibility}"
        );
        assert!(
            shared.contains(responsibility),
            "patch shared owner is missing value helper: {responsibility}"
        );
    }
    assert!(approval_dispatch.lines().count() < 225);
    assert!(proposal_api.lines().count() < 100);
    assert!(shared.lines().count() < 50);
    assert!(verification_evidence.lines().count() < 75);
    assert!(patch_facade
        .lines()
        .any(|line| line == "mod approval_transaction;"));
    assert!(approval_transaction
        .lines()
        .any(|line| line == "mod recovery;"));
    for owner in ["hook_event", "members", "receipt", "source", "transaction"] {
        assert!(
            approval_transaction
                .lines()
                .any(|line| line == format!("mod {owner};")),
            "approval transaction facade does not register owner: {owner}"
        );
    }
    let approval_responsibility = "fn approve_prepared_skill_transaction(";
    assert!(
        !patch_facade.contains(approval_responsibility),
        "approval transaction responsibility escaped into patch facade: {approval_responsibility}"
    );
    assert!(
        approval_orchestration.contains(approval_responsibility),
        "approval transaction adapter is missing responsibility: {approval_responsibility}"
    );
    for (owner, responsibility) in [
        (
            approval_hook_event.as_str(),
            "fn prepare_transaction_hook_event(",
        ),
        (
            approval_members.as_str(),
            "fn prepared_approval_members(",
        ),
        (
            approval_receipt.as_str(),
            "fn prepared_approval_receipt_exists(",
        ),
        (
            approval_source.as_str(),
            "fn prepare_approval_source(",
        ),
    ] {
        assert!(
            owner.contains(responsibility),
            "approval transaction owner is missing responsibility: {responsibility}"
        );
        assert!(
            !approval_transaction.contains(responsibility),
            "approval transaction facade still owns responsibility: {responsibility}"
        );
    }
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
    assert!(approval_transaction.lines().count() < 30);
    assert!(approval_hook_event.lines().count() < 50);
    assert!(approval_members.lines().count() < 175);
    assert!(approval_receipt.lines().count() < 75);
    assert!(approval_recovery.lines().count() < 450);
    assert!(approval_source.lines().count() < 75);
    assert!(approval_orchestration.lines().count() < 275);
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
    for owner in ["cancellation", "denial", "gates", "rollback"] {
        assert!(
            terminal
                .lines()
                .any(|line| line == format!("mod {owner};")),
            "terminal facade does not register {owner}"
        );
    }
    for (owner, responsibilities) in [
        (
            terminal_cancellation.as_str(),
            &[
                "pub fn cancel_workflow_report(",
                "fn cancel_workflow_transaction(",
            ][..],
        ),
        (
            terminal_denial.as_str(),
            &[
                "pub(crate) fn deny_pending_gate_for_tui(",
                "fn deny_pending_gate_transaction(",
            ][..],
        ),
        (
            terminal_gates.as_str(),
            &[
                "pub(crate) fn denial_phase_outcome_code(",
                "pub(super) fn validate_terminal_gate(",
            ][..],
        ),
        (
            terminal_rollback.as_str(),
            &["pub(super) fn prepare_terminal_rollback_source("][..],
        ),
    ] {
        for responsibility in responsibilities {
            assert!(
                !patch_facade.contains(responsibility),
                "terminal workflow responsibility escaped into patch facade: {responsibility}"
            );
            assert!(
                !terminal.contains(responsibility),
                "terminal facade still owns responsibility: {responsibility}"
            );
            assert!(
                owner.contains(responsibility),
                "terminal owner is missing responsibility: {responsibility}"
            );
        }
    }
    for (path, source, line_budget) in [
        (terminal_adapter, terminal.as_str(), 25),
        (
            terminal_cancellation_adapter,
            terminal_cancellation.as_str(),
            125,
        ),
        (terminal_denial_adapter, terminal_denial.as_str(), 275),
        (terminal_gates_adapter, terminal_gates.as_str(), 100),
        (terminal_rollback_adapter, terminal_rollback.as_str(), 100),
    ] {
        assert!(
            source.lines().count() < line_budget,
            "terminal owner regrew beyond {line_budget} lines: {path}"
        );
    }
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
        patch_workflow_journeys.lines().count() < 10,
        "patch workflow journey facade regrew beyond include registration"
    );
    for owner in patch_workflow_journey_owners {
        assert!(
            patch_workflow_journeys
                .contains(&format!("include!(\"{}\")", &owner["tests/patch/".len()..])),
            "patch workflow journey facade is missing owner: {owner}"
        );
    }
    assert!(
        patch_workflow_journey_sources
            .iter()
            .any(|source| source.contains("fn happy_path_is_restart_safe_and_reports_korean")),
        "patch workflow journey owners are missing the happy-path responsibility"
    );
    for (owner, source) in patch_workflow_journey_owners
        .iter()
        .zip(patch_workflow_journey_sources.iter())
    {
        assert!(
            source.lines().count() < 500,
            "patch workflow journey owner regrew beyond its boundary: {owner}"
        );
    }
    assert!(
        patch_contract.lines().count() < 425,
        "patch lifecycle facade regrew beyond fixture and module registration"
    );
}
