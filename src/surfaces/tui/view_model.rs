use super::runtime_bridge::{TuiReadBudget, TuiReadRequest};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InteractiveView {
    Overview,
    Monitor,
    Sessions,
    Transcript(String),
    ToolOutput(String),
    Approvals,
    Diff(String),
    Evidence,
}

pub(crate) struct InteractiveState {
    pub(crate) view: InteractiveView,
    pub(crate) page: u64,
    pub(crate) selected_id: Option<String>,
    pub(crate) notice: String,
}

impl InteractiveState {
    pub(crate) fn new() -> Self {
        Self {
            view: InteractiveView::Overview,
            page: 0,
            selected_id: None,
            notice: "help로 명령 목록을 확인하세요.".to_string(),
        }
    }

    pub(crate) fn set_view(&mut self, view: InteractiveView) {
        self.view = view;
        self.page = 0;
        self.notice = "화면을 변경했습니다.".to_string();
    }

    pub(crate) fn read_request(&self, width: u16, height: u16) -> TuiReadRequest {
        let items = usize::from(height.saturating_sub(6)).max(1);
        let chars = usize::from(width).saturating_mul(items).max(1);
        let budget = TuiReadBudget::bounded(items, chars);
        match &self.view {
            InteractiveView::Overview => TuiReadRequest::Overview { budget },
            InteractiveView::Monitor => TuiReadRequest::Monitor { budget },
            InteractiveView::Sessions => TuiReadRequest::Sessions {
                page: self.page,
                budget,
            },
            InteractiveView::Transcript(session_id) => TuiReadRequest::Transcript {
                session_id: session_id.clone(),
                page: self.page,
                budget,
            },
            InteractiveView::ToolOutput(artifact_id) => TuiReadRequest::ToolOutput {
                artifact_id: artifact_id.clone(),
                page: self.page,
                budget,
            },
            InteractiveView::Approvals => TuiReadRequest::Approvals {
                page: self.page,
                budget,
            },
            InteractiveView::Diff(proposal_id) => TuiReadRequest::Diff {
                proposal_id: proposal_id.clone(),
                page: self.page,
                budget,
            },
            InteractiveView::Evidence => TuiReadRequest::Evidence {
                page: self.page,
                budget,
            },
        }
    }
}

pub(crate) struct EvidenceReportView {
    pub(crate) project_root: String,
    pub(crate) session_id: String,
    pub(crate) runtime_evidence_file: String,
    pub(crate) runtime_evidence_records: usize,
    pub(crate) project_evidence_dir: String,
    pub(crate) project_artifacts: usize,
    pub(crate) observability_path: String,
    pub(crate) evidence_records: i64,
    pub(crate) stop_gate_results: i64,
    pub(crate) stale_policy: String,
}

pub(crate) struct SessionSummaryView {
    pub(crate) session_id: String,
    pub(crate) event_count: i64,
    pub(crate) last_summary: Option<String>,
}

pub(crate) struct SessionsReportView {
    pub(crate) project_root: String,
    pub(crate) current_session_id: String,
    pub(crate) state_path: String,
    pub(crate) sessions: Vec<SessionSummaryView>,
}

pub(crate) struct ModelMetricView {
    pub(crate) model_id: String,
    pub(crate) runs: i64,
    pub(crate) prompt_tokens: i64,
    pub(crate) completion_tokens: i64,
    pub(crate) total_tokens: i64,
    pub(crate) avg_latency_ms: Option<f64>,
    pub(crate) avg_tokens_per_second: Option<f64>,
}

pub(crate) struct OverviewStoreView {
    pub(crate) path: String,
    pub(crate) recovered_from: Option<String>,
    pub(crate) ledger_events: i64,
    pub(crate) sessions: i64,
    pub(crate) workflows: i64,
    pub(crate) transcript_records: i64,
}

pub(crate) struct OverviewReportView {
    pub(crate) project_root: String,
    pub(crate) session_id: String,
    pub(crate) store: OverviewStoreView,
    pub(crate) models: Vec<ModelMetricView>,
    pub(crate) candidate_summary: String,
    pub(crate) recent_sessions: Vec<SessionSummaryView>,
}

pub(crate) struct MonitorStoreView {
    pub(crate) path: String,
    pub(crate) migration_version: i64,
    pub(crate) model_runs: i64,
    pub(crate) token_records: i64,
    pub(crate) transcript_records: i64,
    pub(crate) resource_samples: i64,
}

pub(crate) struct ResourceSampleView {
    pub(crate) resource_sample_id: String,
    pub(crate) backend_id: String,
    pub(crate) pid: u32,
    pub(crate) process_cpu_percent: Option<f64>,
    pub(crate) average_rss_bytes: Option<u64>,
    pub(crate) peak_rss_bytes: Option<u64>,
    pub(crate) disk_bytes: Option<u64>,
    pub(crate) sample_count: u32,
    pub(crate) pressure_status: String,
    pub(crate) recorded_at_ms: u128,
}

pub(crate) struct MonitorReportView {
    pub(crate) store: MonitorStoreView,
    pub(crate) models: Vec<ModelMetricView>,
    pub(crate) resource: Option<ResourceSampleView>,
    pub(crate) candidate_summary: String,
}

pub(crate) struct TranscriptSessionView {
    pub(crate) project_root: String,
    pub(crate) session_id: String,
    pub(crate) started_at_ms: i64,
    pub(crate) last_event_at_ms: Option<i64>,
    pub(crate) event_count: i64,
}

pub(crate) struct TranscriptRecordView {
    pub(crate) kind: String,
    pub(crate) workflow_id: String,
    pub(crate) content: String,
}

pub(crate) struct TimelineEventView {
    pub(crate) event_id: String,
    pub(crate) ts_ms: i64,
    pub(crate) event_type: String,
    pub(crate) summary: String,
}

pub(crate) struct TranscriptReportView {
    pub(crate) session: TranscriptSessionView,
    pub(crate) records: Vec<TranscriptRecordView>,
    pub(crate) events: Vec<TimelineEventView>,
}
