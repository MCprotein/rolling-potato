use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::foundation::error::AppError;
use crate::runtime_core::observability::facade::{
    ModelMetricSummary, OptimizationPolicy, PerformanceBaseline, PrunePreview,
    ResourceSampleMetric, StoreStatus,
};
use crate::runtime_core::observability::html::{self, HtmlReportSnapshot, ReportData};

pub(crate) enum MonitorExportFormat {
    Jsonl,
    Csv,
    Html,
}

pub(crate) trait MonitorQueryPort {
    fn status(&self) -> Result<StoreStatus, AppError>;

    fn latest_resource_sample(&self) -> Result<Option<ResourceSampleMetric>, AppError>;

    fn runtime_ledger_path(&self) -> PathBuf;

    fn runtime_evidence_path(&self) -> PathBuf;

    fn model_summaries(&self) -> Result<Vec<ModelMetricSummary>, AppError>;

    fn model_candidate_summary(&self) -> String;

    fn performance_baseline(&self) -> Result<PerformanceBaseline, AppError>;

    fn optimization_policy(&self) -> Result<OptimizationPolicy, AppError>;

    fn export_jsonl(&self) -> Result<String, AppError>;

    fn export_csv(&self) -> Result<String, AppError>;

    fn prune_preview(&self, before_days: u64) -> Result<PrunePreview, AppError>;
}

pub(crate) fn status_report(port: &impl MonitorQueryPort) -> Result<String, AppError> {
    let store = port.status()?;
    let latest_resource = port.latest_resource_sample()?;
    let recovered = store
        .recovered_from
        .as_ref()
        .map(|path| format!("\n- recovered corrupt db: {}", path.display()))
        .unwrap_or_default();

    Ok(format!(
        "monitor 상태\n- observability store: {}\n- schema migration: v{}\n- runtime ledger: {}\n- runtime evidence: {}\n- ledger events: {}\n- sessions: {}\n- workflows: {}\n- transcript records: {}\n- model runs: {}\n- token usage records: {}\n- resource samples: {}\n- benchmark runs: {}\n- latest resource pressure: {}\n- latest resource cpu percent: {}\n- latest resource average rss bytes: {}\n- latest resource peak rss bytes: {}\n- latest resource disk bytes: {}\n- evidence records: {}\n- stop gate results: {}\n- transcript 저장: user/visible model/normalized tool/evidence만 영속화; hidden model response와 raw source는 저장하지 않음{}",
        store.path.display(),
        store.migration_version,
        port.runtime_ledger_path().display(),
        port.runtime_evidence_path().display(),
        store.ledger_events,
        store.sessions,
        store.workflows,
        store.transcript_records,
        store.model_runs,
        store.token_records,
        store.resource_samples,
        store.benchmark_runs,
        latest_resource
            .as_ref()
            .map(|sample| sample.pressure_status.as_str())
            .unwrap_or("없음"),
        display_optional_f64(
            latest_resource
                .as_ref()
                .and_then(|sample| sample.process_cpu_percent)
        ),
        display_optional_u64(
            latest_resource
                .as_ref()
                .and_then(|sample| sample.average_rss_bytes)
        ),
        display_optional_u64(
            latest_resource
                .as_ref()
                .and_then(|sample| sample.peak_rss_bytes)
        ),
        display_optional_u64(
            latest_resource
                .as_ref()
                .and_then(|sample| sample.disk_bytes)
        ),
        store.evidence_records,
        store.stop_gate_results,
        recovered
    ))
}

