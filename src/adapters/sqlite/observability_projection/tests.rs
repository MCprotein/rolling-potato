use std::cell::Cell;
use std::time::{Duration, Instant};

use super::*;

type LedgerProjectionRow = (i64, String, i64, String, String, String, String);
const TEST_LEDGER: TestCanonicalLedgerReader = TestCanonicalLedgerReader;

struct TestCanonicalLedgerReader;

impl CanonicalLedgerReadPort for TestCanonicalLedgerReader {
    fn read_events(&self) -> Result<Vec<ParsedLedgerEvent>, AppError> {
        crate::app::workflow_adapter::ledger::read_runtime_events()
    }
}

impl CanonicalTranscriptReadPort for TestCanonicalLedgerReader {}

fn replay_test_event(index: u64) -> ParsedLedgerEvent {
    ParsedLedgerEvent {
        event_id: format!("event-replay-{index}"),
        ts_ms: u128::from(index),
        event_type: "test.replay".to_string(),
        project_id: "project-replay".to_string(),
        session_id: "session-replay".to_string(),
        summary: format!("summary-{index}"),
        details: format!("detail={index}"),
        previous_event_hash: None,
        event_hash: None,
    }
}

fn current_identity() -> RuntimeIdentity {
    crate::app::workflow_adapter::ledger::validated_current_identity().unwrap()
}

fn projected_status() -> StoreStatus {
    status(&TEST_LEDGER).unwrap()
}

fn record_test_model_run(metric: &ModelRunMetric) -> Result<(), AppError> {
    record_model_run(&current_identity(), &TEST_LEDGER, metric)
}

fn record_test_resource_sample(metric: &ResourceSampleMetric) -> Result<(), AppError> {
    record_resource_sample(&current_identity(), &TEST_LEDGER, metric)
}

fn record_test_benchmark_run(metric: &BenchmarkRunMetric) -> Result<(), AppError> {
    record_benchmark_run(&current_identity(), &TEST_LEDGER, metric)
}

struct FailingLedgerReader<'a> {
    database: &'a std::path::Path,
    called_after_recovery: Cell<bool>,
}

impl CanonicalLedgerReadPort for FailingLedgerReader<'_> {
    fn read_events(&self) -> Result<Vec<ParsedLedgerEvent>, AppError> {
        let file_name = self.database.file_name().unwrap().to_string_lossy();
        let recovered_prefix = format!("{file_name}.corrupt.");
        let recovered_exists = self
            .database
            .parent()
            .unwrap()
            .read_dir()
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(&recovered_prefix)
            });
        self.called_after_recovery.set(recovered_exists);
        Err(AppError::blocked("injected canonical ledger read failure"))
    }
}

impl CanonicalTranscriptReadPort for FailingLedgerReader<'_> {}

fn ledger_projection_rows(connection: &Connection) -> Vec<LedgerProjectionRow> {
    let mut statement = connection
        .prepare(
            "SELECT rowid, event_id, ts_ms, event_type, project_id, session_id, summary
               FROM ledger_events
           ORDER BY rowid",
        )
        .unwrap();
    statement
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

#[test]
fn corrupt_sqlite_is_preserved_before_canonical_ledger_failure() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-sqlite-ledger-recovery-order-{}-{}",
        std::process::id(),
        now_ms()
    ));
    let project_root = root.join("project");
    fs::create_dir_all(&project_root).unwrap();
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
    let database = paths::observability_db_file();
    fs::create_dir_all(database.parent().unwrap()).unwrap();
    fs::write(&database, b"corrupt sqlite bytes").unwrap();
    let ledger = FailingLedgerReader {
        database: &database,
        called_after_recovery: Cell::new(false),
    };

    let error = status(&ledger).unwrap_err();

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);
    assert_eq!(error.message, "injected canonical ledger read failure");
    assert!(
        ledger.called_after_recovery.get(),
        "canonical ledger was read before corrupt SQLite preservation"
    );
}

