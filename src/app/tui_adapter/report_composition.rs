use super::{canonical_read_page, AppError, TuiReadBudget, TuiReadRequest};
use crate::adapters::filesystem::layout as paths;
use crate::app::evidence_adapter as evidence;
use crate::app::inference_adapter::model;
use crate::app::observability_adapter as observability;
use crate::app::workflow_adapter::ledger;
use crate::app::workflow_adapter::transcript;
use crate::surfaces::tui::render::terminal_width;
use crate::surfaces::tui::report_render::{
    canonical_page_report, render_evidence_report, render_monitor_report, render_overview_report,
    render_sessions_report, render_transcript_report,
};
use crate::surfaces::tui::view_model::{
    EvidenceReportView, ModelMetricView, MonitorReportView, MonitorStoreView, OverviewReportView,
    OverviewStoreView, ResourceSampleView, SessionSummaryView, SessionsReportView,
    TimelineEventView, TranscriptRecordView, TranscriptReportView, TranscriptSessionView,
};

pub fn overview_report() -> Result<String, AppError> {
    let width = terminal_width();
    let store = observability::status()?;
    let models = observability::model_summaries()?;
    let sessions = observability::session_history(5)?;
    let identity = ledger::validated_current_identity()?;
    Ok(render_overview_report(
        width,
        &OverviewReportView {
            project_root: identity.project_root,
            session_id: identity.session_id,
            store: OverviewStoreView {
                path: store.path.display().to_string(),
                recovered_from: store.recovered_from.map(|path| path.display().to_string()),
                ledger_events: store.ledger_events,
                sessions: store.sessions,
                workflows: store.workflows,
                transcript_records: store.transcript_records,
            },
            models: models.into_iter().map(model_metric_view).collect(),
            candidate_summary: model::candidate_summary(),
            recent_sessions: sessions.into_iter().map(session_summary_view).collect(),
        },
    ))
}

pub fn monitor_report() -> Result<String, AppError> {
    let width = terminal_width();
    let store = observability::status()?;
    let models = observability::model_summaries()?;
    let resource = observability::latest_resource_sample()?;
    Ok(render_monitor_report(
        width,
        &MonitorReportView {
            store: MonitorStoreView {
                path: store.path.display().to_string(),
                migration_version: store.migration_version,
                model_runs: store.model_runs,
                token_records: store.token_records,
                transcript_records: store.transcript_records,
                resource_samples: store.resource_samples,
            },
            models: models.into_iter().map(model_metric_view).collect(),
            resource: resource.map(|sample| ResourceSampleView {
                resource_sample_id: sample.resource_sample_id,
                backend_id: sample.backend_id,
                pid: sample.pid,
                process_cpu_percent: sample.process_cpu_percent,
                average_rss_bytes: sample.average_rss_bytes,
                peak_rss_bytes: sample.peak_rss_bytes,
                disk_bytes: sample.disk_bytes,
                sample_count: sample.sample_count,
                pressure_status: sample.pressure_status,
                recorded_at_ms: sample.recorded_at_ms,
            }),
            candidate_summary: model::candidate_summary(),
        },
    ))
}

pub fn sessions_report() -> Result<String, AppError> {
    let width = terminal_width();
    let identity = ledger::validated_current_identity()?;
    let sessions = observability::session_history(10)?;
    Ok(render_sessions_report(
        width,
        &SessionsReportView {
            project_root: identity.project_root,
            current_session_id: identity.session_id,
            state_path: paths::current_state_file().display().to_string(),
            sessions: sessions
                .into_iter()
                .map(|session| SessionSummaryView {
                    session_id: session.session_id,
                    event_count: session.event_count,
                    last_summary: session.last_summary,
                })
                .collect(),
        },
    ))
}

pub fn transcript_report(session_id: &str) -> Result<String, AppError> {
    let width = terminal_width();
    let session = observability::session_entry(session_id)?.ok_or_else(|| {
        AppError::blocked(format!(
            "tui transcript 차단\n- session id: {}\n- 이유: 현재 project의 session history에서 찾지 못했습니다.\n- 확인: rpotato tui sessions",
            session_id
        ))
    })?;
    let events = observability::session_events(session_id, 40)?;
    let transcript = transcript::records_for_session(session_id)?;

    Ok(render_transcript_report(
        width,
        &TranscriptReportView {
            session: TranscriptSessionView {
                project_root: session.project_root,
                session_id: session.session_id,
                started_at_ms: session.started_at_ms,
                last_event_at_ms: session.last_event_at_ms,
                event_count: session.event_count,
            },
            records: transcript
                .into_iter()
                .map(|record| TranscriptRecordView {
                    kind: record.kind,
                    workflow_id: record.workflow_id,
                    content: record.content,
                })
                .collect(),
            events: events
                .into_iter()
                .map(|event| TimelineEventView {
                    event_id: event.event_id,
                    ts_ms: event.ts_ms,
                    event_type: event.event_type,
                    summary: event.summary,
                })
                .collect(),
        },
    ))
}

pub fn approvals_report() -> Result<String, AppError> {
    let page = canonical_read_page(TuiReadRequest::Approvals {
        page: 0,
        budget: TuiReadBudget::bounded(40, 64 * 1024),
    })?;
    Ok(canonical_page_report(page))
}

pub fn diff_report(proposal_id: &str) -> Result<String, AppError> {
    let page = canonical_read_page(TuiReadRequest::Diff {
        proposal_id: proposal_id.to_string(),
        page: 0,
        budget: TuiReadBudget::bounded(120, 64 * 1024),
    })?;
    Ok(canonical_page_report(page))
}

fn model_metric_view(
    summary: crate::runtime_core::observability::facade::ModelMetricSummary,
) -> ModelMetricView {
    ModelMetricView {
        model_id: summary.model_id,
        runs: summary.runs,
        prompt_tokens: summary.prompt_tokens,
        completion_tokens: summary.completion_tokens,
        total_tokens: summary.total_tokens,
        avg_latency_ms: summary.avg_latency_ms,
        avg_tokens_per_second: summary.avg_tokens_per_second,
    }
}

fn session_summary_view(session: observability::SessionHistoryEntry) -> SessionSummaryView {
    SessionSummaryView {
        session_id: session.session_id,
        event_count: session.event_count,
        last_summary: session.last_summary,
    }
}

pub fn evidence_report() -> Result<String, AppError> {
    let width = terminal_width();
    let identity = ledger::validated_current_identity()?;
    let store = observability::status()?;
    let evidence = evidence::store_status()?;
    Ok(render_evidence_report(
        width,
        &EvidenceReportView {
            project_root: identity.project_root,
            session_id: identity.session_id,
            runtime_evidence_file: evidence.runtime_evidence_file.display().to_string(),
            runtime_evidence_records: evidence.runtime_evidence_records,
            project_evidence_dir: evidence.project_evidence_dir.display().to_string(),
            project_artifacts: evidence.project_artifacts,
            observability_path: store.path.display().to_string(),
            evidence_records: store.evidence_records,
            stop_gate_results: store.stop_gate_results,
            stale_policy: evidence.stale_policy.to_string(),
        },
    ))
}
