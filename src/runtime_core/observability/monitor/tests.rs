use std::path::PathBuf;

use crate::foundation::error::AppError;
use crate::runtime_core::inference::resource::{
    ModelRouteHint, OptimizationPolicyDecision, OptimizationPolicyStatus,
};
use crate::runtime_core::observability::facade::{
    BenchmarkEvidenceSummary, ModelMetricSummary, OptimizationPolicy, PerformanceBaseline,
    PrunePreview, ResourceSampleMetric, StoreStatus,
};

use super::*;

struct FakePort;
struct FailingReportPort;

impl MonitorQueryPort for FakePort {
    fn status(&self) -> Result<StoreStatus, AppError> {
        Ok(StoreStatus {
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
        })
    }

    fn latest_resource_sample(&self) -> Result<Option<ResourceSampleMetric>, AppError> {
        Ok(Some(ResourceSampleMetric {
            resource_sample_id: "sample-1".to_owned(),
            session_id: "session-1".to_owned(),
            backend_id: "backend-1".to_owned(),
            pid: 42,
            process_cpu_percent: Some(12.5),
            average_rss_bytes: Some(100),
            peak_rss_bytes: Some(200),
            disk_bytes: Some(300),
            sample_count: 2,
            pressure_status: "normal".to_owned(),
            recorded_at_ms: 1,
        }))
    }

    fn runtime_ledger_path(&self) -> PathBuf {
        PathBuf::from("/state/runtime-ledger.jsonl")
    }

    fn runtime_evidence_path(&self) -> PathBuf {
        PathBuf::from("/state/runtime-evidence.jsonl")
    }

    fn model_summaries(&self) -> Result<Vec<ModelMetricSummary>, AppError> {
        Ok(Vec::new())
    }

    fn model_candidate_summary(&self) -> String {
        "candidate-a".to_owned()
    }

    fn performance_baseline(&self) -> Result<PerformanceBaseline, AppError> {
        Err(AppError::blocked("unused fake performance baseline"))
    }

    fn optimization_policy(&self) -> Result<OptimizationPolicy, AppError> {
        Ok(OptimizationPolicy {
            store: self.status()?,
            model_runs: 5,
            resource_samples: 7,
            latest_resource_pressure: "normal".to_owned(),
            context_clamp_count: 1,
            context_tokens_dropped: 2,
            p95_latency_ms: Some(30.0),
            avg_tokens_per_second: Some(8.0),
            peak_rss_bytes: Some(200),
            benchmark_evidence: BenchmarkEvidenceSummary {
                measured_runs: 1,
                passed_runs: 1,
                failed_runs: 0,
                avg_score: Some(3.0),
                latest_benchmark_run_id: Some("benchmark-1".to_owned()),
                latest_model_id: Some("model-a".to_owned()),
                latest_benchmark_name: Some("smoke".to_owned()),
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
        })
    }

    fn export_jsonl(&self) -> Result<String, AppError> {
        Ok("jsonl".to_owned())
    }

    fn export_csv(&self) -> Result<String, AppError> {
        Ok("csv".to_owned())
    }

    fn prune_preview(&self, before_days: u64) -> Result<PrunePreview, AppError> {
        Ok(PrunePreview {
            cutoff_ms: u128::from(before_days),
            ledger_rows: 1,
            model_run_rows: 2,
            command_run_rows: 3,
            resource_sample_rows: 4,
        })
    }
}

impl MonitorQueryPort for FailingReportPort {
    fn status(&self) -> Result<StoreStatus, AppError> {
        Err(AppError::runtime("status unavailable"))
    }

    fn latest_resource_sample(&self) -> Result<Option<ResourceSampleMetric>, AppError> {
        Err(AppError::runtime("resource unavailable"))
    }

    fn runtime_ledger_path(&self) -> PathBuf {
        PathBuf::new()
    }

    fn runtime_evidence_path(&self) -> PathBuf {
        PathBuf::new()
    }

    fn model_summaries(&self) -> Result<Vec<ModelMetricSummary>, AppError> {
        Err(AppError::runtime("models unavailable"))
    }

    fn model_candidate_summary(&self) -> String {
        "candidate unavailable".to_owned()
    }

    fn performance_baseline(&self) -> Result<PerformanceBaseline, AppError> {
        Err(AppError::runtime("baseline unavailable"))
    }

    fn optimization_policy(&self) -> Result<OptimizationPolicy, AppError> {
        Err(AppError::runtime("policy unavailable"))
    }

    fn export_jsonl(&self) -> Result<String, AppError> {
        Err(AppError::runtime("jsonl unavailable"))
    }

    fn export_csv(&self) -> Result<String, AppError> {
        Err(AppError::runtime("csv unavailable"))
    }

    fn prune_preview(&self, _before_days: u64) -> Result<PrunePreview, AppError> {
        Err(AppError::runtime("prune unavailable"))
    }
}

#[test]
fn status_report_is_rendered_from_port_data() {
    let report = status_report(&FakePort).unwrap();

    assert!(report.contains("- observability store: /state/observability.sqlite"));
    assert!(report.contains("- runtime ledger: /state/runtime-ledger.jsonl"));
    assert!(report.contains("- ledger events: 11"));
    assert!(report.contains("- latest resource pressure: normal"));
    assert!(report.contains("- latest resource peak rss bytes: 200"));
}

#[test]
fn models_export_and_prune_use_cases_stay_surface_neutral() {
    assert!(models_report(&FakePort).unwrap().contains("candidate-a"));
    assert_eq!(
        export_report(&FakePort, MonitorExportFormat::Jsonl).unwrap(),
        "jsonl"
    );
    assert_eq!(
        export_report(&FakePort, MonitorExportFormat::Csv).unwrap(),
        "csv"
    );
    let html = export_report(&FakePort, MonitorExportFormat::Html).unwrap();
    assert!(html.starts_with("<!doctype html>"));
    assert!(html.contains("Content-Security-Policy"));
    let prune = prune_report(&FakePort, 30, true).unwrap();
    assert!(prune.contains("- mode: dry-run"));
    assert!(prune.contains("- cutoff_ms: 30"));
    assert!(prune.contains("- resource sample rows: 4"));
}

#[test]
fn html_export_preserves_all_sections_when_queries_are_unavailable() {
    let html = export_report(&FailingReportPort, MonitorExportFormat::Html).unwrap();

    assert!(html.starts_with("<!doctype html>"));
    assert!(html.contains("observability store 상태를 읽지 못했습니다"));
    assert!(html.contains("resource metric을 읽지 못했습니다"));
    assert!(html.contains("model metric을 읽지 못했습니다"));
    assert!(html.contains("performance/optimization policy를 읽지 못했습니다"));
    assert!(html.ends_with("</html>\n"));
}
