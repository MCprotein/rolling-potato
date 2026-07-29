fn assert_application_backend_owners() {
    let backend_adapter_path = "src/app/inference_adapter/backend.rs";
    let backend_generation_state_path = "src/app/inference_adapter/backend/generation_state.rs";
    let backend_installation_path = "src/app/inference_adapter/backend/installation.rs";
    let backend_resource_sampling_path = "src/app/inference_adapter/backend/resource_sampling.rs";
    let backend_runtime_snapshot_path = "src/app/inference_adapter/backend/runtime_snapshot.rs";
    let backend_sidecar_path = "src/app/inference_adapter/backend/sidecar.rs";
    let backend_sidecar_startup_path = "src/app/inference_adapter/backend/sidecar/startup.rs";
    let backend_state_path = "src/adapters/filesystem/backend_state.rs";
    let backend_tests_path = "src/app/inference_adapter/backend/tests.rs";
    let backend_tests_termination_path = "src/app/inference_adapter/backend/tests/termination.rs";
    let backend_tests_discovery_path = "src/app/inference_adapter/backend/tests/discovery.rs";
    let backend_tests_installation_path = "src/app/inference_adapter/backend/tests/installation.rs";
    let backend_tests_records_path = "src/app/inference_adapter/backend/tests/records.rs";
    let backend_tests_generation_path = "src/app/inference_adapter/backend/tests/generation.rs";
    let backend_tests_lifecycle_path = "src/app/inference_adapter/backend/tests/lifecycle.rs";
    let backend_tests_diagnostics_path = "src/app/inference_adapter/backend/tests/diagnostics.rs";
    let context_window_path = "src/app/inference_adapter/context_window.rs";
    assert!(Path::new(backend_generation_state_path).is_file());
    assert!(Path::new(backend_installation_path).is_file());
    assert!(Path::new(backend_resource_sampling_path).is_file());
    assert!(Path::new(backend_runtime_snapshot_path).is_file());
    assert!(Path::new(backend_sidecar_path).is_file());
    assert!(Path::new(backend_sidecar_startup_path).is_file());
    assert!(Path::new(backend_tests_path).is_file());
    assert!(Path::new(backend_tests_termination_path).is_file());
    assert!(Path::new(backend_tests_discovery_path).is_file());
    assert!(Path::new(backend_tests_installation_path).is_file());
    assert!(Path::new(backend_tests_records_path).is_file());
    assert!(Path::new(backend_tests_generation_path).is_file());
    assert!(Path::new(backend_tests_lifecycle_path).is_file());
    assert!(Path::new(backend_tests_diagnostics_path).is_file());
    assert!(Path::new(context_window_path).is_file());
    let backend_adapter = fs::read_to_string(backend_adapter_path).unwrap();
    let backend_generation_state = fs::read_to_string(backend_generation_state_path).unwrap();
    let backend_installation = fs::read_to_string(backend_installation_path).unwrap();
    let backend_resource_sampling = fs::read_to_string(backend_resource_sampling_path).unwrap();
    let backend_runtime_snapshot = fs::read_to_string(backend_runtime_snapshot_path).unwrap();
    let backend_sidecar = fs::read_to_string(backend_sidecar_path).unwrap();
    let backend_sidecar_startup = fs::read_to_string(backend_sidecar_startup_path).unwrap();
    let backend_state = fs::read_to_string(backend_state_path).unwrap();
    let backend_tests = fs::read_to_string(backend_tests_path).unwrap();
    let backend_tests_termination = fs::read_to_string(backend_tests_termination_path).unwrap();
    let backend_tests_discovery = fs::read_to_string(backend_tests_discovery_path).unwrap();
    let backend_tests_installation = fs::read_to_string(backend_tests_installation_path).unwrap();
    let backend_tests_records = fs::read_to_string(backend_tests_records_path).unwrap();
    let backend_tests_generation = fs::read_to_string(backend_tests_generation_path).unwrap();
    let backend_tests_lifecycle = fs::read_to_string(backend_tests_lifecycle_path).unwrap();
    let backend_tests_diagnostics = fs::read_to_string(backend_tests_diagnostics_path).unwrap();
    let inference_facade = fs::read_to_string("src/app/inference_adapter.rs").unwrap();
    let context_window = fs::read_to_string(context_window_path).unwrap();
    assert!(
        backend_adapter.contains("#[path = \"backend/tests.rs\"]"),
        "inference backend adapter does not register its regression-test owner"
    );
    for owner in [
        "termination",
        "discovery",
        "installation",
        "records",
        "generation",
        "lifecycle",
        "diagnostics",
    ] {
        assert!(
            backend_tests.contains(&format!("include!(\"tests/{owner}.rs\");")),
            "inference backend regression facade does not register its {owner} owner"
        );
    }
    assert!(
        inference_facade
            .lines()
            .any(|line| line == "pub(crate) mod context_window;"),
        "inference adapter does not register its context-window owner"
    );
    for responsibility in [
        "pub(crate) struct EffectiveContextWindow",
        "pub(crate) fn effective_context_window(",
        "fn active_ready_backend_owns_the_effective_context_window(",
        "fn incomplete_or_inactive_runtime_uses_the_configured_manifest(",
    ] {
        assert!(
            context_window.contains(responsibility),
            "context-window owner is missing: {responsibility}"
        );
    }
    application_backend_chat::assert_backend_chat_owners(&backend_adapter);
    assert!(
        backend_adapter
            .lines()
            .any(|line| line == "mod generation_state;"),
        "inference backend adapter does not register its generation-state owner"
    );
    assert!(
        backend_adapter
            .lines()
            .any(|line| line == "mod installation;"),
        "inference backend adapter does not register its installation owner"
    );
    assert!(
        backend_adapter
            .lines()
            .any(|line| line == "mod resource_sampling;"),
        "inference backend adapter does not register its resource-sampling owner"
    );
    assert!(
        backend_adapter
            .lines()
            .any(|line| line == "mod runtime_snapshot;"),
        "inference backend adapter does not register its runtime-snapshot owner"
    );
    for responsibility in [
        "pub(crate) struct BackendRuntimeSnapshot",
        "pub(crate) fn runtime_snapshot(",
    ] {
        assert!(
            backend_runtime_snapshot.contains(responsibility),
            "backend runtime-snapshot owner is missing: {responsibility}"
        );
        assert!(
            !backend_adapter.contains(responsibility),
            "inference backend facade still owns runtime snapshot: {responsibility}"
        );
    }
    assert!(
        backend_adapter.lines().any(|line| line == "mod sidecar;"),
        "inference backend adapter does not register its sidecar owner"
    );
    assert!(
        backend_sidecar.lines().any(|line| line == "mod startup;"),
        "inference backend sidecar owner does not register its startup owner"
    );
    for responsibility in [
        "pub(super) struct ActiveGenerationGuard",
        "pub(super) fn begin_active_generation(",
        "pub(super) fn write_backend_generation_record(",
        "pub(super) fn generation_cancel_requested(",
        "pub(super) fn write_generation_cancel_marker(",
        "pub(super) fn write_generation_terminal_record(",
        "pub(super) fn wait_for_generation_terminal(",
        "pub(super) fn release_generation_admission(",
    ] {
        assert!(
            backend_generation_state.contains(responsibility),
            "inference backend generation-state owner is missing: {responsibility}"
        );
        assert!(
            !backend_adapter.contains(responsibility),
            "inference backend facade still owns generation state: {responsibility}"
        );
    }
    for responsibility in [
        "pub fn install_plan_report(",
        "pub fn install_report(",
        "pub fn verify_archive_report(",
        "pub(super) fn install_backend_from_archive(",
    ] {
        assert!(
            backend_installation.contains(responsibility),
            "inference backend installation owner is missing: {responsibility}"
        );
    }
    for responsibility in [
        "pub(super) struct BackendResourceSampleReport",
        "pub(super) fn display_optional_f64(",
        "pub(super) fn display_optional_u64_unknown(",
        "fn backend_resource_paths(",
        "pub(super) fn record_backend_resource_sample(",
    ] {
        assert!(
            backend_resource_sampling.contains(responsibility),
            "inference backend resource-sampling owner is missing: {responsibility}"
        );
        assert!(
            !backend_adapter.contains(responsibility),
            "inference backend facade still owns resource sampling: {responsibility}"
        );
    }
    for responsibility in [
        "pub fn doctor_report(",
        "pub fn start_report(",
        "pub fn status_report(",
        "pub fn stop_report(",
        "pub fn health_check_report(",
        "pub(super) fn terminate_with_fallback(",
        "pub(super) fn cancel_active_generation_before_stop(",
    ] {
        assert!(
            backend_sidecar.contains(responsibility),
            "inference backend sidecar owner is missing: {responsibility}"
        );
        assert!(
            !backend_adapter.contains(responsibility),
            "inference backend facade still owns sidecar lifecycle: {responsibility}"
        );
    }
    for responsibility in [
        "fn start_sidecar_with_timeout(",
        "fn canonical_existing_file(",
    ] {
        assert!(
            backend_sidecar_startup.contains(responsibility),
            "inference backend sidecar startup owner is missing: {responsibility}"
        );
        assert!(
            !backend_sidecar.contains(responsibility),
            "inference backend sidecar lifecycle still owns startup: {responsibility}"
        );
    }
    assert!(
        backend_sidecar.contains("fn trace_backend_start("),
        "inference backend sidecar lifecycle is missing startup tracing"
    );
    assert!(
        !backend_sidecar_startup.contains("fn trace_backend_start("),
        "inference backend startup orchestration still owns lifecycle tracing"
    );
    assert!(
        backend_state.contains("fn create_log_file("),
        "backend filesystem adapter is missing exclusive log creation"
    );
    assert!(
        !backend_sidecar_startup.contains("fn create_log_file("),
        "inference backend startup orchestration still owns log persistence"
    );
    for (owner, responsibility) in [
        (
            &backend_tests_termination,
            "fn termination_fallback_forces_a_process_after_graceful_command_failure(",
        ),
        (
            &backend_tests_discovery,
            "fn default_discovery_uses_managed_path(",
        ),
        (
            &backend_tests_installation,
            "fn release_manifest_has_source_backed_supported_artifacts(",
        ),
        (
            &backend_tests_records,
            "fn generation_record_codec_preserves_exact_bytes_and_round_trips(",
        ),
        (
            &backend_tests_generation,
            "fn parallel_generation_cancel_reaches_secondary_and_keeps_state_until_last_release(",
        ),
        (
            &backend_tests_lifecycle,
            "fn start_timeout_removes_record_and_keeps_logs(",
        ),
        (
            &backend_tests_diagnostics,
            "fn health_check_report_is_diagnostic_not_process_start(",
        ),
    ] {
        assert!(
            owner.contains(responsibility),
            "inference backend regression owner is missing: {responsibility}"
        );
        assert!(
            !backend_tests.contains(responsibility),
            "inference backend regression facade still owns: {responsibility}"
        );
    }
    assert!(
        backend_adapter.lines().count() < 125,
        "inference backend production adapter regrew beyond its resource-sampling extraction boundary"
    );
    assert!(context_window.lines().count() < 100);
    assert!(backend_runtime_snapshot.lines().count() < 75);
    assert!(
        backend_generation_state.lines().count() < 250,
        "inference backend generation-state module regrew beyond its ownership boundary"
    );
    assert!(
        backend_installation.lines().count() < 225,
        "inference backend installation module regrew beyond its ownership boundary"
    );
    assert!(
        backend_resource_sampling.lines().count() < 110,
        "inference backend resource-sampling module regrew beyond its ownership boundary"
    );
    assert!(
        backend_sidecar.lines().count() < 375,
        "inference backend sidecar module regrew beyond its ownership boundary"
    );
    assert!(
        backend_sidecar_startup.lines().count() < 300,
        "inference backend sidecar startup module regrew beyond its ownership boundary"
    );
    assert!(backend_tests.lines().count() < 75);
    assert!(backend_tests_termination.lines().count() < 100);
    assert!(backend_tests_discovery.lines().count() < 75);
    assert!(backend_tests_installation.lines().count() < 250);
    assert!(backend_tests_records.lines().count() < 150);
    assert!(backend_tests_generation.lines().count() < 325);
    assert!(backend_tests_lifecycle.lines().count() < 150);
    assert!(backend_tests_diagnostics.lines().count() < 50);
}
