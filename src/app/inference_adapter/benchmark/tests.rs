use super::*;
use std::fs;
use std::path::{Path, PathBuf};

use crate::adapters::filesystem::layout as paths;
use crate::runtime_core::inference::backend::BackendChatSampling;
use crate::runtime_core::inference::benchmark::ADOPTION_EXACT_RESPONSE;

#[test]
fn validates_fixture_metadata() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-benchmark-validate-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &root);
    let fixture_path = write_fixture(&root);

    let report = validate_report(fixture_path.to_str().unwrap()).unwrap();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);

    assert!(report.contains("status: valid"));
    assert!(report.contains("fixture id: sample-fixture"));
    assert!(report.contains("raw prompt/source 저장: 없음"));
}

#[test]
fn records_fixture_without_score_claim() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-benchmark-record-test-{}",
        std::process::id()
    ));
    let data_root = root.join("data");
    let project_root = root.join("project");
    fs::create_dir_all(&project_root).unwrap();
    std::env::set_var("RPOTATO_DATA_HOME", &data_root);
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
    let fixture_path = write_fixture(&project_root);

    let report = record_report(fixture_path.to_str().unwrap()).unwrap();
    let export = report_export(BenchmarkReportFormat::Jsonl).unwrap();

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);

    assert!(report.contains("claim state: not-comparable"));
    assert!(report.contains("score: 없음"));
    assert!(export.contains("\"fixture_id\":\"sample-fixture\""));
    assert!(export.contains("\"claim_state\":\"not-comparable\""));
    assert!(export.contains("\"score\":null"));
}

#[test]
fn executable_run_records_local_score_without_prompt_text() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-benchmark-executable-test-{}",
        std::process::id()
    ));
    let data_root = root.join("data");
    let project_root = root.join("project");
    fs::create_dir_all(&project_root).unwrap();
    std::env::set_var("RPOTATO_DATA_HOME", &data_root);
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
    let fixture_path = write_fixture(&project_root);
    mutate_fixture(&fixture_path, |text| {
        text.replace(
            "\"raw_artifact_retention_policy\": \"redacted-only\"",
            "\"raw_artifact_retention_policy\": \"redacted-only\",\n  \"expected_response_contains\": [\"RPOTATO_BENCHMARK_OK\"],\n  \"forbidden_response_contains\": [\"SECRET_PROMPT\"],\n  \"minimum_score\": 3",
        )
    });
    let prompt_path = project_root.join("prompt.txt");
    fs::write(
        &prompt_path,
        "SECRET_PROMPT: reply with RPOTATO_BENCHMARK_OK only.",
    )
    .unwrap();

    let report = run_report_with_chat(
        fixture_path.to_str().unwrap(),
        prompt_path.to_str().unwrap(),
        Some(16),
        |_prompt, _max_tokens| Ok(fake_chat_run("RPOTATO_BENCHMARK_OK")),
    )
    .unwrap();
    let export = report_export(BenchmarkReportFormat::Jsonl).unwrap();

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);

    assert!(report.contains("claim state: measured-locally"));
    assert!(report.contains("score: 3/3"));
    assert!(export.contains("\"claim_state\":\"measured-locally\""));
    assert!(export.contains("\"score\":3.000000"));
    assert!(export.contains("\"model_run_id\":\"model-run-backend-chat-event\""));
    assert!(export.contains("\"expected_matches\":1"));
    assert!(export.contains("\"forbidden_matches\":0"));
    assert!(export.contains("\"prompt_artifact_sha256\""));
    assert!(!export.contains("SECRET_PROMPT"));
}

#[test]
fn rejects_array_trailing_comma() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-benchmark-invalid-array-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &root);
    let fixture_path = write_fixture(&root);
    let text = fs::read_to_string(&fixture_path)
        .unwrap()
        .replace("\"required_tools\": [],", "\"required_tools\": [\"rg\",],");
    fs::write(&fixture_path, text).unwrap();

    let err = validate_report(fixture_path.to_str().unwrap()).unwrap_err();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);

    assert_eq!(err.code, 2);
    assert!(err.message.contains("schema parser"));
}