#[test]
fn sqlite_replay_faults_are_atomic_and_concurrent_readers_see_complete_rows() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-sqlite-atomic-replay-{}-{}",
        std::process::id(),
        now_ms()
    ));
    fs::create_dir_all(&root).unwrap();
    let database = root.join("observability.sqlite");
    let connection = Connection::open(&database).unwrap();
    migrate(&connection).unwrap();
    let original = vec![replay_test_event(1), replay_test_event(2)];
    replay_ledger_events(&connection, &original, &TEST_LEDGER).unwrap();
    let original_rows = ledger_projection_rows(&connection);
    drop(connection);

    let replacement = vec![
        replay_test_event(10),
        replay_test_event(11),
        replay_test_event(12),
    ];
    let pause_dir = root.join("pause");
    std::env::set_var("RPOTATO_TEST_SQLITE_REPLAY_PAUSE_DIR", &pause_dir);
    let replay_database = database.clone();
    let replay_events = replacement.clone();
    let replay = std::thread::spawn(move || {
        let connection = Connection::open(replay_database).unwrap();
        replay_ledger_events(&connection, &replay_events, &TEST_LEDGER)
    });
    let entered = pause_dir.join("after-clear.entered");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !entered.exists() {
        assert!(
            Instant::now() < deadline,
            "sqlite replay pause 진입 timeout"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let reader = open_read_only_path(&database).unwrap();
    assert_eq!(ledger_projection_rows(&reader), original_rows);
    fs::write(pause_dir.join("after-clear.release"), b"release").unwrap();
    replay.join().unwrap().unwrap();
    std::env::remove_var("RPOTATO_TEST_SQLITE_REPLAY_PAUSE_DIR");
    assert_eq!(ledger_projection_rows(&reader), original_rows);
    drop(reader);
    let reader = open_read_only_path(&database).unwrap();
    let replacement_rows = ledger_projection_rows(&reader);
    assert_eq!(replacement_rows.len(), replacement.len());
    assert_ne!(replacement_rows, original_rows);
    drop(reader);

    let connection = Connection::open(&database).unwrap();
    for point in ["after-clear", "after-first-event"] {
        std::env::set_var("RPOTATO_TEST_SQLITE_REPLAY_FAULT", point);
        let error = replay_ledger_events(&connection, &original, &TEST_LEDGER).unwrap_err();
        std::env::remove_var("RPOTATO_TEST_SQLITE_REPLAY_FAULT");
        assert!(error.message.contains(point));
        assert_eq!(
            ledger_projection_rows(&connection),
            replacement_rows,
            "fault point: {point}"
        );
    }

    drop(connection);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn csv_cell_quotes_only_when_needed() {
    assert_eq!(csv_cell("plain"), "plain");
    assert_eq!(csv_cell("a,b"), "\"a,b\"");
    assert_eq!(csv_cell("a\"b"), "\"a\"\"b\"");
}

#[test]
fn workflow_projection_uses_checkpoint_active_skill_id() {
    let connection = Connection::open_in_memory().unwrap();
    migrate(&connection).unwrap();

    project_workflow_checkpoint(
        &connection,
        "workflow.checkpoint",
        "workflow_id=workflow-skill phase=running active_skill_id=ralph skill_state=active",
        "session-test",
        42,
    )
    .unwrap();
    project_workflow_checkpoint(
        &connection,
        "workflow.checkpoint",
        "workflow_id=workflow-legacy phase=model-pending",
        "session-test",
        43,
    )
    .unwrap();

    let actual: Option<String> = connection
        .query_row(
            "SELECT active_skill_id FROM workflows WHERE workflow_id = 'workflow-skill'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let absent: Option<String> = connection
        .query_row(
            "SELECT active_skill_id FROM workflows WHERE workflow_id = 'workflow-legacy'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(actual.as_deref(), Some("ralph"));
    assert_eq!(absent, None);
}

#[test]
fn evidence_and_stop_gate_events_are_projected_as_rebuildable_truth_views() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-patch-projection-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let project_root = root.join("project");
    fs::create_dir_all(&project_root).unwrap();
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
    crate::test_support::initialize_runtime_state().unwrap();

    crate::test_support::record_runtime_event(
        "verification.evidence.recorded",
        "evidence",
        "workflow_id=workflow-test evidence_id=evidence-test artifact_hash=abc passed=true source_hash=def",
    )
    .unwrap();
    crate::test_support::record_runtime_event(
        "workflow.stop_gate.passed",
        "stop gate",
        "workflow_id=workflow-test proposal_id=proposal-test evidence_id=evidence-test applied_hash=def unresolved_approval=false",
    )
    .unwrap();
    let projected = projected_status();

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);
    assert_eq!(projected.evidence_records, 1);
    assert_eq!(projected.stop_gate_results, 1);
}

#[test]
fn record_model_run_updates_model_summary() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root =
        std::env::temp_dir().join(format!("rpotato-model-metric-test-{}", std::process::id()));
    let project_root = root.join("project");
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);

    record_test_model_run(&ModelRunMetric {
        model_run_id: "model-run-test".to_string(),
        session_id: "session-test".to_string(),
        workflow_id: None,
        model_id: "qwen-test".to_string(),
        model_artifact_hash: None,
        backend_id: Some("llama.cpp".to_string()),
        backend_version: None,
        quantization: None,
        context_limit_tokens: Some(4096),
        started_at_ms: 1,
        first_token_latency_ms: None,
        total_latency_ms: Some(100.0),
        prompt_eval_ms: None,
        generation_eval_ms: None,
        tokens_per_second: Some(120.0),
        cancelled: false,
        token_usage_complete: true,
        prompt_tokens: 10,
        completion_tokens: 12,
        total_tokens: 22,
        context_tokens_used: 10,
        context_tokens_dropped: 0,
        ontology_tokens: 0,
        tool_summary_tokens: 0,
        max_output_tokens: Some(64),
    })
    .unwrap();

    let summaries = model_summaries().unwrap();

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");

    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].model_id, "qwen-test");
    assert_eq!(summaries[0].runs, 1);
    assert_eq!(summaries[0].prompt_tokens, 10);
    assert_eq!(summaries[0].completion_tokens, 12);
    assert_eq!(summaries[0].total_tokens, 22);
    assert_eq!(summaries[0].avg_latency_ms, Some(100.0));
    assert_eq!(summaries[0].avg_tokens_per_second, Some(120.0));
}

