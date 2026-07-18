use crate::adapters::filesystem::benchmark_artifact;
use crate::app::observability_adapter as observability;
use crate::app::workflow_adapter::ledger;
use crate::foundation::error::AppError;
use crate::runtime_core::inference::backend::BackendChatRun;
use crate::runtime_core::inference::benchmark::fixture::BenchmarkFixture;
use crate::runtime_core::inference::benchmark::report::{
    display_optional_u32, display_optional_u64, executable_redacted_report_json,
    executable_reproducibility_manifest_json, harness_ref, json_option, json_option_bool,
    json_option_f64, json_option_u32, json_option_u64, json_raw_or_string, redacted_report_json,
    reproducibility_manifest_json, BenchmarkReportFormat,
};
use crate::runtime_core::inference::benchmark::{
    self as benchmark_policy, BenchmarkScore, BenchmarkScoringPolicy,
};

use super::backend;

pub fn validate_report(path: &str) -> Result<String, AppError> {
    let fixture = benchmark_artifact::read_fixture(path)?;
    Ok(format!(
        "benchmark fixture validation\n- status: valid\n- fixture id: {}\n- benchmark: {}\n- fixture path: {}\n- fixture sha256: {}\n- runtime capability: {}\n- responsibility split: {}\n- expected route: {}\n- expected policy decision: {}\n- expected escalation target: {}\n- required tools: {}\n- required source reads: {}\n- required evidence records: {}\n- abstention required: {}\n- expected failure category: {}\n- ontology view: {}\n- context budget tokens: {}\n- model id: {}\n- backend: {} {}\n- dataset: {}\n- reproducibility metadata: ready\n- raw prompt/source 저장: 없음\n- boundary: fixture metadata validation only; model 실행, scoring, public benchmark parity claim을 수행하지 않음",
        fixture.fixture_id,
        fixture.benchmark_name,
        fixture.path.display(),
        fixture.sha256,
        fixture.runtime_capability_under_test,
        fixture.model_vs_runtime_responsibility,
        fixture.expected_route,
        fixture.expected_policy_decision,
        fixture.expected_escalation_target,
        fixture.required_tools.len(),
        fixture.required_source_reads.len(),
        fixture.required_evidence_records.len(),
        if fixture.abstention_required { "yes" } else { "no" },
        fixture.expected_failure_category,
        fixture.ontology_view,
        fixture.context_budget,
        fixture.model_id,
        fixture.backend_id,
        fixture.backend_version,
        fixture.dataset_ref
    ))
}

pub fn record_report(path: &str) -> Result<String, AppError> {
    let fixture = benchmark_artifact::read_fixture(path)?;
    let identity = ledger::validated_current_identity()?;
    let event = ledger::new_event_for(
        &identity,
        "benchmark.run.recorded",
        "benchmark fixture metadata recorded",
        &format!(
            "fixture_id={} benchmark={} fixture_sha256={} model_id={} backend_id={} harness_ref={} claim_state=not-comparable",
            fixture.fixture_id,
            fixture.benchmark_name,
            fixture.sha256,
            fixture.model_id,
            fixture.backend_id,
            harness_ref()
        ),
    );
    let benchmark_run_id = format!("benchmark-{}", event.event_id);
    let manifest = reproducibility_manifest_json(&fixture, &benchmark_run_id, event.ts_ms);
    let redacted_report = redacted_report_json(&fixture, &benchmark_run_id);
    let metric = observability::BenchmarkRunMetric {
        benchmark_run_id: benchmark_run_id.clone(),
        session_id: identity.session_id.clone(),
        model_run_id: None,
        model_id: fixture.model_id.clone(),
        benchmark_name: fixture.benchmark_name.clone(),
        fixture_id: fixture.fixture_id.clone(),
        fixture_sha256: fixture.sha256.clone(),
        prompt_artifact_sha256: None,
        prompt_chars: None,
        claim_state: "not-comparable".to_string(),
        score: None,
        score_unit: None,
        local_pass: None,
        expected_matches: None,
        expected_total: None,
        forbidden_matches: None,
        harness_ref: harness_ref(),
        dataset_ref: Some(fixture.dataset_ref.clone()),
        backend_id: Some(fixture.backend_id.clone()),
        latency_ms: None,
        tokens_per_second: None,
        prompt_tokens: None,
        completion_tokens: None,
        total_tokens: None,
        resource_pressure: None,
        peak_rss_bytes: None,
        reproducibility_manifest: manifest,
        redacted_report,
        recorded_at_ms: event.ts_ms,
    };

    let appended = ledger::append_event(&event)?;
    observability::project_event_with_ordinal(&event, appended.ordinal)?;
    observability::record_benchmark_run(&metric)?;

    Ok(format!(
        "benchmark run 기록\n- status: recorded\n- benchmark run id: {}\n- session id: {}\n- fixture id: {}\n- benchmark: {}\n- fixture sha256: {}\n- harness ref: {}\n- claim state: not-comparable\n- score: 없음\n- ledger event: {}\n- SQLite projection: benchmark_runs\n- raw prompt/source 저장: 없음\n- boundary: fixture metadata와 reproducibility manifest만 기록했습니다. model 실행, scoring, public benchmark parity claim은 수행하지 않았습니다.",
        benchmark_run_id,
        identity.session_id,
        fixture.fixture_id,
        fixture.benchmark_name,
        fixture.sha256,
        harness_ref(),
        event.event_id
    ))
}

