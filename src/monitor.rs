use crate::app::AppError;
use crate::cli::MonitorExportFormat;
use crate::{model, observability, paths};

pub fn status_report() -> Result<String, AppError> {
    let store = observability::status()?;
    let latest_resource = observability::latest_resource_sample()?;
    let recovered = store
        .recovered_from
        .as_ref()
        .map(|path| format!("\n- recovered corrupt db: {}", path.display()))
        .unwrap_or_default();

    Ok(format!(
        "monitor 상태\n- observability store: {}\n- schema migration: v{}\n- runtime ledger: {}\n- runtime evidence: {}\n- ledger events: {}\n- sessions: {}\n- workflows: {}\n- model runs: {}\n- token usage records: {}\n- resource samples: {}\n- benchmark runs: {}\n- latest resource pressure: {}\n- latest resource cpu percent: {}\n- latest resource average rss bytes: {}\n- latest resource peak rss bytes: {}\n- latest resource disk bytes: {}\n- evidence records: {}\n- stop gate results: {}\n- raw prompt/source 저장: 기본 비활성{}",
        store.path.display(),
        store.migration_version,
        paths::runtime_ledger_file().display(),
        paths::runtime_evidence_file().display(),
        store.ledger_events,
        store.sessions,
        store.workflows,
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

pub fn models_report() -> Result<String, AppError> {
    let summaries = observability::model_summaries()?;
    if summaries.is_empty() {
        return Ok(format!(
            "model monitoring\n- model candidates: {}\n- recorded model runs: 없음\n- token/latency metric: schema 준비됨, 실제 실행 기록은 backend runtime 이후 생성\n- resource samples: monitor status에서 확인",
            model::candidate_summary()
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

pub fn baseline_report() -> Result<String, AppError> {
    let baseline = observability::performance_baseline()?;
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

pub fn export_report(format: MonitorExportFormat) -> Result<String, AppError> {
    match format {
        MonitorExportFormat::Jsonl => observability::export_jsonl(),
        MonitorExportFormat::Csv => observability::export_csv(),
    }
}

pub fn prune_report(before_days: u64, dry_run: bool) -> Result<String, AppError> {
    let preview = observability::prune_preview(before_days)?;
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