#[test]
fn incomplete_stream_usage_keeps_model_run_without_zero_token_record() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-incomplete-stream-metric-test-{}",
        std::process::id()
    ));
    let project_root = root.join("project");
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);

    record_test_model_run(&ModelRunMetric {
        model_run_id: "model-run-incomplete-stream".to_string(),
        session_id: "session-test".to_string(),
        workflow_id: None,
        model_id: "qwen-test".to_string(),
        model_artifact_hash: None,
        backend_id: Some("llama.cpp".to_string()),
        backend_version: None,
        quantization: None,
        context_limit_tokens: Some(4096),
        started_at_ms: 1,
        first_token_latency_ms: Some(25.0),
        total_latency_ms: Some(100.0),
        prompt_eval_ms: None,
        generation_eval_ms: None,
        tokens_per_second: None,
        cancelled: true,
        token_usage_complete: false,
        prompt_tokens: 0,
        completion_tokens: 0,
        total_tokens: 0,
        context_tokens_used: 0,
        context_tokens_dropped: 0,
        ontology_tokens: 0,
        tool_summary_tokens: 0,
        max_output_tokens: Some(64),
    })
    .unwrap();

    let store = projected_status();
    let summaries = model_summaries().unwrap();

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);

    assert_eq!(store.model_runs, 1);
    assert_eq!(store.token_records, 0);
    assert!(summaries.is_empty());
}

#[test]
fn record_resource_sample_updates_resource_status() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-resource-metric-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let project_root = root.join("project");
    fs::create_dir_all(&project_root).unwrap();
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
    let identity = crate::app::workflow_adapter::ledger::validated_current_identity().unwrap();

    record_test_resource_sample(&ResourceSampleMetric {
        resource_sample_id: "resource-sample-test".to_string(),
        session_id: identity.session_id,
        backend_id: "llama.cpp".to_string(),
        pid: 123,
        process_cpu_percent: Some(42.5),
        average_rss_bytes: Some(256 * 1024 * 1024),
        peak_rss_bytes: Some(512 * 1024 * 1024),
        disk_bytes: Some(1024),
        sample_count: 1,
        pressure_status: "normal".to_string(),
        recorded_at_ms: 1000,
    })
    .unwrap();

    let status = projected_status();
    let latest = latest_resource_sample().unwrap().unwrap();

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);

    assert_eq!(status.resource_samples, 1);
    assert_eq!(latest.backend_id, "llama.cpp");
    assert_eq!(latest.process_cpu_percent, Some(42.5));
    assert_eq!(latest.average_rss_bytes, Some(256 * 1024 * 1024));
    assert_eq!(latest.peak_rss_bytes, Some(512 * 1024 * 1024));
    assert_eq!(latest.disk_bytes, Some(1024));
    assert_eq!(latest.pressure_status, "normal");
}