#[test]
fn rejects_malformed_context_budget_suffix() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-benchmark-invalid-number-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &root);
    let fixture_path = write_fixture(&root);
    mutate_fixture(&fixture_path, |text| {
        text.replace("\"context_budget\": 2048,", "\"context_budget\": 2048.5,")
    });

    let err = validate_report(fixture_path.to_str().unwrap()).unwrap_err();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);

    assert_eq!(err.code, 2);
    assert!(err.message.contains("schema parser"));
}

#[test]
fn rejects_malformed_bool_suffix() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-benchmark-invalid-bool-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &root);
    let fixture_path = write_fixture(&root);
    mutate_fixture(&fixture_path, |text| {
        text.replace(
            "\"abstention_required\": false,",
            "\"abstention_required\": falsex,",
        )
    });

    let err = validate_report(fixture_path.to_str().unwrap()).unwrap_err();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);

    assert_eq!(err.code, 2);
    assert!(err.message.contains("schema parser"));
}

#[test]
fn rejects_raw_prompt_field() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-benchmark-raw-prompt-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &root);
    let fixture_path = write_fixture(&root);
    mutate_fixture(&fixture_path, |text| {
        text.replace(
            "\"fixture_id\": \"sample-fixture\",",
            "\"fixture_id\": \"sample-fixture\",\n  \"raw_prompt\": \"secret\",",
        )
    });

    let err = validate_report(fixture_path.to_str().unwrap()).unwrap_err();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);

    assert_eq!(err.code, 2);
    assert!(err.message.contains("raw prompt/source field"));
}

#[test]
fn rejects_duplicate_field() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-benchmark-duplicate-field-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &root);
    let fixture_path = write_fixture(&root);
    mutate_fixture(&fixture_path, |text| {
        text.replace(
            "\"fixture_id\": \"sample-fixture\",",
            "\"fixture_id\": \"sample-fixture\",\n  \"fixture_id\": \"duplicate\",",
        )
    });

    let err = validate_report(fixture_path.to_str().unwrap()).unwrap_err();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);

    assert_eq!(err.code, 2);
    assert!(err.message.contains("중복"));
}

fn write_fixture(root: &Path) -> PathBuf {
    let fixture = root.join("sample-fixture.json");
    fs::write(
        &fixture,
        r#"{
  "fixture_id": "sample-fixture",
  "benchmark_name": "foundation-smoke",
  "runtime_capability_under_test": "fixture-validation",
  "model_vs_runtime_responsibility": "runtime validates metadata; model is not executed",
  "expected_route": "benchmark.record",
  "expected_policy_decision": "allow",
  "expected_escalation_target": "none",
  "required_tools": [],
  "required_source_reads": [],
  "required_evidence_records": ["benchmark_run"],
  "abstention_required": false,
  "expected_failure_category": "none",
  "ontology_view": "static-summary",
  "context_budget": 2048,
  "model_id": "not-applicable",
  "model_artifact_hash": "not-applicable",
  "quantization": "not-applicable",
  "backend_id": "not-applicable",
  "backend_version": "not-applicable",
  "dataset_ref": "local-fixture",
  "prompt_runtime_version": "rpotato-test",
  "tool_policy_version": "rpotato-test",
  "seed_policy": "fixed-0",
  "sampling_options": "not-applicable",
  "raw_artifact_retention_policy": "redacted-only"
}"#,
    )
    .unwrap();
    fixture
}

fn mutate_fixture(path: &Path, mutate: impl FnOnce(String) -> String) {
    let text = fs::read_to_string(path).unwrap();
    fs::write(path, mutate(text)).unwrap();
}

