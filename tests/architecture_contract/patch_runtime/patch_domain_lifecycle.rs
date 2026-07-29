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

    assert_application_adapter_contracts();
}