#[test]
fn record_benchmark_run_projects_report_rows() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-benchmark-run-metric-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let project_root = root.join("project");
    fs::create_dir_all(&project_root).unwrap();
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);

    let metric = BenchmarkRunMetric {
        benchmark_run_id: "benchmark-run-test".to_string(),
        session_id: "session-test".to_string(),
        model_run_id: Some("model-run-test".to_string()),
        model_id: "qwen-test".to_string(),
        benchmark_name: "foundation-smoke".to_string(),
        fixture_id: "fixture-test".to_string(),
        fixture_sha256: "sha256-test".to_string(),
        prompt_artifact_sha256: Some("prompt-sha256-test".to_string()),
        prompt_chars: Some(42),
        claim_state: "measured-locally".to_string(),
        score: Some(3.0),
        score_unit: Some("0-3-local-product-score".to_string()),
        local_pass: Some(true),
        expected_matches: Some(1),
        expected_total: Some(1),
        forbidden_matches: Some(0),
        harness_ref: "rpotato-benchmark-harness@test".to_string(),
        dataset_ref: Some("local-fixture".to_string()),
        backend_id: Some("llama.cpp".to_string()),
        latency_ms: Some(123.0),
        tokens_per_second: Some(4.5),
        prompt_tokens: Some(10),
        completion_tokens: Some(5),
        total_tokens: Some(15),
        resource_pressure: Some("normal".to_string()),
        peak_rss_bytes: Some(2048),
        reproducibility_manifest: "{\"fixture_id\":\"fixture-test\"}".to_string(),
        redacted_report: "{\"raw_prompt_source_stored\":false}".to_string(),
        recorded_at_ms: 1000,
    };
    record_test_benchmark_run(&metric).unwrap();
    let duplicate_err = record_test_benchmark_run(&metric).unwrap_err();

    let status = projected_status();
    let reports = benchmark_run_reports(&TEST_LEDGER).unwrap();

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);

    assert_eq!(status.benchmark_runs, 1);
    assert_eq!(duplicate_err.code, 1);
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].benchmark_run_id, "benchmark-run-test");
    assert_eq!(reports[0].session_id, "session-test");
    assert_eq!(reports[0].model_run_id.as_deref(), Some("model-run-test"));
    assert_eq!(reports[0].fixture_id, "fixture-test");
    assert_eq!(reports[0].claim_state, "measured-locally");
    assert_eq!(reports[0].score, Some(3.0));
    assert_eq!(reports[0].local_pass, Some(true));
    assert_eq!(reports[0].expected_matches, Some(1));
    assert_eq!(reports[0].expected_total, Some(1));
    assert_eq!(reports[0].forbidden_matches, Some(0));
    assert_eq!(reports[0].prompt_tokens, Some(10));
    assert_eq!(reports[0].completion_tokens, Some(5));
    assert_eq!(reports[0].total_tokens, Some(15));
    assert_eq!(reports[0].resource_pressure.as_deref(), Some("normal"));
    assert_eq!(reports[0].peak_rss_bytes, Some(2048));
    assert!(reports[0]
        .reproducibility_manifest
        .contains("\"fixture_id\":\"fixture-test\""));
    assert!(reports[0]
        .redacted_report
        .contains("\"raw_prompt_source_stored\":false"));
}

