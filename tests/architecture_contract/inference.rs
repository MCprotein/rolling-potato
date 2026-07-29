use super::*;

#[test]
fn v0373_inference_owners_replace_legacy_domain_and_adapter_slices() {
    for target in [
        "src/runtime_core/inference/backend.rs",
        "src/runtime_core/inference/backend/admission.rs",
        "src/runtime_core/inference/backend/lifecycle.rs",
        "src/runtime_core/inference/benchmark.rs",
        "src/runtime_core/inference/benchmark/fixture.rs",
        "src/runtime_core/inference/benchmark/report.rs",
        "src/runtime_core/inference/model.rs",
        "src/runtime_core/inference/model/codec.rs",
        "src/runtime_core/inference/model/manifest.rs",
        "src/runtime_core/inference/model/promotion.rs",
        "src/runtime_core/inference/resource.rs",
        "src/runtime_core/inference/stream.rs",
        "src/adapters/filesystem/backend_state.rs",
        "src/adapters/filesystem/benchmark_artifact.rs",
        "src/adapters/filesystem/model_artifact.rs",
        "src/adapters/filesystem/model_artifact/cache.rs",
        "src/adapters/filesystem/model_artifact/download.rs",
        "src/adapters/filesystem/model_artifact/store.rs",
        "src/adapters/llama_cpp/backend.rs",
        "src/adapters/llama_cpp/backend/discovery.rs",
        "src/adapters/llama_cpp/backend/health.rs",
        "src/adapters/llama_cpp/backend/request.rs",
        "src/adapters/llama_cpp/backend/sidecar.rs",
        "src/adapters/llama_cpp/backend/tests.rs",
        "src/adapters/llama_cpp/backend/version.rs",
        "src/adapters/llama_cpp/install.rs",
        "src/adapters/llama_cpp/install/archive.rs",
        "src/adapters/llama_cpp/stream.rs",
        "src/adapters/llama_cpp/stream/protocol.rs",
        "src/adapters/llama_cpp/stream/tests.rs",
        "src/adapters/process/backend.rs",
        "src/adapters/process/resource.rs",
    ] {
        assert!(
            Path::new(target).is_file(),
            "missing inference owner: {target}"
        );
    }
    for legacy in ["src/backend_stream.rs", "src/resource.rs"] {
        assert!(
            !Path::new(legacy).exists(),
            "legacy inference owner remains: {legacy}"
        );
    }

    let model_artifact = fs::read_to_string("src/adapters/filesystem/model_artifact.rs").unwrap();
    let model_cache =
        fs::read_to_string("src/adapters/filesystem/model_artifact/cache.rs").unwrap();
    let model_download =
        fs::read_to_string("src/adapters/filesystem/model_artifact/download.rs").unwrap();
    let model_store =
        fs::read_to_string("src/adapters/filesystem/model_artifact/store.rs").unwrap();
    for owner in ["cache", "download", "store"] {
        assert!(
            model_artifact
                .lines()
                .any(|line| line == format!("mod {owner};")),
            "model-artifact facade does not register {owner}"
        );
    }
    assert!(model_cache.contains("pub(crate) fn cleanup_failed_artifacts("));
    assert!(model_cache.contains("pub(crate) fn local_artifact_state("));
    assert!(model_download.contains("pub(crate) fn fetch_evaluation_artifact("));
    assert!(model_download.contains("pub(crate) fn fetch_managed_projector_artifact("));
    assert!(model_store.contains("pub(crate) fn read_registry_entries("));
    assert!(model_store.contains("pub(crate) fn read_default_selection("));
    assert!(model_artifact.lines().count() < 175);
    assert!(model_cache.lines().count() < 250);
    assert!(model_download.lines().count() < 375);
    assert!(model_store.lines().count() < 125);

    let llama_backend = fs::read_to_string("src/adapters/llama_cpp/backend.rs").unwrap();
    let llama_discovery =
        fs::read_to_string("src/adapters/llama_cpp/backend/discovery.rs").unwrap();
    let llama_health = fs::read_to_string("src/adapters/llama_cpp/backend/health.rs").unwrap();
    let llama_request = fs::read_to_string("src/adapters/llama_cpp/backend/request.rs").unwrap();
    let llama_sidecar = fs::read_to_string("src/adapters/llama_cpp/backend/sidecar.rs").unwrap();
    let llama_tests = fs::read_to_string("src/adapters/llama_cpp/backend/tests.rs").unwrap();
    let llama_version = fs::read_to_string("src/adapters/llama_cpp/backend/version.rs").unwrap();
    for owner in ["discovery", "health", "request", "sidecar", "version"] {
        assert!(
            llama_backend
                .lines()
                .any(|line| line == format!("mod {owner};")),
            "llama.cpp backend facade does not register {owner}"
        );
    }
    assert!(llama_discovery.contains("pub(crate) fn discover("));
    assert!(llama_health.contains("pub(crate) fn probe_health("));
    assert!(llama_request.contains("pub(crate) fn chat_request_body_for_input("));
    assert!(llama_sidecar.contains("pub(crate) fn sidecar_command("));
    assert!(llama_version.contains("pub(crate) fn probe_version("));
    assert!(llama_tests.contains("fn multimodal_request_uses_openai_image_content_parts("));
    assert!(llama_backend.lines().count() < 100);
    assert!(llama_discovery.lines().count() < 125);
    assert!(llama_health.lines().count() < 125);
    assert!(llama_request.lines().count() < 225);
    assert!(llama_sidecar.lines().count() < 50);
    assert!(llama_tests.lines().count() < 325);
    assert!(llama_version.lines().count() < 250);

    let install_adapter = fs::read_to_string("src/adapters/llama_cpp/install.rs").unwrap();
    let install_archive = fs::read_to_string("src/adapters/llama_cpp/install/archive.rs").unwrap();
    let install_payload = fs::read_to_string("src/adapters/llama_cpp/install/payload.rs").unwrap();
    assert!(install_adapter.lines().any(|line| line == "mod archive;"));
    assert!(install_adapter.lines().any(|line| line == "mod payload;"));
    let install_orchestration = install_adapter
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(&install_adapter);
    for responsibility in [
        "pub(crate) fn download_archive(",
        "pub(crate) fn verify_archive_file(",
        "fn copy_reader_with_limit<",
    ] {
        assert!(
            install_archive.contains(responsibility),
            "llama.cpp install archive owner is missing: {responsibility}"
        );
        assert!(
            !install_orchestration.contains(responsibility),
            "llama.cpp install orchestration still owns archive transfer: {responsibility}"
        );
    }
    for responsibility in [
        "pub(crate) fn prepare_install(",
        "pub(crate) fn cleanup_staging(",
        "fn extract_archive(",
        "fn find_extracted_binary(",
        "fn collect_binary_matches(",
        "fn place_managed_payload(",
        "fn copy_release_tree(",
        "pub(crate) fn set_executable_bit(",
    ] {
        assert!(
            install_payload.contains(responsibility),
            "llama.cpp install payload owner is missing: {responsibility}"
        );
        assert!(
            !install_orchestration.contains(responsibility),
            "llama.cpp install manifest/record adapter still owns payload placement: {responsibility}"
        );
    }
    assert!(install_adapter.lines().count() < 425);
    assert!(install_archive.lines().count() < 200);
    assert!(install_payload.lines().count() < 375);

    let stream_adapter = fs::read_to_string("src/adapters/llama_cpp/stream.rs").unwrap();
    let stream_protocol = fs::read_to_string("src/adapters/llama_cpp/stream/protocol.rs").unwrap();
    let stream_tests = fs::read_to_string("src/adapters/llama_cpp/stream/tests.rs").unwrap();
    assert!(
        stream_adapter.lines().any(|line| line == "mod protocol;"),
        "llama.cpp stream adapter does not register its protocol owner"
    );
    let stream_transport = stream_adapter
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(&stream_adapter);
    for responsibility in [
        "pub(super) struct HttpResponseDecoder",
        "pub(super) struct ChatSseDecoder",
        "pub(super) struct ReasoningTraceFilter",
        "fn find_sse_event_end(",
        "fn malformed_sse_event(",
    ] {
        assert!(
            stream_protocol.contains(responsibility),
            "llama.cpp stream protocol owner is missing: {responsibility}"
        );
        assert!(
            !stream_transport.contains(responsibility),
            "llama.cpp stream transport still owns protocol behavior: {responsibility}"
        );
    }
    assert!(
        stream_adapter.contains("#[path = \"stream/tests.rs\"]"),
        "llama.cpp stream adapter does not register its regression-test owner"
    );
    for regression in [
        "fn decodes_split_chunked_http_body(",
        "fn rejects_many_valid_events_over_total_completion_limit(",
        "fn streams_chunked_sse_over_tcp(",
        "fn cancellation_interrupts_a_stalled_request_upload(",
        "fn total_timeout_closes_stalled_stream(",
    ] {
        assert!(
            stream_tests.contains(regression),
            "llama.cpp stream regression owner is missing: {regression}"
        );
        assert!(
            !stream_adapter.contains(regression),
            "llama.cpp stream adapter still owns regression test: {regression}"
        );
    }
    assert!(
        stream_adapter.lines().count() < 225,
        "llama.cpp stream adapter regrew beyond its ownership boundary"
    );
    assert!(
        stream_protocol.lines().count() < 450,
        "llama.cpp stream protocol module regrew beyond its ownership boundary"
    );
    assert!(
        stream_tests.lines().count() < 550,
        "llama.cpp stream regression module regrew beyond its ownership boundary"
    );

    let process_mod = fs::read_to_string("src/adapters/process/mod.rs").unwrap();
    let resource_policy = fs::read_to_string("src/runtime_core/inference/resource.rs").unwrap();
    let resource_tests =
        fs::read_to_string("src/runtime_core/inference/resource/tests.rs").unwrap();
    let resource_sampler = fs::read_to_string("src/adapters/process/resource.rs").unwrap();
    assert!(
        process_mod
            .lines()
            .any(|line| line == "pub(crate) mod resource;"),
        "process adapter does not register resource sampler"
    );
    for responsibility in [
        "pub(crate) struct ProcessResourceSnapshot",
        "pub(crate) fn sample_process(",
        "fn process_cpu_and_rss(",
        "fn bounded_command_output(",
        "fn path_disk_bytes(",
    ] {
        assert!(
            resource_sampler.contains(responsibility),
            "process resource sampler is missing: {responsibility}"
        );
        assert!(
            !resource_policy.contains(responsibility),
            "resource policy still owns concrete sampling: {responsibility}"
        );
    }
    for forbidden in ["std::fs", "std::path", "std::process", "std::thread"] {
        assert!(
            !resource_policy.contains(forbidden),
            "resource policy has concrete adapter dependency: {forbidden}"
        );
    }
    assert!(
        resource_policy.contains("#[path = \"resource/tests.rs\"]"),
        "resource policy does not register its regression test owner"
    );
    for regression in [
        "fn classify_pressure_handles_unknown_normal_and_thresholds(",
        "fn chat_governor_allows_clamps_and_blocks_by_pressure(",
        "fn optimization_policy_uses_local_metrics_and_benchmark_evidence(",
    ] {
        assert!(
            resource_tests.contains(regression),
            "resource test owner is missing regression: {regression}"
        );
        assert!(
            !resource_policy.contains(regression),
            "resource policy still owns regression: {regression}"
        );
    }
    assert!(
        resource_policy.lines().count() < 600,
        "resource policy regrew beyond its ownership boundary"
    );
    assert!(
        resource_tests.lines().count() < 200,
        "resource policy regression module regrew beyond its ownership boundary"
    );
    assert!(
        resource_sampler.lines().count() < 300,
        "process resource sampler regrew beyond its ownership boundary"
    );

    let main = fs::read_to_string("src/main.rs").unwrap();
    for legacy_module in ["backend_stream", "resource"] {
        assert!(
            !main
                .lines()
                .any(|line| line == format!("mod {legacy_module};")),
            "legacy inference module remains compile-connected: {legacy_module}"
        );
    }

    for (facade, moved_definition) in [
        (
            "src/app/inference_adapter/backend.rs",
            "struct BackendSidecarRecord",
        ),
        (
            "src/app/inference_adapter/benchmark.rs",
            "struct BenchmarkFixture",
        ),
        ("src/app/inference_adapter/model.rs", "const CANDIDATES"),
    ] {
        let source = fs::read_to_string(facade).unwrap();
        assert!(
            !source.contains(moved_definition),
            "legacy facade still owns moved definition: {facade} -> {moved_definition}"
        );
    }

    let backend_adapter_path = "src/app/inference_adapter/backend.rs";
    let backend_chat_path = "src/app/inference_adapter/backend/chat.rs";
    let backend_chat_interruption_path = "src/app/inference_adapter/backend/chat/interruption.rs";
    let backend_chat_report_path = "src/app/inference_adapter/backend/chat/report.rs";
    let backend_generation_state_path = "src/app/inference_adapter/backend/generation_state.rs";
    let backend_installation_path = "src/app/inference_adapter/backend/installation.rs";
    let backend_resource_sampling_path = "src/app/inference_adapter/backend/resource_sampling.rs";
    let backend_runtime_snapshot_path = "src/app/inference_adapter/backend/runtime_snapshot.rs";
    let backend_sidecar_path = "src/app/inference_adapter/backend/sidecar.rs";
    let backend_sidecar_startup_path = "src/app/inference_adapter/backend/sidecar/startup.rs";
    let backend_state_path = "src/adapters/filesystem/backend_state.rs";
    let backend_tests_path = "src/app/inference_adapter/backend/tests.rs";
    let context_window_path = "src/app/inference_adapter/context_window.rs";
    let model_adapter_path = "src/app/inference_adapter/model.rs";
    let model_evidence_path = "src/app/inference_adapter/model/evidence.rs";
    let model_registry_path = "src/app/inference_adapter/model/registry.rs";
    let model_registry_vision_path = "src/app/inference_adapter/model/registry/vision.rs";
    let model_registry_vision_preparation_path =
        "src/app/inference_adapter/model/registry/vision/preparation.rs";
    let model_registry_vision_preparation_tests_path =
        "src/app/inference_adapter/model/registry/vision/preparation/tests.rs";
    let model_default_selection_path =
        "src/app/inference_adapter/model/registry/default_selection.rs";
    let model_setup_path = "src/app/inference_adapter/model/setup.rs";
    let model_setup_catalog_path = "src/app/inference_adapter/model/setup/catalog.rs";
    let model_runtime_spec_path = "src/app/inference_adapter/model/setup/runtime_spec.rs";
    let model_setup_tests_path = "src/app/inference_adapter/model/setup/tests.rs";
    let model_tests_path = "src/app/inference_adapter/model/tests.rs";
    assert!(Path::new(backend_chat_path).is_file());
    assert!(Path::new(backend_chat_interruption_path).is_file());
    assert!(Path::new(backend_chat_report_path).is_file());
    assert!(Path::new(backend_generation_state_path).is_file());
    assert!(Path::new(backend_installation_path).is_file());
    assert!(Path::new(backend_resource_sampling_path).is_file());
    assert!(Path::new(backend_runtime_snapshot_path).is_file());
    assert!(Path::new(backend_sidecar_path).is_file());
    assert!(Path::new(backend_sidecar_startup_path).is_file());
    assert!(Path::new(backend_tests_path).is_file());
    assert!(Path::new(context_window_path).is_file());
    assert!(Path::new(model_evidence_path).is_file());
    assert!(Path::new(model_registry_path).is_file());
    assert!(Path::new(model_registry_vision_path).is_file());
    assert!(Path::new(model_registry_vision_preparation_path).is_file());
    assert!(Path::new(model_registry_vision_preparation_tests_path).is_file());
    assert!(Path::new(model_default_selection_path).is_file());
    assert!(Path::new(model_setup_path).is_file());
    assert!(Path::new(model_setup_catalog_path).is_file());
    assert!(Path::new(model_runtime_spec_path).is_file());
    assert!(Path::new(model_setup_tests_path).is_file());
    assert!(Path::new(model_tests_path).is_file());
    let backend_adapter = fs::read_to_string(backend_adapter_path).unwrap();
    let backend_chat = fs::read_to_string(backend_chat_path).unwrap();
    let backend_chat_interruption = fs::read_to_string(backend_chat_interruption_path).unwrap();
    let backend_chat_report = fs::read_to_string(backend_chat_report_path).unwrap();
    let backend_generation_state = fs::read_to_string(backend_generation_state_path).unwrap();
    let backend_installation = fs::read_to_string(backend_installation_path).unwrap();
    let backend_resource_sampling = fs::read_to_string(backend_resource_sampling_path).unwrap();
    let backend_runtime_snapshot = fs::read_to_string(backend_runtime_snapshot_path).unwrap();
    let backend_sidecar = fs::read_to_string(backend_sidecar_path).unwrap();
    let backend_sidecar_startup = fs::read_to_string(backend_sidecar_startup_path).unwrap();
    let backend_state = fs::read_to_string(backend_state_path).unwrap();
    let backend_tests = fs::read_to_string(backend_tests_path).unwrap();
    let inference_facade = fs::read_to_string("src/app/inference_adapter.rs").unwrap();
    let context_window = fs::read_to_string(context_window_path).unwrap();
    let model_adapter = fs::read_to_string(model_adapter_path).unwrap();
    let model_evidence = fs::read_to_string(model_evidence_path).unwrap();
    let model_registry = fs::read_to_string(model_registry_path).unwrap();
    let model_registry_vision = fs::read_to_string(model_registry_vision_path).unwrap();
    let model_registry_vision_preparation =
        fs::read_to_string(model_registry_vision_preparation_path).unwrap();
    let model_registry_vision_preparation_tests =
        fs::read_to_string(model_registry_vision_preparation_tests_path).unwrap();
    let model_default_selection = fs::read_to_string(model_default_selection_path).unwrap();
    let model_setup = fs::read_to_string(model_setup_path).unwrap();
    let model_setup_catalog = fs::read_to_string(model_setup_catalog_path).unwrap();
    let model_runtime_spec = fs::read_to_string(model_runtime_spec_path).unwrap();
    let model_tests = fs::read_to_string(model_tests_path).unwrap();
    assert!(
        backend_adapter.contains("#[path = \"backend/tests.rs\"]"),
        "inference backend adapter does not register its regression-test owner"
    );
    assert!(
        model_adapter.contains("#[path = \"model/tests.rs\"]"),
        "model adapter does not register its regression-test owner"
    );
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
    assert!(
        model_adapter.lines().any(|line| line == "mod evidence;"),
        "model adapter does not register its local evidence owner"
    );
    assert!(
        model_registry.lines().any(|line| line == "mod vision;"),
        "model registry does not register its vision owner"
    );
    assert!(
        model_registry_vision
            .lines()
            .any(|line| line == "mod preparation;"),
        "model registry vision owner does not register its lazy preparation owner"
    );
    for responsibility in [
        "pub(crate) struct VerifiedVisionProjector",
        "pub(crate) fn verified_vision_projector(",
        "pub(super) fn local_registry_vision(",
    ] {
        assert!(
            model_registry_vision.contains(responsibility),
            "model registry vision owner is missing: {responsibility}"
        );
        assert!(
            !model_registry.contains(responsibility),
            "model registry facade still owns vision verification: {responsibility}"
        );
    }
    assert!(
        model_registry_vision_preparation.contains("pub(crate) fn prepare_bound_vision_projector("),
        "model registry vision preparation owner is missing its projector preparation"
    );
    for forbidden in [
        "fetch_managed_projector_artifact",
        "require_declared_projector",
        "vision_projector_artifact_path",
    ] {
        assert!(
            !model_setup.contains(forbidden),
            "base model setup must not own eager projector preparation: {forbidden}"
        );
    }
    assert!(
        !model_adapter.contains("fetch_managed_projector_artifact"),
        "model evaluation fetch must not eagerly download an optional projector"
    );
    for regression in [
        "model_upgrade_compatibility_image_use_migrates_v1_binding_and_preserves_state",
        "model_upgrade_compatibility_preparation_failure_preserves_registry_default_and_backend",
    ] {
        assert!(
            model_registry_vision_preparation_tests.contains(regression),
            "projector preparation regression owner is missing: {regression}"
        );
    }
    for responsibility in [
        "pub(super) fn local_benchmark_status(",
        "pub(super) fn local_promotion_readiness(",
        "pub(super) fn promotion_benchmark_run(",
        "pub(super) fn promotion_benchmark_evidence(",
        "pub(super) fn backend_smoke_evidence(",
        "pub(super) fn persist_promotion_evidence(",
        "pub(super) fn read_promotion_evidence_file(",
    ] {
        assert!(
            model_evidence.contains(responsibility),
            "model local evidence owner is missing: {responsibility}"
        );
        assert!(
            !model_adapter.contains(responsibility),
            "model adapter still owns local evidence collection: {responsibility}"
        );
    }
    assert!(
        model_adapter.lines().any(|line| line == "mod registry;"),
        "model adapter does not register its registry owner"
    );
    assert!(
        model_adapter.lines().any(|line| line == "mod setup;"),
        "model adapter does not register its setup owner"
    );
    assert!(model_setup.lines().any(|line| line == "mod catalog;"));
    assert!(model_setup.lines().any(|line| line == "mod runtime_spec;"));
    assert!(model_setup.lines().any(|line| line == "mod tests;"));
    assert!(model_setup_catalog.contains("pub(super) fn setup_options("));
    for responsibility in [
        "pub(crate) struct PreparedSetupModel",
        "pub(crate) fn setup_options(",
        "pub(crate) fn prepare_setup_model(",
        "pub(crate) fn activate_setup_model(",
    ] {
        assert!(
            model_setup.contains(responsibility),
            "model setup owner is missing: {responsibility}"
        );
        assert!(
            !model_adapter.contains(responsibility),
            "model adapter still owns interactive setup: {responsibility}"
        );
    }
    for responsibility in [
        "pub(crate) struct ConfiguredRuntimeSpec",
        "pub(crate) fn configured_runtime_spec(",
        "pub(crate) fn configured_vision_runtime_spec(",
    ] {
        assert!(
            model_runtime_spec.contains(responsibility),
            "configured runtime specification owner is missing: {responsibility}"
        );
        assert!(
            !model_setup.contains(responsibility),
            "interactive setup still owns configured runtime validation: {responsibility}"
        );
    }
    assert!(
        model_registry
            .lines()
            .any(|line| line == "mod default_selection;"),
        "model registry does not register its default-selection owner"
    );
    for responsibility in [
        "pub(crate) struct DefaultSelectionSnapshot",
        "pub(crate) fn snapshot_default_selection(",
        "pub(crate) fn restore_default_selection(",
    ] {
        assert!(
            model_default_selection.contains(responsibility),
            "default-selection owner is missing: {responsibility}"
        );
        assert!(
            !model_registry.contains(responsibility),
            "model registry still owns default-selection rollback: {responsibility}"
        );
    }
    for responsibility in [
        "pub fn registry_report(",
        "pub fn default_report(",
        "pub fn set_default_report(",
        "pub fn default_artifact_path(",
        "pub fn install_candidate(",
        "fn validated_registry_entry(",
        "pub(super) fn registry_entry_json(",
    ] {
        assert!(
            model_registry.contains(responsibility),
            "model registry owner is missing: {responsibility}"
        );
        assert!(
            !model_adapter.contains(responsibility),
            "model adapter still owns registry behavior: {responsibility}"
        );
    }
    assert!(
        backend_adapter.lines().any(|line| line == "mod chat;"),
        "inference backend adapter does not register its chat owner"
    );
    assert!(
        backend_chat.lines().any(|line| line == "mod report;"),
        "inference backend chat owner does not register its report owner"
    );
    assert!(
        backend_chat.lines().any(|line| line == "mod interruption;"),
        "inference backend chat owner does not register its interruption owner"
    );
    for responsibility in [
        "pub fn chat_report(",
        "pub fn chat_stream_report(",
        "fn format_chat_run(",
    ] {
        assert!(
            backend_chat_report.contains(responsibility),
            "inference backend chat report owner is missing: {responsibility}"
        );
        assert!(
            !backend_chat.contains(responsibility),
            "inference backend chat execution still owns reporting: {responsibility}"
        );
    }
    assert!(backend_chat_report
        .contains("fn chat_report_format_preserves_diagnostics_and_response_boundary("));
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
        "pub fn chat_once(",
        "pub fn chat_once_bounded(",
        "pub fn chat_once_bounded_with_cancel(",
        "pub fn preflight_chat_ready(",
        "fn ready_sidecar_record(",
        "fn chat_once_with_options(",
    ] {
        assert!(
            backend_chat.contains(responsibility),
            "inference backend chat owner is missing: {responsibility}"
        );
        assert!(
            !backend_adapter.contains(responsibility),
            "inference backend facade still owns chat execution: {responsibility}"
        );
    }
    for responsibility in [
        "pub fn cancel_generation_report(",
        "pub(super) fn finish_interrupted_generation(",
    ] {
        assert!(
            backend_chat_interruption.contains(responsibility),
            "inference backend chat interruption owner is missing: {responsibility}"
        );
        assert!(
            !backend_chat.contains(responsibility),
            "inference backend chat execution still owns interruption behavior: {responsibility}"
        );
    }
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
    for responsibility in [
        "fn release_manifest_has_source_backed_supported_artifacts(",
        "fn generation_record_codec_preserves_exact_bytes_and_round_trips(",
        "fn parallel_generation_cancel_reaches_secondary_and_keeps_state_until_last_release(",
        "fn start_timeout_removes_record_and_keeps_logs(",
    ] {
        assert!(
            backend_tests.contains(responsibility),
            "inference backend regression owner is missing: {responsibility}"
        );
    }
    for responsibility in [
        "fn manifest_validation_blocks_unverified_artifact_candidate(",
        "fn promotion_evidence_validation_accepts_measured_local_benchmark(",
        "fn registry_promotion_binding_rejects_backend_and_benchmark_drift(",
        "fn cleanup_failed_dry_run_lists_app_managed_paths(",
    ] {
        assert!(
            model_tests.contains(responsibility),
            "model regression owner is missing: {responsibility}"
        );
        assert!(
            !model_adapter.contains(responsibility),
            "model adapter still owns regression test: {responsibility}"
        );
    }
    assert!(
        backend_adapter.lines().count() < 125,
        "inference backend production adapter regrew beyond its resource-sampling extraction boundary"
    );
    assert!(context_window.lines().count() < 100);
    assert!(backend_runtime_snapshot.lines().count() < 75);
    assert!(
        backend_chat.lines().count() < 500,
        "inference backend chat module regrew beyond its interruption extraction boundary"
    );
    assert!(
        backend_chat_interruption.lines().count() < 225,
        "inference backend chat interruption module regrew beyond its ownership boundary"
    );
    assert!(
        backend_chat_report.lines().count() < 200,
        "inference backend chat report module regrew beyond its ownership boundary"
    );
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
    assert!(
        backend_tests.lines().count() < 900,
        "inference backend regression module regrew beyond its ownership boundary"
    );
    assert!(
        model_adapter.lines().count() < 550,
        "model adapter regrew beyond its local evidence extraction boundary"
    );
    assert!(
        model_evidence.lines().count() < 250,
        "model local evidence module regrew beyond its ownership boundary"
    );
    assert!(
        model_registry.lines().count() < 350,
        "model registry module regrew beyond its ownership boundary"
    );
    assert!(
        model_registry_vision.lines().count() < 125,
        "model registry vision module regrew beyond its ownership boundary"
    );
    assert!(
        model_registry_vision_preparation.lines().count() < 125,
        "model registry vision preparation module regrew beyond its ownership boundary"
    );
    assert!(
        model_registry_vision_preparation_tests.lines().count() < 225,
        "model registry vision preparation tests regrew beyond their ownership boundary"
    );
    assert!(model_default_selection.lines().count() < 75);
    assert!(model_setup.lines().count() < 100);
    assert!(model_runtime_spec.lines().count() < 125);
    assert!(
        model_tests.lines().count() < 550,
        "model regression module regrew beyond its ownership boundary"
    );
}
