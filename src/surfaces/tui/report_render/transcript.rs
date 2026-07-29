use super::super::render::{
    push_footer, push_header, push_kv, push_rule, push_section, push_wrapped, short_id,
};
use super::super::view_model::TranscriptReportView;

pub(crate) fn render_transcript_report(width: usize, view: &TranscriptReportView) -> String {
    let session = &view.session;
    let mut lines = Vec::new();
    push_header(&mut lines, width, "rpotato TUI beta - transcript");
    push_kv(&mut lines, width, "project", &session.project_root);
    push_kv(&mut lines, width, "session", &session.session_id);
    push_kv(
        &mut lines,
        width,
        "started",
        &session.started_at_ms.to_string(),
    );
    push_kv(
        &mut lines,
        width,
        "last event",
        &session
            .last_event_at_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
    );
    push_kv(
        &mut lines,
        width,
        "events",
        &session.event_count.to_string(),
    );
    push_rule(&mut lines, width);
    push_section(&mut lines, width, "durable conversation");
    if view.records.is_empty() {
        push_wrapped(&mut lines, width, "No durable conversation turns recorded.");
    } else {
        for record in &view.records {
            push_wrapped(
                &mut lines,
                width,
                &format!(
                    "{} | {} | {}",
                    record.kind,
                    short_id(&record.workflow_id),
                    record.content
                ),
            );
        }
    }
    push_rule(&mut lines, width);
    push_section(&mut lines, width, "timeline");
    if view.events.is_empty() {
        push_wrapped(
            &mut lines,
            width,
            "No ledger events are projected for this session yet.",
        );
    } else {
        push_wrapped(&mut lines, width, "ts_ms | event type | event id | summary");
        for event in &view.events {
            push_wrapped(
                &mut lines,
                width,
                &format!(
                    "{} | {} | {} | {}",
                    event.ts_ms,
                    event.event_type,
                    short_id(&event.event_id),
                    event.summary
                ),
            );
        }
        if session.event_count > i64::try_from(view.events.len()).unwrap_or(i64::MAX) {
            push_wrapped(
                &mut lines,
                width,
                &format!(
                    "showing first {} projected events; total event count is {}",
                    view.events.len(),
                    session.event_count
                ),
            );
        }
    }
    push_rule(&mut lines, width);
    push_kv(
        &mut lines,
        width,
        "resume",
        &format!("rpotato session resume {}", session.session_id),
    );
    push_kv(
        &mut lines,
        width,
        "raw details",
        "not shown in the TUI beta by default",
    );
    push_footer(&mut lines, width);
    lines.join("\n")
}