#[test]
fn performance_baseline_aggregates_local_metrics() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-performance-baseline-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let project_root = root.join("project");
    fs::create_dir_all(&project_root).unwrap();
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);

    for (model_run_id, session_id, model_id, latency, tps, dropped, total_tokens) in [
        (
            "run-a",
            "session-a",
            "qwen-test",
            100.0,
            20.0,
            0_u32,
            30_u32,
        ),
        (
            "run-b",
            "session-a",
            "qwen-test",
            200.0,
            30.0,
            8_u32,
            40_u32,
        ),
        (
            "run-c",
            "session-b",
            "gemma-test",
            300.0,
            10.0,
            0_u32,
            50_u32,
        ),
    ] {
        record_test_model_run(&ModelRunMetric {
            model_run_id: model_run_id.to_string(),
            session_id: session_id.to_string(),
            workflow_id: None,
            model_id: model_id.to_string(),
            model_artifact_hash: Some(format!("hash-{model_run_id}")),
            backend_id: Some("llama.cpp".to_string()),
            backend_version: Some("test".to_string()),
            quantization: Some("q4".to_string()),
            context_limit_tokens: Some(4096),
            started_at_ms: 1,
            first_token_latency_ms: None,
            total_latency_ms: Some(latency),
            prompt_eval_ms: None,
            generation_eval_ms: None,
            tokens_per_second: Some(tps),
            cancelled: false,
            token_usage_complete: true,
            prompt_tokens: 10,
            completion_tokens: total_tokens - 10,
            total_tokens,
            context_tokens_used: 100,
            context_tokens_dropped: dropped,
            ontology_tokens: 0,
            tool_summary_tokens: 0,
            max_output_tokens: Some(64),
        })
        .unwrap();
    }

    for (sample_id, pressure, peak_rss) in [
        ("sample-a", "normal", 256 * 1024 * 1024),
        ("sample-b", "degraded", 512 * 1024 * 1024),
    ] {
        record_test_resource_sample(&ResourceSampleMetric {
            resource_sample_id: sample_id.to_string(),
            session_id: "session-a".to_string(),
            backend_id: "llama.cpp".to_string(),
            pid: 123,
            process_cpu_percent: Some(42.0),
            average_rss_bytes: Some(128 * 1024 * 1024),
            peak_rss_bytes: Some(peak_rss),
            disk_bytes: Some(2048),
            sample_count: 1,
            pressure_status: pressure.to_string(),
            recorded_at_ms: 1000,
        })
        .unwrap();
    }

    let baseline = performance_baseline(&TEST_LEDGER).unwrap();

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);

    assert_eq!(baseline.model_runs, 3);
    assert_eq!(baseline.token_records, 3);
    assert_eq!(baseline.resource_samples, 2);
    assert_eq!(baseline.total_tokens, 120);
    assert_eq!(baseline.context_clamp_count, 1);
    assert_eq!(baseline.context_tokens_dropped, 8);
    assert_eq!(baseline.p50_latency_ms, Some(200.0));
    assert_eq!(baseline.p95_latency_ms, Some(290.0));
    assert_eq!(baseline.avg_tokens_per_second, Some(20.0));
    assert_eq!(baseline.peak_rss_bytes, Some(512 * 1024 * 1024));
    assert_eq!(
        baseline.pressure_states,
        vec![
            PressureStateSummary {
                pressure_status: "degraded".to_string(),
                samples: 1
            },
            PressureStateSummary {
                pressure_status: "normal".to_string(),
                samples: 1
            }
        ]
    );

    let qwen_group = baseline
        .groups
        .iter()
        .find(|group| group.model_id == "qwen-test" && group.session_id == "session-a")
        .unwrap();
    assert_eq!(qwen_group.runs, 2);
    assert_eq!(qwen_group.total_tokens, 70);
    assert_eq!(qwen_group.context_clamp_count, 1);
    assert_eq!(qwen_group.context_tokens_dropped, 8);
}

