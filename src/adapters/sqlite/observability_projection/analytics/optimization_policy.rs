//! Optimization policy projection from measured observability evidence.

use super::performance_baseline::performance_baseline;
use super::statistics::average;
use super::*;

pub(in crate::adapters::sqlite::observability_projection) fn optimization_policy(
    ledger: &dyn CanonicalProjectionReadPort,
) -> Result<OptimizationPolicy, AppError> {
    let baseline = performance_baseline(ledger)?;
    let latest_resource = latest_resource_sample()?;
    let latest_resource_pressure = latest_resource
        .as_ref()
        .map(|sample| sample.pressure_status.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let benchmark_evidence = benchmark_evidence_summary(&benchmark_run_reports(ledger)?);
    let decision = resource::optimization_policy_decision(resource::OptimizationPolicyInput {
        pressure: resource_pressure_from_status(&latest_resource_pressure),
        model_runs: baseline.model_runs,
        measured_benchmark_runs: benchmark_evidence.measured_runs,
        failed_benchmark_runs: benchmark_evidence.failed_runs,
        context_clamp_count: baseline.context_clamp_count,
        p95_latency_ms: baseline.p95_latency_ms,
        avg_tokens_per_second: baseline.avg_tokens_per_second,
    });

    Ok(OptimizationPolicy {
        store: baseline.store.clone(),
        model_runs: baseline.model_runs,
        resource_samples: baseline.resource_samples,
        latest_resource_pressure,
        context_clamp_count: baseline.context_clamp_count,
        context_tokens_dropped: baseline.context_tokens_dropped,
        p95_latency_ms: baseline.p95_latency_ms,
        avg_tokens_per_second: baseline.avg_tokens_per_second,
        peak_rss_bytes: baseline.peak_rss_bytes,
        benchmark_evidence,
        decision,
    })
}

fn benchmark_evidence_summary(rows: &[BenchmarkRunReport]) -> BenchmarkEvidenceSummary {
    let measured = rows
        .iter()
        .filter(|row| row.claim_state == "measured-locally")
        .collect::<Vec<_>>();
    let scores = measured
        .iter()
        .filter_map(|row| row.score)
        .filter(|score| score.is_finite())
        .collect::<Vec<_>>();
    let latest = measured.iter().max_by(|left, right| {
        left.recorded_at_ms
            .cmp(&right.recorded_at_ms)
            .then_with(|| left.benchmark_run_id.cmp(&right.benchmark_run_id))
    });

    BenchmarkEvidenceSummary {
        measured_runs: measured.len(),
        passed_runs: measured
            .iter()
            .filter(|row| row.local_pass == Some(true))
            .count(),
        failed_runs: measured
            .iter()
            .filter(|row| row.local_pass == Some(false))
            .count(),
        avg_score: average(&scores),
        latest_benchmark_run_id: latest.map(|row| row.benchmark_run_id.clone()),
        latest_model_id: latest.map(|row| row.model_id.clone()),
        latest_benchmark_name: latest.map(|row| row.benchmark_name.clone()),
    }
}

fn resource_pressure_from_status(value: &str) -> resource::ResourcePressure {
    match value {
        "normal" => resource::ResourcePressure::Normal,
        "degraded" => resource::ResourcePressure::Degraded,
        "critical" => resource::ResourcePressure::Critical,
        _ => resource::ResourcePressure::Unknown,
    }
}
