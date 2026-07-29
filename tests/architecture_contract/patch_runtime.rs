use super::*;

#[test]
fn v0379_patch_owners_hold_lifecycle_decisions() {
    let intent_execution_path = "src/app/intent_adapter/execution.rs";
    let intent_tests_path = "src/app/intent_adapter/tests.rs";
    assert!(Path::new("src/app/intent_adapter.rs").is_file());
    assert!(Path::new(intent_execution_path).is_file());
    assert!(Path::new(intent_tests_path).is_file());
    assert!(!Path::new("src/intent.rs").exists());
    assert!(!Path::new("src/intent").exists());
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(!main.lines().any(|line| line == "mod intent;"));
    let app_root = fs::read_to_string("src/app.rs").unwrap();
    assert!(
        app_root
            .lines()
            .any(|line| line == "pub(crate) mod intent_adapter;"),
        "application root does not register the intent adapter"
    );
    assert!(Path::new("src/app/patch_adapter.rs").is_file());
    assert!(!Path::new("src/patch.rs").exists());
    assert!(!Path::new("src/patch").exists());
    assert!(!main.lines().any(|line| line == "mod patch;"));
    assert!(
        app_root
            .lines()
            .any(|line| line == "pub(crate) mod patch_adapter;"),
        "application root does not register the patch adapter"
    );
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
    assert!(Path::new(approval_transaction_adapter).is_file());
    assert!(Path::new(approval_recovery_adapter).is_file());
    assert!(Path::new(execution_adapter).is_file());
    assert!(Path::new(guard_adapter).is_file());
    assert!(Path::new(proposal_builder_adapter).is_file());
    assert!(Path::new(proposal_store_adapter).is_file());
    assert!(Path::new(resume_adapter).is_file());
    assert!(Path::new(terminal_adapter).is_file());
    assert!(Path::new(verification_adapter).is_file());
    assert!(Path::new(workflow_contract_adapter).is_file());
    assert!(Path::new(workflow_execution_adapter).is_file());
    assert!(Path::new(plugin_completion_adapter).is_file());
    assert!(Path::new(skill_lifecycle_adapter).is_file());
    assert!(Path::new(intent_execution_path).is_file());
    assert!(Path::new(intent_tests_path).is_file());
    for patch_test_module in patch_test_modules {
        assert!(
            Path::new(patch_test_module).is_file(),
            "missing patch regression test owner: {patch_test_module}"
        );
    }
    let owners = [
        "src/runtime_core/patch/approval.rs",
        "src/runtime_core/patch/application.rs",
        "src/runtime_core/patch/intent.rs",
        "src/runtime_core/patch/proposal.rs",
        "src/runtime_core/patch/verification.rs",
    ];
    for target in owners {
        assert!(
            Path::new(target).is_file(),
            "missing v0.37.9 patch owner: {target}"
        );
    }

    let runtime_core = fs::read_to_string("src/runtime_core/mod.rs").unwrap();
    assert!(
        runtime_core
            .lines()
            .any(|line| line == "pub(crate) mod patch;"),
        "patch runtime owner is not crate-private"
    );
    let patch_mod = fs::read_to_string("src/runtime_core/patch/mod.rs").unwrap();
    for child in [
        "approval",
        "application",
        "intent",
        "proposal",
        "verification",
    ] {
        let expected = format!("pub(crate) mod {child};");
        assert!(
            patch_mod.lines().any(|line| line == expected),
            "patch child is not crate-private: {child}"
        );
    }

    for (owner, rules) in [
        (
            "src/runtime_core/patch/approval.rs",
            ["fn token_from_entropy", "fn hash_token", "fn matches_hash"].as_slice(),
        ),
        (
            "src/runtime_core/patch/application.rs",
            [
                "enum ApplyAdmission",
                "fn admit_apply",
                "fn admit_rollback",
                "fn validate_applied_source",
            ]
            .as_slice(),
        ),
        (
            "src/runtime_core/patch/intent.rs",
            ["struct IntentDecision", "fn plan_action_candidate"].as_slice(),
        ),
        (
            "src/runtime_core/patch/intent/classification.rs",
            ["fn classify", "fn detect_constraints"].as_slice(),
        ),
        (
            "src/runtime_core/patch/intent/model_action.rs",
            ["fn parse_model_action", "fn fallback_model_action"].as_slice(),
        ),
        (
            "src/runtime_core/patch/proposal.rs",
            [
                "struct PatchPreview",
                "fn build_preview",
                "fn render_record",
                "fn parse_record",
            ]
            .as_slice(),
        ),
        (
            "src/runtime_core/patch/verification.rs",
            [
                "struct VerificationPlan",
                "enum RecoveryAdmission",
                "fn build_plan",
                "fn recovery_admission",
            ]
            .as_slice(),
        ),
    ] {
        let source = fs::read_to_string(owner).unwrap();
        for rule in rules {
            assert!(
                source.contains(rule),
                "v0.37.9 owner is missing lifecycle rule: {owner} -> {rule}"
            );
        }
        for forbidden in [
            "crate::adapters",
            "crate::ledger",
            "crate::state",
            "crate::runtime::",
            "crate::skill",
            "std::fs",
            "std::process",
        ] {
            assert!(
                !source.contains(forbidden),
                "patch owner has concrete reverse dependency: {owner} -> {forbidden}"
            );
        }
    }

    for (facade, forbidden) in [
        ("src/app/intent_adapter.rs", "struct IntentDecision"),
        ("src/app/intent_adapter.rs", "fn plan_action_candidate"),
        ("src/app/intent_adapter.rs", "fn parse_model_action"),
        ("src/app/patch_adapter.rs", "struct PatchPreview"),
        ("src/app/patch_adapter.rs", "struct ProposalRecord"),
        ("src/app/patch_adapter.rs", "struct ApplyResult"),
        ("src/app/patch_adapter.rs", "struct RollbackResult"),
        ("src/app/patch_adapter.rs", "struct VerificationPlan"),
        ("src/app/patch_adapter.rs", "struct VerificationResult"),
        ("src/app/patch_adapter.rs", "fn render_unified_diff"),
        ("src/app/patch_adapter.rs", "fn parse_proposal_header"),
        ("src/app/patch_adapter.rs", "fn constant_time_eq"),
        ("src/app/patch_adapter.rs", "fn is_test_verification"),
        ("src/app/patch_adapter.rs", "fn output_excerpt"),
    ] {
        let source = fs::read_to_string(facade).unwrap();
        let production = source.split("#[cfg(test)]").next().unwrap_or(&source);
        assert!(
            !production.contains(forbidden),
            "legacy facade retains moved patch rule: {facade} -> {forbidden}"
        );
    }

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
    assert!(
        patch_contract.contains("fn happy_path_is_restart_safe_and_reports_korean"),
        "patch lifecycle contract was not moved to its owner"
    );
}

