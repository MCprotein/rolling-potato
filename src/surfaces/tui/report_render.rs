use super::render::{
    bytes_label, latency_label, percent_label, push_footer, push_header, push_kv,
    push_literal_block, push_rule, push_section, push_wrapped, short_id, terminal_width, tps_label,
};
use super::runtime_bridge::TuiReadPage;
use super::view_model::{
    EvidenceReportView, MonitorReportView, OverviewReportView, SessionsReportView,
    TranscriptReportView,
};

pub(crate) fn canonical_page_report(page: TuiReadPage) -> String {
    let width = terminal_width();
    let literal_content = page.title == "diff";
    let mut lines = Vec::new();
    push_header(
        &mut lines,
        width,
        &format!("rpotato TUI beta - {}", page.title),
    );
    push_kv(&mut lines, width, "page", &(page.page + 1).to_string());
    push_kv(&mut lines, width, "freshness", page.freshness.as_str());
    push_kv(
        &mut lines,
        width,
        "continuation",
        page.continuation.as_str(),
    );
    push_rule(&mut lines, width);
    push_section(&mut lines, width, "canonical authority");
    push_kv(
        &mut lines,
        width,
        "current",
        &authority_pair(
            page.authority.current_revision,
            page.authority.current_hash.as_deref(),
        ),
    );
    push_kv(
        &mut lines,
        width,
        "workflow",
        &authority_pair(
            page.authority.workflow_revision,
            page.authority.workflow_hash.as_deref(),
        ),
    );
    push_kv(
        &mut lines,
        width,
        "ledger",
        &authority_pair(
            page.authority.ledger_sequence,
            page.authority.ledger_hash.as_deref(),
        ),
    );
    push_kv(
        &mut lines,
        width,
        "content hash",
        page.authority
            .content_hash
            .as_deref()
            .unwrap_or("unavailable"),
    );
    push_kv(
        &mut lines,
        width,
        "validated at ms",
        &page
            .authority
            .validated_at_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unavailable".to_string()),
    );
    push_rule(&mut lines, width);
    push_section(&mut lines, width, "content");
    if page.lines.is_empty() {
        push_wrapped(&mut lines, width, "No canonical records are available.");
    } else {
        for (index, line) in page.lines.iter().enumerate() {
            if literal_content && index > 0 {
                push_literal_block(&mut lines, width, line);
            } else {
                push_wrapped(&mut lines, width, line);
            }
        }
    }
    push_footer(&mut lines, width);
    lines.join("\n")
}

pub(crate) fn render_evidence_report(width: usize, view: &EvidenceReportView) -> String {
    let mut lines = Vec::new();
    push_header(&mut lines, width, "rpotato TUI beta - evidence");
    push_kv(&mut lines, width, "project", &view.project_root);
    push_kv(&mut lines, width, "session", &view.session_id);
    push_kv(&mut lines, width, "mode", "read-only evidence status");
    push_rule(&mut lines, width);
    push_section(&mut lines, width, "stores");
    push_kv(
        &mut lines,
        width,
        "runtime evidence",
        &view.runtime_evidence_file,
    );
    push_kv(
        &mut lines,
        width,
        "runtime records",
        &view.runtime_evidence_records.to_string(),
    );
    push_kv(
        &mut lines,
        width,
        "project evidence",
        &view.project_evidence_dir,
    );
    push_kv(
        &mut lines,
        width,
        "project artifacts",
        &view.project_artifacts.to_string(),
    );
    push_kv(&mut lines, width, "observability", &view.observability_path);
    push_rule(&mut lines, width);
    push_section(&mut lines, width, "stop gate boundary");
    push_kv(
        &mut lines,
        width,
        "recorded evidence",
        &view.evidence_records.to_string(),
    );
    push_kv(
        &mut lines,
        width,
        "stop gate results",
        &view.stop_gate_results.to_string(),
    );
    push_kv(&mut lines, width, "stale policy", &view.stale_policy);
    push_kv(
        &mut lines,
        width,
        "terminal gate",
        "not implemented; this view does not pass or fail workflows",
    );
    push_rule(&mut lines, width);
    push_kv(
        &mut lines,
        width,
        "validate",
        "rpotato evidence validate <artifact-pointer>",
    );
    push_kv(
        &mut lines,
        width,
        "raw prompt/source",
        "disabled by default",
    );
    push_footer(&mut lines, width);
    lines.join("\n")
}

pub(crate) fn render_sessions_report(width: usize, view: &SessionsReportView) -> String {
    let mut lines = Vec::new();
    push_header(&mut lines, width, "rpotato TUI beta - sessions");
    push_kv(&mut lines, width, "project", &view.project_root);
    push_kv(
        &mut lines,
        width,
        "current session",
        &view.current_session_id,
    );
    push_rule(&mut lines, width);
    if view.sessions.is_empty() {
        push_wrapped(
            &mut lines,
            width,
            "No session history yet. Start with `rpotato init` or `rpotato session new`.",
        );
    } else {
        push_wrapped(&mut lines, width, "session id | events | last summary");
        for session in &view.sessions {
            push_wrapped(
                &mut lines,
                width,
                &format!(
                    "{} | {} | {}",
                    session.session_id,
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
        "resume",
        "rpotato session resume <session-id>",
    );
    push_kv(
        &mut lines,
        width,
        "inspect",
        "rpotato tui transcript <session-id>",
    );
    push_kv(&mut lines, width, "state", &view.state_path);
    push_footer(&mut lines, width);
    lines.join("\n")
}

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

fn authority_pair(revision: Option<u64>, hash: Option<&str>) -> String {
    match (revision, hash) {
        (Some(revision), Some(hash)) => format!("revision={revision} hash={hash}"),
        _ => "unavailable".to_string(),
    }
}
