use std::path::PathBuf;

use crate::runtime_core::inference::resource::{
    ModelRouteHint, OptimizationPolicyDecision, OptimizationPolicyStatus,
};
use crate::runtime_core::observability::facade::BenchmarkEvidenceSummary;

use super::*;

#[test]
fn report_is_self_contained_responsive_and_escapes_dynamic_values() {
    let mut snapshot = snapshot();
    snapshot.store = ReportData::Available(StoreStatus {
        path: PathBuf::from("/secret/private/observability.sqlite"),
        recovered_from: Some(PathBuf::from("/secret/private/recovered.sqlite")),
        ..store()
    });
    snapshot.model_candidate_summary = "api_key=top-secret".to_owned();
    snapshot.model_summaries = ReportData::Available(vec![
        ModelMetricSummary {
            model_id: "<script>alert('x')</script>&model".to_owned(),
            runs: 1,
            prompt_tokens: 2,
            completion_tokens: 3,
            total_tokens: 5,
            avg_latency_ms: Some(6.0),
            avg_tokens_per_second: Some(7.0),
        },
        ModelMetricSummary {
            model_id: "ghp_1234567890abcdef".to_owned(),
            runs: 1,
            prompt_tokens: 2,
            completion_tokens: 3,
            total_tokens: 5,
            avg_latency_ms: None,
            avg_tokens_per_second: None,
        },
        ModelMetricSummary {
            model_id: r"C:\Users\alice\private-model.gguf".to_owned(),
            runs: 1,
            prompt_tokens: 2,
            completion_tokens: 3,
            total_tokens: 5,
            avg_latency_ms: None,
            avg_tokens_per_second: None,
        },
    ]);
    if let ReportData::Available(policy) = &mut snapshot.optimization_policy {
        policy.benchmark_evidence.latest_model_id =
            Some("/Users/alice/private-model.gguf".to_owned());
        policy.benchmark_evidence.latest_benchmark_name =
            Some("Authorization: Bearer abc123".to_owned());
    }

    let report = render_report(&snapshot);

    assert!(report.starts_with("<!doctype html>"));
    assert!(report.contains("Content-Security-Policy"));
    assert!(report.contains("default-src 'none'"));
    assert!(report.contains("@media (max-width: 48rem)"));
    assert!(report.contains("&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt;&amp;model"));
    assert!(!report.contains("<script>"));
    assert!(!report.contains("https://"));
    assert!(!report.contains("http://"));
    assert!(!report.contains("/secret/private"));
    assert!(!report.contains("/Users/alice"));
    assert!(!report.contains(r"C:\Users\alice"));
    assert!(!report.contains("ghp_1234567890abcdef"));
    assert!(!report.contains("top-secret"));
    assert!(!report.contains("abc123"));
    assert!(report.contains("[REDACTED]"));
    assert!(report.contains("[REDACTED_PATH]"));
    assert!(report.contains("<main>"));
    assert!(report.contains("<caption>"));
    assert!(report.contains("raw prompt/source</dt><dd>저장·표시 안 함"));
}

#[test]
fn empty_and_unavailable_states_preserve_the_document() {
    let mut snapshot = snapshot();
    snapshot.store = ReportData::Unavailable;
    snapshot.latest_resource = ReportData::Unavailable;
    snapshot.model_summaries = ReportData::Available(Vec::new());
    snapshot.model_candidate_summary = "후보 <A>".to_owned();
    snapshot.optimization_policy = ReportData::Unavailable;

    let report = render_report(&snapshot);

    assert!(report.contains("observability store 상태를 읽지 못했습니다"));
    assert!(report.contains("resource metric을 읽지 못했습니다"));
    assert!(report.contains("기록된 model run이 없습니다"));
    assert!(report.contains("후보 &lt;A&gt;"));
    assert!(report.contains("performance/optimization policy를 읽지 못했습니다"));
    assert!(report.ends_with("</html>\n"));
}

fn snapshot() -> HtmlReportSnapshot {
    let store = store();
    HtmlReportSnapshot {
        generated_at_ms: 123,
        store: ReportData::Available(store.clone()),
        latest_resource: ReportData::Available(None),
        model_summaries: ReportData::Available(Vec::new()),
        model_candidate_summary: "candidate-a".to_owned(),
        optimization_policy: ReportData::Available(OptimizationPolicy {
            store,
            model_runs: 5,
            resource_samples: 7,
            latest_resource_pressure: "normal".to_owned(),
            context_clamp_count: 1,
            context_tokens_dropped: 12,
            p95_latency_ms: Some(42.0),
            avg_tokens_per_second: Some(8.0),
            peak_rss_bytes: Some(1024),
            benchmark_evidence: BenchmarkEvidenceSummary {
                measured_runs: 2,
                passed_runs: 1,
                failed_runs: 1,
                avg_score: Some(2.5),
                latest_benchmark_run_id: Some("benchmark-1".to_owned()),
                latest_model_id: Some("model-a".to_owned()),
                latest_benchmark_name: Some("local-smoke".to_owned()),
            },
            decision: OptimizationPolicyDecision {
                status: OptimizationPolicyStatus::Recommend,
                recommended_context_tokens: Some(2048),
                recommended_lanes: 2,
                fallback: "sequential",
                model_hint: ModelRouteHint::Keep,
                reason: "local evidence",
                hint: "keep measuring",
            },
        }),
    }
}

fn store() -> StoreStatus {
    StoreStatus {
        path: PathBuf::from("/state/observability.sqlite"),
        recovered_from: None,
        migration_version: 6,
        ledger_events: 11,
        sessions: 2,
        workflows: 3,
        transcript_records: 4,
        model_runs: 5,
        token_records: 6,
        resource_samples: 7,
        benchmark_runs: 8,
        evidence_records: 9,
        stop_gate_results: 10,
    }
}
