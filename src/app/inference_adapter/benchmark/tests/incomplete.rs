use super::*;

#[test]
fn token_limit_run_records_no_benchmark_evidence() {
    assert_incomplete_run_records_no_benchmark_evidence(
        BackendGenerationIncompleteReason::TokenLimit,
        "token limit",
    );
}

#[test]
fn unknown_finish_run_records_no_benchmark_evidence() {
    assert_incomplete_run_records_no_benchmark_evidence(
        BackendGenerationIncompleteReason::UnknownFinish,
        "종료 상태",
    );
}

fn assert_incomplete_run_records_no_benchmark_evidence(
    reason: BackendGenerationIncompleteReason,
    expected_error: &str,
) {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-benchmark-incomplete-{}-{}",
        expected_error.replace(' ', "-"),
        std::process::id()
    ));
    let data_root = root.join("data");
    let project_root = root.join("project");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&project_root).unwrap();
    std::env::set_var("RPOTATO_DATA_HOME", &data_root);
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
    let fixture_path = write_fixture(&project_root);
    mutate_fixture(&fixture_path, |text| {
        text.replace(
            "\"raw_artifact_retention_policy\": \"redacted-only\"",
            "\"raw_artifact_retention_policy\": \"redacted-only\",\n  \"expected_response_contains\": [\"RPOTATO_BENCHMARK_OK\"],\n  \"minimum_score\": 3",
        )
    });
    let prompt_path = project_root.join("prompt.txt");
    fs::write(&prompt_path, "reply with RPOTATO_BENCHMARK_OK only.").unwrap();
    let events_before = ledger::read_runtime_events().unwrap();

    let error = run_report_with_chat(
        fixture_path.to_str().unwrap(),
        prompt_path.to_str().unwrap(),
        Some(16),
        |_prompt, _max_tokens| {
            let mut run = fake_chat_run("RPOTATO_BENCHMARK_OK");
            run.generation_status = BackendGenerationStatus::Incomplete(reason);
            Ok(run)
        },
    )
    .unwrap_err();

    assert_eq!(error.code, 3);
    assert!(error.message.contains(expected_error));
    assert_eq!(ledger::read_runtime_events().unwrap(), events_before);
    assert!(report_export(BenchmarkReportFormat::Jsonl)
        .unwrap()
        .is_empty());

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);
}