fn fake_chat_run(response: &str) -> BackendChatRun {
    BackendChatRun {
        backend_id: "llama.cpp".to_string(),
        backend_version: "b9878".to_string(),
        pid: 1234,
        model_id: "qwen-test".to_string(),
        model_path: PathBuf::from("/tmp/model.gguf"),
        model_artifact_hash: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .to_string(),
        ctx_size: Some(2048),
        prompt_chars: 52,
        response_chars: response.chars().count(),
        requested_max_tokens: 16,
        effective_max_tokens: 16,
        sampling: BackendChatSampling {
            temperature: 0.1,
            top_p: 0.8,
        },
        finish_reason: "stop".to_string(),
        guard_status: "pass",
        prompt_tokens: Some(8),
        completion_tokens: Some(4),
        total_tokens: Some(12),
        elapsed_ms: 200,
        first_token_latency_ms: Some(50),
        streaming_display: false,
        ledger_event: "backend-chat-event".to_string(),
        resource_governor_admission: "allow".to_string(),
        resource_governor_token_action: "none".to_string(),
        resource_governor_reason: "normal",
        resource_governor_hint: "none",
        resource_governor_sample_event: "resource-governor-event".to_string(),
        resource_pressure: "normal".to_string(),
        resource_cpu_percent: Some(12.0),
        resource_average_rss_bytes: Some(1024),
        resource_peak_rss_bytes: Some(2048),
        resource_disk_bytes: Some(4096),
        resource_sample_event: "resource-sample-event".to_string(),
        response: response.to_string(),
    }
}

#[test]
fn canonical_model_adoption_fixture_is_valid() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let project_root = paths::project_root();
    let fixture_dir = project_root.join("benchmarks/fixtures");
    fs::create_dir_all(&fixture_dir).unwrap();
    let fixture_path = fixture_dir.join("model-adoption-smoke-v1.json");
    fs::write(
        &fixture_path,
        include_str!("../../../../benchmarks/fixtures/model-adoption-smoke-v1.json"),
    )
    .unwrap();
    let prompt_dir = project_root.join("benchmarks/prompts");
    fs::create_dir_all(&prompt_dir).unwrap();
    let prompt_path = prompt_dir.join("model-adoption-smoke-v1.txt");
    fs::write(
        &prompt_path,
        include_str!("../../../../benchmarks/prompts/model-adoption-smoke-v1.txt"),
    )
    .unwrap();

    let fixture = benchmark_artifact::read_fixture(fixture_path.to_str().unwrap()).unwrap();
    let prompt = benchmark_artifact::read_prompt_artifact(prompt_path.to_str().unwrap()).unwrap();

    assert_eq!(fixture.fixture_id, benchmark_policy::ADOPTION_FIXTURE_ID);
    assert_eq!(fixture.sha256, benchmark_policy::ADOPTION_FIXTURE_SHA256);
    assert_eq!(fixture.dataset_ref, benchmark_policy::ADOPTION_DATASET_REF);
    assert_eq!(prompt.sha256, benchmark_policy::ADOPTION_PROMPT_SHA256);
    assert_eq!(fixture.minimum_score, Some(3));
    benchmark_policy::validate_canonical_adoption_artifacts(&fixture, &prompt).unwrap();

    let exact = score_response(&fixture, ADOPTION_EXACT_RESPONSE);
    assert_eq!(exact.score, 3);
    assert!(exact.local_pass);
    for invalid in [
        format!("extra\n{ADOPTION_EXACT_RESPONSE}"),
        ADOPTION_EXACT_RESPONSE
            .lines()
            .rev()
            .collect::<Vec<_>>()
            .join("\n"),
        format!("{ADOPTION_EXACT_RESPONSE}\nrm -rf /"),
    ] {
        let score = score_response(&fixture, &invalid);
        assert_eq!(score.score, 2);
        assert!(!score.local_pass);
    }

    let mut run = fake_chat_run(ADOPTION_EXACT_RESPONSE);
    run.model_artifact_hash =
        "00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4".to_string();
    run.requested_max_tokens = benchmark_policy::ADOPTION_MAX_TOKENS;
    run.effective_max_tokens = benchmark_policy::ADOPTION_MAX_TOKENS;
    benchmark_policy::validate_canonical_adoption_run(&fixture, &run).unwrap();
    let manifest = executable_reproducibility_manifest_json(
        &fixture,
        &prompt,
        &run,
        &exact,
        "benchmark-test",
        "model-run-test",
        1,
    );
    assert!(manifest.contains("requested_max_tokens=192,effective_max_tokens=192"));
    assert!(manifest.contains("\"quantization\":\"Q4_K_M\""));

    run.effective_max_tokens = benchmark_policy::ADOPTION_MAX_TOKENS - 1;
    assert!(benchmark_policy::validate_canonical_adoption_run(&fixture, &run).is_err());
}