pub fn run_report(
    fixture_path: &str,
    prompt_path: &str,
    max_tokens: Option<u32>,
) -> Result<String, AppError> {
    run_report_with_chat(fixture_path, prompt_path, max_tokens, backend::chat_once)
}

fn run_report_with_chat(
    fixture_path: &str,
    prompt_path: &str,
    max_tokens: Option<u32>,
    chat_once: impl FnOnce(&str, Option<u32>) -> Result<BackendChatRun, AppError>,
) -> Result<String, AppError> {
    let fixture = benchmark_artifact::read_fixture(fixture_path)?;
    if fixture.expected_response_contains.is_empty() {
        return Err(AppError::usage(
            "benchmark run에는 expected_response_contains fixture field가 필요합니다.",
        ));
    }
    let prompt = benchmark_artifact::read_prompt_artifact(prompt_path)?;
    benchmark_policy::validate_canonical_adoption_artifacts(&fixture, &prompt)?;
    let run = chat_once(&prompt.text, max_tokens)?;
    benchmark_policy::validate_canonical_adoption_run(&fixture, &run)?;
    let score = score_response(&fixture, &run.response);
    let identity = ledger::validated_current_identity()?;
    let event = ledger::new_event_for(
        &identity,
        "benchmark.run.executed",
        "benchmark executable run recorded",
        &format!(
            "fixture_id={} benchmark={} fixture_sha256={} prompt_sha256={} model_id={} backend_id={} model_run_id=model-run-{} score={} local_pass={} claim_state=measured-locally",
            fixture.fixture_id,
            fixture.benchmark_name,
            fixture.sha256,
            prompt.sha256,
            run.model_id,
            run.backend_id,
            run.ledger_event,
            score.score,
            score.local_pass
        ),
    );
    let benchmark_run_id = format!("benchmark-{}", event.event_id);
    let model_run_id = format!("model-run-{}", run.ledger_event);
    let recorded_at_ms = event.ts_ms;
    let manifest = executable_reproducibility_manifest_json(
        &fixture,
        &prompt,
        &run,
        &score,
        &benchmark_run_id,
        &model_run_id,
        recorded_at_ms,
    );
    let redacted_report = executable_redacted_report_json(
        &fixture,
        &prompt,
        &run,
        &score,
        &benchmark_run_id,
        &model_run_id,
    );
    let completion_tokens = run.completion_tokens.unwrap_or(0);
    let tokens_per_second = if completion_tokens > 0 && run.elapsed_ms > 0 {
        Some((completion_tokens as f64) / ((run.elapsed_ms as f64) / 1000.0))
    } else {
        None
    };

    let appended = ledger::append_event(&event)?;
    observability::project_event_with_ordinal(&event, appended.ordinal)?;
    observability::record_benchmark_run(&observability::BenchmarkRunMetric {
        benchmark_run_id: benchmark_run_id.clone(),
        session_id: identity.session_id.clone(),
        model_run_id: Some(model_run_id.clone()),
        model_id: run.model_id.clone(),
        benchmark_name: fixture.benchmark_name.clone(),
        fixture_id: fixture.fixture_id.clone(),
        fixture_sha256: fixture.sha256.clone(),
        prompt_artifact_sha256: Some(prompt.sha256.clone()),
        prompt_chars: Some(prompt.chars),
        claim_state: "measured-locally".to_string(),
        score: Some(score.score as f64),
        score_unit: Some("0-3-local-product-score".to_string()),
        local_pass: Some(score.local_pass),
        expected_matches: Some(score.expected_matches),
        expected_total: Some(score.expected_total),
        forbidden_matches: Some(score.forbidden_matches),
        harness_ref: harness_ref(),
        dataset_ref: Some(fixture.dataset_ref.clone()),
        backend_id: Some(run.backend_id.clone()),
        latency_ms: Some(run.elapsed_ms as f64),
        tokens_per_second,
        prompt_tokens: run.prompt_tokens,
        completion_tokens: run.completion_tokens,
        total_tokens: run.total_tokens,
        resource_pressure: Some(run.resource_pressure.clone()),
        peak_rss_bytes: run.resource_peak_rss_bytes,
        reproducibility_manifest: manifest,
        redacted_report,
        recorded_at_ms,
    })?;

    Ok(format!(
        "benchmark executable run\n- status: recorded\n- benchmark run id: {}\n- model run id: {}\n- session id: {}\n- fixture id: {}\n- benchmark: {}\n- ontology view: {}\n- fixture sha256: {}\n- prompt artifact: {}\n- prompt artifact sha256: {}\n- model id: {}\n- backend: {}\n- claim state: measured-locally\n- score: {}/3\n- minimum score: {}\n- local pass: {}\n- expected markers: {}/{}\n- forbidden marker matches: {}\n- latency ms: {}\n- prompt tokens: {}\n- completion tokens: {}\n- total tokens: {}\n- resource pressure: {}\n- peak rss bytes: {}\n- ledger event: {}\n- raw prompt/source 저장: 없음\n- boundary: local product benchmark score만 기록했습니다. public benchmark parity claim은 수행하지 않았습니다.",
        benchmark_run_id,
        model_run_id,
        identity.session_id,
        fixture.fixture_id,
        fixture.benchmark_name,
        fixture.ontology_view,
        fixture.sha256,
        prompt.path.display(),
        prompt.sha256,
        run.model_id,
        run.backend_id,
        score.score,
        fixture.minimum_score.unwrap_or(2),
        if score.local_pass { "yes" } else { "no" },
        score.expected_matches,
        score.expected_total,
        score.forbidden_matches,
        run.elapsed_ms,
        display_optional_u32(run.prompt_tokens),
        display_optional_u32(run.completion_tokens),
        display_optional_u32(run.total_tokens),
        run.resource_pressure,
        display_optional_u64(run.resource_peak_rss_bytes),
        event.event_id
    ))
}

