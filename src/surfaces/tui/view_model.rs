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
