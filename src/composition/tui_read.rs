use crate::foundation::error::AppError;
use crate::runtime_core::knowledge::evidence::EvidenceStoreStatus;
use crate::runtime_core::observability::facade::{MonitorProjectionSnapshot, StoreStatus};
use crate::runtime_core::patch::proposal::PatchProposalDetail;
use crate::runtime_core::workflow::domain::snapshot::TuiStateSnapshot;
use crate::runtime_core::workflow::domain::transcript::ToolOutputView;
use crate::runtime_core::workflow::storage_compat::ledger::ParsedLedgerEvent;
use crate::runtime_core::workflow::storage_compat::record::WorkflowRecord;
use crate::runtime_core::workflow::storage_compat::transcript::TranscriptRecord;
use crate::surfaces::tui::page::ProjectionStatus;
use crate::surfaces::tui::runtime_bridge::{TuiReadPage, TuiReadRequest};

#[path = "tui_read/common.rs"]
mod common;
#[path = "tui_read/review.rs"]
mod review;
#[path = "tui_read/state.rs"]
mod state;
#[path = "tui_read/transcript.rs"]
mod transcript;

pub(crate) trait TuiReadPort {
    fn state_snapshot(&mut self, max_ledger_events: usize) -> Result<TuiStateSnapshot, AppError>;
    fn store_status(&mut self) -> Result<StoreStatus, AppError>;
    fn monitor_snapshot(&mut self, limit: usize) -> Result<MonitorProjectionSnapshot, AppError>;
    fn transcript_record(
        &mut self,
        event: &ParsedLedgerEvent,
    ) -> Result<TranscriptRecord, AppError>;
    fn tool_output_view(
        &mut self,
        record: &TranscriptRecord,
        artifact_id: &str,
    ) -> Result<ToolOutputView, AppError>;
    fn proposal_detail(
        &mut self,
        workflow: &WorkflowRecord,
        proposal_id: &str,
        max_bytes: usize,
    ) -> Result<PatchProposalDetail, AppError>;
    fn evidence_status(
        &mut self,
        max_entries: usize,
        max_bytes: u64,
    ) -> Result<EvidenceStoreStatus, AppError>;
    fn content_hash(&mut self, value: &str) -> String;
    fn projection_status(&mut self, project_id: &str) -> ProjectionStatus;
}

pub(crate) fn read_tui_page(
    port: &mut impl TuiReadPort,
    request: TuiReadRequest,
) -> Result<TuiReadPage, AppError> {
    match request {
        TuiReadRequest::Overview { budget } => state::overview(port, budget),
        TuiReadRequest::Monitor { budget } => state::monitor(port, budget),
        TuiReadRequest::Sessions { page, budget } => state::sessions(port, page, budget),
        TuiReadRequest::Transcript {
            session_id,
            page,
            budget,
        } => transcript::transcript(port, session_id, page, budget),
        TuiReadRequest::ToolOutput {
            artifact_id,
            page,
            budget,
        } => transcript::tool_output(port, artifact_id, page, budget),
        TuiReadRequest::Approvals { page, budget } => review::approvals(port, page, budget),
        TuiReadRequest::Diff {
            proposal_id,
            page,
            budget,
        } => review::diff(port, proposal_id, page, budget),
        TuiReadRequest::Evidence { page, budget } => review::evidence(port, page, budget),
    }
}