pub fn report_export(format: BenchmarkReportFormat) -> Result<String, AppError> {
    match format {
        BenchmarkReportFormat::Jsonl => {
            let rows = observability::benchmark_run_reports()?;
            let mut output = String::new();
            for row in rows {
                output.push_str(&format!(
                    "{{\"benchmark_run_id\":\"{}\",\"session_id\":\"{}\",\"model_run_id\":{},\"model_id\":\"{}\",\"benchmark_name\":\"{}\",\"fixture_id\":\"{}\",\"fixture_sha256\":\"{}\",\"prompt_artifact_sha256\":{},\"prompt_chars\":{},\"claim_state\":\"{}\",\"score\":{},\"score_unit\":{},\"local_pass\":{},\"expected_matches\":{},\"expected_total\":{},\"forbidden_matches\":{},\"harness_ref\":\"{}\",\"dataset_ref\":{},\"backend_id\":{},\"latency_ms\":{},\"tokens_per_second\":{},\"prompt_tokens\":{},\"completion_tokens\":{},\"total_tokens\":{},\"resource_pressure\":{},\"peak_rss_bytes\":{},\"recorded_at_ms\":{},\"reproducibility_manifest\":{},\"redacted_report\":{}}}\n",
                    ledger::json_string(&row.benchmark_run_id),
                    ledger::json_string(&row.session_id),
                    json_option(&row.model_run_id),
                    ledger::json_string(&row.model_id),
                    ledger::json_string(&row.benchmark_name),
                    ledger::json_string(&row.fixture_id),
                    ledger::json_string(&row.fixture_sha256),
                    json_option(&row.prompt_artifact_sha256),
                    json_option_u32(row.prompt_chars),
                    ledger::json_string(&row.claim_state),
                    row.score
                        .map(|score| format!("{score:.6}"))
                        .unwrap_or_else(|| "null".to_string()),
                    json_option(&row.score_unit),
                    json_option_bool(row.local_pass),
                    json_option_u32(row.expected_matches),
                    json_option_u32(row.expected_total),
                    json_option_u32(row.forbidden_matches),
                    ledger::json_string(&row.harness_ref),
                    json_option(&row.dataset_ref),
                    json_option(&row.backend_id),
                    json_option_f64(row.latency_ms),
                    json_option_f64(row.tokens_per_second),
                    json_option_u32(row.prompt_tokens),
                    json_option_u32(row.completion_tokens),
                    json_option_u32(row.total_tokens),
                    json_option(&row.resource_pressure),
                    json_option_u64(row.peak_rss_bytes),
                    row.recorded_at_ms,
                    json_raw_or_string(&row.reproducibility_manifest),
                    json_raw_or_string(&row.redacted_report)
                ));
            }
            Ok(output)
        }
    }
}

fn score_response(fixture: &BenchmarkFixture, response: &str) -> BenchmarkScore {
    benchmark_policy::score_response(
        BenchmarkScoringPolicy {
            fixture_id: &fixture.fixture_id,
            expected_markers: &fixture.expected_response_contains,
            forbidden_markers: &fixture.forbidden_response_contains,
            abstention_required: fixture.abstention_required,
            minimum_score: fixture.minimum_score,
        },
        response,
    )
}

#[cfg(test)]
#[path = "benchmark/tests.rs"]
mod tests;
