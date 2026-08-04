fn assert_runtime_inference_owners() {
    for target in [
        "src/runtime_core/inference/backend.rs",
        "src/runtime_core/inference/backend/admission.rs",
        "src/runtime_core/inference/backend/lifecycle.rs",
        "src/runtime_core/inference/benchmark.rs",
        "src/runtime_core/inference/benchmark/fixture.rs",
        "src/runtime_core/inference/benchmark/fixture/json.rs",
        "src/runtime_core/inference/benchmark/fixture/schema.rs",
        "src/runtime_core/inference/benchmark/fixture/types.rs",
        "src/runtime_core/inference/benchmark/report.rs",
        "src/runtime_core/inference/model.rs",
        "src/runtime_core/inference/model/codec.rs",
        "src/runtime_core/inference/model/manifest.rs",
        "src/runtime_core/inference/model/manifest/tests.rs",
        "src/runtime_core/inference/model/manifest/types.rs",
        "src/runtime_core/inference/model/manifest/validation.rs",
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
        "src/adapters/llama_cpp/backend/tests/parser_contract.rs",
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

    let fixture_facade =
        fs::read_to_string("src/runtime_core/inference/benchmark/fixture.rs").unwrap();
    assert!(fixture_facade.lines().count() < 100);
    for (owner, limit, responsibility) in [
        ("json", 240, "fn parse_fixture_json_object("),
        ("schema", 260, "fn validate_fixture_schema("),
        ("types", 100, "struct BenchmarkFixture"),
    ] {
        assert!(fixture_facade
            .lines()
            .any(|line| line == format!("mod {owner};")));
        assert!(!fixture_facade.contains(responsibility));
        let source = fs::read_to_string(format!(
            "src/runtime_core/inference/benchmark/fixture/{owner}.rs"
        ))
        .unwrap();
        assert!(source.contains(responsibility));
        assert!(source.lines().count() < limit);
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
    let llama_parser_tests =
        fs::read_to_string("src/adapters/llama_cpp/backend/tests/parser_contract.rs").unwrap();
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
    assert!(llama_tests.lines().any(|line| line == "mod parser_contract;"));
    assert!(llama_parser_tests.contains("fn managed_llama_parser_accepts_local_turn_schema("));
    assert!(llama_backend.lines().count() < 100);
    assert!(llama_discovery.lines().count() < 125);
    assert!(llama_health.lines().count() < 125);
    assert!(llama_request.lines().count() < 225);
    assert!(llama_sidecar.lines().count() < 50);
    assert!(llama_tests.lines().count() < 325);
    assert!(llama_parser_tests.lines().count() < 75);
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
    for (owner, line_budget, responsibilities) in [
        (
            "types",
            275,
            &[
                "pub enum ResourcePressure",
                "pub struct ResourceLaneDecision",
            ][..],
        ),
        ("pressure", 75, &["pub fn classify_pressure("][..]),
        ("chat", 100, &["pub fn chat_governor_decision("][..]),
        ("lanes", 100, &["pub fn team_lane_decision("][..]),
        (
            "context_model",
            150,
            &["pub fn context_model_governor_decision("][..],
        ),
        (
            "optimization",
            200,
            &["pub fn optimization_policy_decision("][..],
        ),
    ] {
        let relative = format!("resource/{owner}.rs");
        assert!(
            resource_policy.contains(&relative),
            "resource policy facade does not register {owner}"
        );
        let source = fs::read_to_string(format!("src/runtime_core/inference/{relative}")).unwrap();
        assert!(
            source.lines().count() < line_budget,
            "resource policy owner {owner} exceeded its {line_budget}-line budget"
        );
        for responsibility in responsibilities {
            assert!(
                source.contains(responsibility),
                "resource policy owner {owner} is missing: {responsibility}"
            );
        }
    }
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
        resource_policy.lines().count() < 75,
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
}
