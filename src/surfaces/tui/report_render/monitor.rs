use super::super::render::{
    bytes_label, latency_label, percent_label, push_footer, push_header, push_kv, push_rule,
    push_section, push_wrapped, short_id, tps_label,
};
use super::super::view_model::MonitorReportView;

pub(crate) fn render_monitor_report(width: usize, view: &MonitorReportView) -> String {
    let mut lines = Vec::new();
    push_header(&mut lines, width, "rpotato TUI beta - monitor");
    push_kv(&mut lines, width, "observability", &view.store.path);
    push_kv(
        &mut lines,
        width,
        "schema",
        &format!("v{}", view.store.migration_version),
    );
    push_kv(
        &mut lines,
        width,
        "model runs",
        &view.store.model_runs.to_string(),
    );
    push_kv(
        &mut lines,
        width,
        "token records",
        &view.store.token_records.to_string(),
    );
    push_kv(
        &mut lines,
        width,
        "transcript records",
        &view.store.transcript_records.to_string(),
    );
    push_kv(
        &mut lines,
        width,
        "resource samples",
        &view.store.resource_samples.to_string(),
    );
    push_rule(&mut lines, width);
    push_section(&mut lines, width, "resource pressure");
    if let Some(sample) = &view.resource {
        push_wrapped(
            &mut lines,
            width,
            &format!(
                "pressure: {} | backend: {} | pid: {} | sample count: {} | recorded ms: {}",
                sample.pressure_status,
                sample.backend_id,
                sample.pid,
                sample.sample_count,
                sample.recorded_at_ms
            ),
        );
        push_wrapped(
            &mut lines,
            width,
            &format!(
                "cpu: {} | avg rss: {}",
                percent_label(sample.process_cpu_percent),
                bytes_label(sample.average_rss_bytes)
            ),
        );
        push_wrapped(
            &mut lines,
            width,
            &format!(
                "peak rss: {} | disk: {}",
                bytes_label(sample.peak_rss_bytes),
                bytes_label(sample.disk_bytes)
            ),
        );
        push_wrapped(
            &mut lines,
            width,
            &format!("latest sample: {}", short_id(&sample.resource_sample_id)),
        );
    } else {
        push_wrapped(
            &mut lines,
            width,
            "No resource samples yet. Run backend start, backend status, or backend chat after a sidecar is running.",
        );
    }
    push_rule(&mut lines, width);
    push_section(&mut lines, width, "models");
    if view.models.is_empty() {
        push_wrapped(
            &mut lines,
            width,
            &format!(
                "No recorded model runs yet. Candidate state: {}",
                view.candidate_summary
            ),
        );
    } else {
        push_wrapped(
            &mut lines,
            width,
            "model | runs | prompt | completion | total | avg ms | tps",
        );
        for summary in &view.models {
            push_wrapped(
                &mut lines,
                width,
                &format!(
                    "{} | {} | {} | {} | {} | {} | {}",
                    summary.model_id,
                    summary.runs,
                    summary.prompt_tokens,
                    summary.completion_tokens,
                    summary.total_tokens,
                    latency_label(summary.avg_latency_ms),
                    tps_label(summary.avg_tokens_per_second)
                ),
            );
        }
    }
    push_rule(&mut lines, width);
    push_kv(
        &mut lines,
        width,
        "actions",
        "read-only; export/prune remain monitor CLI commands",
    );
    push_footer(&mut lines, width);
    lines.join("\n")
}
