use crate::foundation::error::AppError;

use super::format::{
    display_optional_str, display_optional_u32, display_optional_u64, ms_label, score_label,
    tps_label,
};
use super::MonitorQueryPort;

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
