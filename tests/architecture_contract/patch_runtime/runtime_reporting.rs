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
    let runtime_test_children = [
        "src/app/runtime_adapter/tests/support.rs",
        "src/app/runtime_adapter/tests/read_views.rs",
        "src/app/runtime_adapter/tests/outcome_matrix.rs",
        "src/app/runtime_adapter/tests/outcome_contract.rs",
        "src/app/runtime_adapter/tests/reports.rs",
    ];
    assert!(Path::new(runtime_facade_path).is_file());
    assert!(Path::new(runtime_tests_path).is_file());
    for path in runtime_test_children {
        assert!(
            Path::new(path).is_file(),
            "runtime test owner is missing: {path}"
        );
    }
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
    let runtime_test_sources = runtime_test_children
        .into_iter()
        .map(|path| (path, fs::read_to_string(path).unwrap()))
        .collect::<Vec<_>>();
    let production = &runtime_facade;
    assert!(
        runtime_facade.contains("#[path = \"runtime_adapter/tests.rs\"]"),
        "runtime facade does not register its regression-test owner"
    );
    for include in [
        "include!(\"tests/support.rs\");",
        "include!(\"tests/read_views.rs\");",
        "include!(\"tests/outcome_matrix.rs\");",
        "include!(\"tests/outcome_contract.rs\");",
        "include!(\"tests/reports.rs\");",
    ] {
        assert!(
            runtime_tests.lines().any(|line| line == include),
            "runtime regression facade does not register child owner: {include}"
        );
    }
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
            runtime_test_sources
                .iter()
                .any(|(_, source)| source.contains(regression)),
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
        runtime_tests.lines().count() < 100,
        "runtime regression facade regrew beyond its ownership boundary"
    );
    for (path, source) in runtime_test_sources {
        assert!(
            source.lines().count() < 550,
            "runtime regression child regrew beyond its ownership boundary: {path}"
        );
    }
}
