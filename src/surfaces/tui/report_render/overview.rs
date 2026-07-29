use super::super::render::{
    latency_label, push_footer, push_header, push_kv, push_rule, push_section, push_wrapped,
    short_id, tps_label,
};
use super::super::view_model::OverviewReportView;

pub(crate) fn render_overview_report(width: usize, view: &OverviewReportView) -> String {
    let mut lines = Vec::new();
    push_header(&mut lines, width, "rpotato TUI beta - overview");
    push_kv(&mut lines, width, "project", &view.project_root);
    push_kv(&mut lines, width, "session", &view.session_id);
    push_kv(&mut lines, width, "mode", "read-only dashboard");
    push_rule(&mut lines, width);
    push_section(&mut lines, width, "runtime");
    push_kv(&mut lines, width, "observability", &view.store.path);
    push_kv(
        &mut lines,
        width,
        "ledger events",
        &view.store.ledger_events.to_string(),
    );
    push_kv(
        &mut lines,
        width,
        "sessions",
        &view.store.sessions.to_string(),
    );
    push_kv(
        &mut lines,
        width,
        "workflows",
        &view.store.workflows.to_string(),
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
        "transcript boundary",
        "visible/normalized turns persisted; hidden response and raw source excluded",
    );
    if let Some(path) = &view.store.recovered_from {
        push_kv(&mut lines, width, "recovered db", path);
    }
    push_rule(&mut lines, width);
    push_section(&mut lines, width, "model/token summary");
    if view.models.is_empty() {
        push_kv(
            &mut lines,
            width,
            "model runs",
            &format!("none; candidates {}", view.candidate_summary),
        );
    } else {
        for summary in view.models.iter().take(4) {
            push_wrapped(
                &mut lines,
                width,
                &format!(
                    "{} | runs {} | tokens {} | avg latency {} | avg tps {}",
                    summary.model_id,
                    summary.runs,
                    summary.total_tokens,
                    latency_label(summary.avg_latency_ms),
                    tps_label(summary.avg_tokens_per_second)
                ),
            );
        }
    }
    push_rule(&mut lines, width);
    push_section(&mut lines, width, "recent sessions");
    if view.recent_sessions.is_empty() {
        push_kv(&mut lines, width, "history", "none");
    } else {
        for session in view.recent_sessions.iter().take(3) {
            push_wrapped(
                &mut lines,
                width,
                &format!(
                    "{} | events {} | last {}",
                    short_id(&session.session_id),
                    session.event_count,
                    session
                        .last_summary
                        .as_deref()
                        .unwrap_or("no summary recorded")
                ),
            );
        }
    }
    push_rule(&mut lines, width);
    push_kv(
        &mut lines,
        width,
        "views",
        "rpotato tui | rpotato tui monitor | rpotato tui sessions | rpotato tui transcript <session-id> | rpotato tui approvals | rpotato tui evidence",
    );
    push_footer(&mut lines, width);
    lines.join("\n")
}