pub(crate) fn models_report(port: &impl MonitorQueryPort) -> Result<String, AppError> {
    let summaries = port.model_summaries()?;
    if summaries.is_empty() {
        return Ok(format!(
            "model monitoring\n- model candidates: {}\n- recorded model runs: 없음\n- token/latency metric: schema 준비됨, 실제 실행 기록은 backend runtime 이후 생성\n- resource samples: monitor status에서 확인",
            port.model_candidate_summary()
        ));
    }

    let rows = summaries
        .iter()
        .map(|summary| {
            let latency = summary
                .avg_latency_ms
                .map(|value| format!("{value:.1}ms"))
                .unwrap_or_else(|| "미기록".to_string());
            format!(
                "- {}: runs {}, prompt {}, completion {}, total {}, avg latency {}, avg tps {}",
                summary.model_id,
                summary.runs,
                summary.prompt_tokens,
                summary.completion_tokens,
                summary.total_tokens,
                latency,
                tps_label(summary.avg_tokens_per_second)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    Ok(format!("model monitoring\n{rows}"))
}

pub(crate) fn baseline_report(port: &impl MonitorQueryPort) -> Result<String, AppError> {
    let baseline = port.performance_baseline()?;
    let pressure_rows = if baseline.pressure_states.is_empty() {
        "- 없음".to_string()
    } else {
        baseline
            .pressure_states
            .iter()
            .map(|state| format!("- {}: {} samples", state.pressure_status, state.samples))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let group_rows = if baseline.groups.is_empty() {
        "- 없음".to_string()
    } else {
        baseline
            .groups
            .iter()
            .take(10)
            .map(|group| {
                format!(
                    "- model={} backend={} session={} runs={} total_tokens={} clamp_count={} dropped_tokens={} p50_latency={} p95_latency={} avg_tps={}",
                    group.model_id,
                    group.backend_id,
                    group.session_id,
                    group.runs,
                    group.total_tokens,
                    group.context_clamp_count,
                    group.context_tokens_dropped,
                    ms_label(group.p50_latency_ms),
                    ms_label(group.p95_latency_ms),
                    tps_label(group.avg_tokens_per_second)
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let recovered = baseline
        .store
        .recovered_from
        .as_ref()
        .map(|path| format!("\n- recovered corrupt db: {}", path.display()))
        .unwrap_or_default();

    Ok(format!(
        "performance baseline\n- observability store: {}\n- model runs: {}\n- token usage records: {}\n- resource samples: {}\n- total prompt tokens: {}\n- total completion tokens: {}\n- total tokens: {}\n- context clamp count: {}\n- context tokens dropped: {}\n- p50 latency: {}\n- p95 latency: {}\n- avg tokens/sec: {}\n- peak RSS bytes: {}\n- pressure states:\n{}\n- model/backend/session groups:\n{}\n- raw prompt/source 저장: 없음\n- boundary: local metric report only; model artifact 선택이나 capability claim을 하지 않음{}",
        baseline.store.path.display(),
        baseline.model_runs,
        baseline.token_records,
        baseline.resource_samples,
        baseline.total_prompt_tokens,
        baseline.total_completion_tokens,
        baseline.total_tokens,
        baseline.context_clamp_count,
        baseline.context_tokens_dropped,
        ms_label(baseline.p50_latency_ms),
        ms_label(baseline.p95_latency_ms),
        tps_label(baseline.avg_tokens_per_second),
        display_optional_u64(baseline.peak_rss_bytes),
        pressure_rows,
        group_rows,
        recovered
    ))
}

pub(crate) fn optimize_report(port: &impl MonitorQueryPort) -> Result<String, AppError> {
    let policy = port.optimization_policy()?;
    let recovered = policy
        .store
        .recovered_from
        .as_ref()
        .map(|path| format!("\n- recovered corrupt db: {}", path.display()))
        .unwrap_or_default();

    Ok(format!(
        "optimization policy\n- status: {}\n- observability store: {}\n- evidence source: local SQLite projection\n- model runs: {}\n- resource samples: {}\n- latest resource pressure: {}\n- context clamp count: {}\n- context tokens dropped: {}\n- p95 latency: {}\n- avg tokens/sec: {}\n- peak RSS bytes: {}\n- measured benchmark runs: {}\n- benchmark passed: {}\n- benchmark failed: {}\n- avg benchmark score: {}\n- latest benchmark run: {}\n- latest benchmark model: {}\n- latest benchmark name: {}\n- recommended context tokens: {}\n- recommended team lanes: {}\n- fallback: {}\n- model route hint: {}\n- reason: {}\n- hint: {}\n- raw prompt/source 저장: 없음\n- boundary: local policy recommendation only; does not select a real model artifact, promote a model to verified, or claim public benchmark parity.{}",
        policy.decision.status.as_str(),
        policy.store.path.display(),
        policy.model_runs,
        policy.resource_samples,
        policy.latest_resource_pressure,
        policy.context_clamp_count,
        policy.context_tokens_dropped,
        ms_label(policy.p95_latency_ms),
        tps_label(policy.avg_tokens_per_second),
        display_optional_u64(policy.peak_rss_bytes),
        policy.benchmark_evidence.measured_runs,
        policy.benchmark_evidence.passed_runs,
        policy.benchmark_evidence.failed_runs,
        score_label(policy.benchmark_evidence.avg_score),
        display_optional_str(
            policy
                .benchmark_evidence
                .latest_benchmark_run_id
                .as_deref()
        ),
        display_optional_str(policy.benchmark_evidence.latest_model_id.as_deref()),
        display_optional_str(policy.benchmark_evidence.latest_benchmark_name.as_deref()),
        display_optional_u32(policy.decision.recommended_context_tokens),
        policy.decision.recommended_lanes,
        policy.decision.fallback,
        policy.decision.model_hint.as_str(),
        policy.decision.reason,
        policy.decision.hint,
        recovered
    ))
}

pub(crate) fn export_report(
    port: &impl MonitorQueryPort,
    format: MonitorExportFormat,
) -> Result<String, AppError> {
    match format {
        MonitorExportFormat::Jsonl => port.export_jsonl(),
        MonitorExportFormat::Csv => port.export_csv(),
        MonitorExportFormat::Html => html_report(port),
    }
}

fn html_report(port: &impl MonitorQueryPort) -> Result<String, AppError> {
    let optimization_policy = port.optimization_policy()?;
    let store = optimization_policy.store.clone();
    let latest_resource = match port.latest_resource_sample() {
        Ok(value) => ReportData::Available(value),
        Err(_) => ReportData::Unavailable,
    };
    let model_summaries = match port.model_summaries() {
        Ok(value) => ReportData::Available(value),
        Err(_) => ReportData::Unavailable,
    };
    let generated_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);

    Ok(html::render_report(&HtmlReportSnapshot {
        generated_at_ms,
        store,
        latest_resource,
        model_summaries,
        model_candidate_summary: port.model_candidate_summary(),
        optimization_policy,
    }))
}

pub(crate) fn prune_report(
    port: &impl MonitorQueryPort,
    before_days: u64,
    dry_run: bool,
) -> Result<String, AppError> {
    let preview = port.prune_preview(before_days)?;
    let mode = if dry_run {
        "dry-run"
    } else {
        "blocked: dry-run only"
    };

    Ok(format!(
        "monitor prune 계획\n- mode: {}\n- before: {}d\n- cutoff_ms: {}\n- ledger rows: {}\n- model run rows: {}\n- command run rows: {}\n- resource sample rows: {}\n- 동작: 실제 삭제는 아직 수행하지 않습니다.",
        mode,
        before_days,
        preview.cutoff_ms,
        preview.ledger_rows,
        preview.model_run_rows,
        preview.command_run_rows,
        preview.resource_sample_rows
    ))
}

fn display_optional_f64(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.1}"))
        .unwrap_or_else(|| "없음".to_string())
}

fn display_optional_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "없음".to_string())
}

fn display_optional_u32(value: Option<u32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "없음".to_string())
}

fn display_optional_str(value: Option<&str>) -> String {
    value.unwrap_or("없음").to_string()
}

fn ms_label(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.1}ms"))
        .unwrap_or_else(|| "미기록".to_string())
}

fn tps_label(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.1} tok/s"))
        .unwrap_or_else(|| "미기록".to_string())
}

fn score_label(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.2}/3"))
        .unwrap_or_else(|| "미기록".to_string())
}

#[cfg(test)]
mod tests {
    use crate::runtime_core::inference::resource::{
        ModelRouteHint, OptimizationPolicyDecision, OptimizationPolicyStatus,
    };
    use crate::runtime_core::observability::facade::BenchmarkEvidenceSummary;

    use super::*;

    struct FakePort;

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
}
