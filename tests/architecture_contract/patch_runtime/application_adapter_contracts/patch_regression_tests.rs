fn assert_patch_regression_test_contracts(patch_facade: &str) {
    let patch_test_modules = [
        "src/app/patch_adapter/tests/mod.rs",
        "src/app/patch_adapter/tests/approval_cases.rs",
        "src/app/patch_adapter/tests/recovery_cases.rs",
        "src/app/patch_adapter/tests/support_cases.rs",
        "src/app/patch_adapter/tests/terminal_cases.rs",
        "src/app/patch_adapter/tests/verification_cases.rs",
    ];
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