#[test]
fn optimization_policy_reads_metrics_and_measured_benchmark_evidence() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-optimization-policy-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let project_root = root.join("project");
    fs::create_dir_all(&project_root).unwrap();
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);

    record_test_model_run(&ModelRunMetric {
        model_run_id: "model-run-optimization-test".to_string(),
        session_id: "session-test".to_string(),
        workflow_id: None,
        model_id: "qwen-test".to_string(),
        model_artifact_hash: Some("hash-test".to_string()),
        backend_id: Some("llama.cpp".to_string()),
        backend_version: Some("test".to_string()),
        quantization: Some("q4".to_string()),
        context_limit_tokens: Some(4096),
        started_at_ms: 1,
        first_token_latency_ms: None,
        total_latency_ms: Some(250.0),
        prompt_eval_ms: None,
        generation_eval_ms: None,
        tokens_per_second: Some(32.0),
        cancelled: false,
        token_usage_complete: true,
        prompt_tokens: 10,
        completion_tokens: 10,
        total_tokens: 20,
        context_tokens_used: 100,
        context_tokens_dropped: 0,
        ontology_tokens: 0,
        tool_summary_tokens: 0,
        max_output_tokens: Some(64),
    })
    .unwrap();
    record_test_resource_sample(&ResourceSampleMetric {
        resource_sample_id: "resource-sample-optimization-test".to_string(),
        session_id: "session-test".to_string(),
        backend_id: "llama.cpp".to_string(),
        pid: 123,
        process_cpu_percent: Some(22.0),
        average_rss_bytes: Some(256 * 1024 * 1024),
        peak_rss_bytes: Some(512 * 1024 * 1024),
        disk_bytes: Some(1024),
        sample_count: 1,
        pressure_status: "normal".to_string(),
        recorded_at_ms: 1000,
    })
    .unwrap();
    record_test_benchmark_run(&BenchmarkRunMetric {
        benchmark_run_id: "benchmark-run-optimization-test".to_string(),
        session_id: "session-test".to_string(),
        model_run_id: Some("model-run-optimization-test".to_string()),
        model_id: "qwen-test".to_string(),
        benchmark_name: "optimization-smoke".to_string(),
        fixture_id: "fixture-optimization".to_string(),
        fixture_sha256: "sha256-test".to_string(),
        prompt_artifact_sha256: Some("prompt-sha256-test".to_string()),
        prompt_chars: Some(42),
        claim_state: "measured-locally".to_string(),
        score: Some(3.0),
        score_unit: Some("0-3-local-product-score".to_string()),
        local_pass: Some(true),
        expected_matches: Some(1),
        expected_total: Some(1),
        forbidden_matches: Some(0),
        harness_ref: "rpotato-benchmark-harness@test".to_string(),
        dataset_ref: Some("local-fixture".to_string()),
        backend_id: Some("llama.cpp".to_string()),
        latency_ms: Some(250.0),
        tokens_per_second: Some(32.0),
        prompt_tokens: Some(10),
        completion_tokens: Some(10),
        total_tokens: Some(20),
        resource_pressure: Some("normal".to_string()),
        peak_rss_bytes: Some(512 * 1024 * 1024),
        reproducibility_manifest: "{\"fixture_id\":\"fixture-optimization\"}".to_string(),
        redacted_report: "{\"raw_prompt_source_stored\":false}".to_string(),
        recorded_at_ms: 1100,
    })
    .unwrap();

    let policy = optimization_policy(&TEST_LEDGER).unwrap();

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);

    assert_eq!(policy.model_runs, 1);
    assert_eq!(policy.resource_samples, 1);
    assert_eq!(policy.latest_resource_pressure, "normal");
    assert_eq!(policy.benchmark_evidence.measured_runs, 1);
    assert_eq!(policy.benchmark_evidence.passed_runs, 1);
    assert_eq!(policy.benchmark_evidence.failed_runs, 0);
    assert_eq!(policy.benchmark_evidence.avg_score, Some(3.0));
    assert_eq!(
        policy.benchmark_evidence.latest_benchmark_run_id.as_deref(),
        Some("benchmark-run-optimization-test")
    );
    assert_eq!(
        policy.decision.status,
        resource::OptimizationPolicyStatus::Recommend
    );
    assert_eq!(
        policy.decision.recommended_context_tokens,
        Some(resource::DEFAULT_CONTEXT_LIMIT_TOKENS)
    );
    assert_eq!(
        policy.decision.recommended_lanes,
        resource::DEFAULT_TEAM_REQUESTED_LANES
    );
    assert_eq!(policy.decision.model_hint, resource::ModelRouteHint::Keep);
}