#[test]
fn v03710_runtime_and_reporting_owners_hold_dispatch_and_output_decisions() {
    let korean_guard = "src/runtime_core/reporting/korean_guard.rs";
    let korean_classification = "src/runtime_core/reporting/korean_guard/classification.rs";
    let korean_language = "src/runtime_core/reporting/korean_guard/language.rs";
    let korean_projection = "src/runtime_core/reporting/korean_guard/projection.rs";
    let korean_streaming = "src/runtime_core/reporting/korean_guard/streaming.rs";
    let korean_tests = "src/runtime_core/reporting/korean_guard/tests.rs";
    let runtime_report = "src/runtime_core/reporting/runtime_report.rs";
    let runner = "src/runtime_core/workflow/application/runner.rs";
    for target in [
        korean_guard,
        korean_classification,
        korean_language,
        korean_projection,
        korean_streaming,
        korean_tests,
        runtime_report,
        runner,
    ] {
        assert!(
            Path::new(target).is_file(),
            "missing v0.37.10 runtime owner: {target}"
        );
    }

    let runtime_core = fs::read_to_string("src/runtime_core/mod.rs").unwrap();
    assert!(
        runtime_core
            .lines()
            .any(|line| line == "pub(crate) mod reporting;"),
        "reporting runtime owner is not crate-private"
    );
    let reporting_mod = fs::read_to_string("src/runtime_core/reporting/mod.rs").unwrap();
    for child in ["korean_guard", "runtime_report"] {
        let expected = format!("pub(crate) mod {child};");
        assert!(
            reporting_mod.lines().any(|line| line == expected),
            "reporting child is not crate-private: {child}"
        );
    }
    let application_mod =
        fs::read_to_string("src/runtime_core/workflow/application/mod.rs").unwrap();
    assert!(
        application_mod
            .lines()
            .any(|line| line == "pub(crate) mod runner;"),
        "workflow application runner is not crate-private"
    );

    for (owner, rules) in [
        (
            korean_guard,
            [
                "pub use streaming::StreamingGuard",
                "fn guard_or_failure",
                "fn validate",
            ]
            .as_slice(),
        ),
        (
            korean_classification,
            [
                "struct OutsideTextClassification",
                "fn classify_outside_text",
            ]
            .as_slice(),
        ),
        (korean_language, ["fn allows_non_korean"].as_slice()),
        (korean_projection, ["fn stricter_projection"].as_slice()),
        (korean_streaming, ["struct StreamingGuard"].as_slice()),
        (
            runtime_report,
            [
                "struct WorkflowResumeReport",
                "struct SessionResumeReport",
                "struct InitReport",
                "struct DoctorReport",
                "fn render_workflow_resume",
                "fn render_session_resume",
                "fn guard_patch_terminal",
                "fn render_init",
                "fn render_doctor",
            ]
            .as_slice(),
        ),
        (
            runner,
            [
                "trait RuntimeApplicationPort",
                "fn agent_run_report",
                "fn workflow_resume_report",
                "fn session_resume_report",
                "fn patch_approve_to_stdout",
                "fn patch_verify_report",
            ]
            .as_slice(),
        ),
    ] {
        let source = fs::read_to_string(owner).unwrap();
        for rule in rules {
            assert!(
                source.contains(rule),
                "v0.37.10 owner is missing runtime rule: {owner} -> {rule}"
            );
        }
        for forbidden in [
            "crate::adapters",
            "crate::backend",
            "crate::context",
            "crate::intent",
            "crate::ledger",
            "crate::model",
            "crate::ontology",
            "crate::patch",
            "crate::state",
            "std::env",
            "std::fs",
            "std::process",
        ] {
            assert!(
                !source.contains(forbidden),
                "runtime owner has concrete reverse dependency: {owner} -> {forbidden}"
            );
        }
    }

    assert!(!Path::new("src/korean_guard.rs").exists());
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(!main.lines().any(|line| line == "mod korean_guard;"));

    let runtime_facade_path = "src/app/runtime_adapter.rs";
    let runtime_tests_path = "src/app/runtime_adapter/tests.rs";
    assert!(Path::new(runtime_facade_path).is_file());
    assert!(Path::new(runtime_tests_path).is_file());
    assert!(!Path::new("src/runtime.rs").exists());
    assert!(!Path::new("src/runtime").exists());
    assert!(!main.lines().any(|line| line == "mod runtime;"));
    let app_root = fs::read_to_string("src/app.rs").unwrap();
    assert!(
        app_root
            .lines()
            .any(|line| line == "pub(crate) mod runtime_adapter;"),
        "application root does not register the runtime adapter"
    );
    let runtime_facade = fs::read_to_string(runtime_facade_path).unwrap();
    let runtime_tests = fs::read_to_string(runtime_tests_path).unwrap();
    let production = &runtime_facade;
    assert!(
        runtime_facade.contains("#[path = \"runtime_adapter/tests.rs\"]"),
        "runtime facade does not register its regression-test owner"
    );
    for forbidden in [
        "fn guard_patch_terminal_report",
        "fn release_smoke_summary",
        "rpotato 진단\\n- CLI",
        "{}\\n- reconstructed context: {}",
    ] {
        assert!(
            !production.contains(forbidden),
            "legacy runtime facade retains moved report rule: {forbidden}"
        );
    }
    for delegation in [
        "impl RuntimeApplicationPort for RuntimeApplicationAdapter",
        "runner::workflow_resume_report",
        "runner::session_resume_report",
        "runner::patch_approve_to_stdout",
        "runner::patch_verify_report",
        "runtime_report::render_init",
        "runtime_report::render_doctor",
    ] {
        assert!(
            production.contains(delegation),
            "legacy runtime facade is missing owner delegation: {delegation}"
        );
    }
    for regression in [
        "fn tui_read_facade_is_bounded_fresh_and_non_mutating_with_tool_output(",
        "fn tui_read_facade_all_views_are_canonical_bounded_fresh_and_non_mutating(",
        "fn runtime_tui_outcome_oracle_all_families_exact_utf8(",
        "fn doctor_report_includes_release_smoke_fields(",
    ] {
        assert!(
            runtime_tests.contains(regression),
            "runtime regression owner is missing: {regression}"
        );
        assert!(
            !runtime_facade.contains(regression),
            "runtime facade still owns regression test: {regression}"
        );
    }
    assert!(
        runtime_facade.lines().count() < 200,
        "runtime facade regrew beyond the v0.37.10 boundary"
    );
    assert!(
        runtime_tests.lines().count() < 1_100,
        "runtime regression module regrew beyond its ownership boundary"
    );
}
