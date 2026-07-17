//! TUI runtime adapters and application entrypoints.

use crate::adapters::terminal::capability;
use crate::adapters::terminal::native::NativeTerminal;
use crate::app::workflow_adapter::transcript;
use crate::app::workflow_adapter::transition;
use crate::composition::tui_action::{self, TuiActionPort, TuiMutationFailure};
use crate::composition::tui_read::{self, TuiReadPort};
use crate::foundation::error::AppError;
pub(crate) use crate::surfaces::tui::controller::terminal_fault_error;
use crate::surfaces::tui::controller::{self, TuiRuntimePort};
use crate::surfaces::tui::outcome::TuiOutcome;
use crate::surfaces::tui::page::ProjectionStatus;
use crate::surfaces::tui::runtime_bridge::{
    new_tui_intent_id, SelectionLease, TuiGateKind, TuiIntent, TuiReadBudget, TuiReadPage,
    TuiReadRequest,
};

pub fn run_auto() -> Result<(), AppError> {
    if capability::attached() {
        let mut terminal = NativeTerminal::new();
        controller::run_controller(&mut terminal, &mut TuiRuntimeAdapter)
    } else {
        println!("{}", overview_report()?);
        Ok(())
    }
}

pub fn run_interactive() -> Result<(), AppError> {
    let mut terminal = NativeTerminal::explicit_line_mode();
    controller::run_controller(&mut terminal, &mut TuiRuntimeAdapter)
}

struct TuiRuntimeAdapter;

pub(crate) struct TuiReadAdapter;

struct TuiActionAdapter;

impl TuiActionPort for TuiActionAdapter {
    fn selection_observation(
        &mut self,
    ) -> Result<crate::surfaces::tui::runtime_bridge::SelectionObservation, AppError> {
        let identity = crate::app::workflow_adapter::ledger::validated_current_identity()?;
        let current = crate::app::workflow_adapter::state::current_state_lease_view()?;
        let active_workflow = crate::app::workflow_adapter::state::active_workflow_id()?
            .map(|workflow_id| crate::app::workflow_adapter::state::load_workflow(&workflow_id))
            .transpose()?
            .map(
                |workflow| crate::surfaces::tui::runtime_bridge::ObservedWorkflow {
                    workflow_id: workflow.workflow_id,
                    revision: workflow.revision,
                    hash: workflow.artifact_hash,
                },
            );
        Ok(crate::surfaces::tui::runtime_bridge::SelectionObservation {
            project_id: identity.project_id,
            session_id: identity.session_id,
            current_revision: current.revision,
            current_hash: current.artifact_hash,
            active_workflow,
        })
    }

    fn workflow(
        &mut self,
        workflow_id: &str,
    ) -> Result<crate::runtime_core::workflow::storage_compat::record::WorkflowRecord, AppError>
    {
        crate::app::workflow_adapter::state::load_workflow(workflow_id)
    }

    fn approve_patch(
        &mut self,
        proposal_id: &str,
        token: &str,
        intent_id: &str,
        lease: &SelectionLease,
    ) -> Result<Option<crate::surfaces::tui::runtime_bridge::OneShotSecret>, TuiMutationFailure>
    {
        crate::app::patch_adapter::approve_for_tui(proposal_id, token, intent_id, lease)
            .map_err(classify_tui_mutation_failure)
    }

    fn approve_verification(
        &mut self,
        proposal_id: &str,
        token: &str,
        intent_id: &str,
        lease: &SelectionLease,
    ) -> Result<(), TuiMutationFailure> {
        crate::app::patch_adapter::verify_for_tui(proposal_id, token, intent_id, lease)
            .map(|_| ())
            .map_err(classify_tui_mutation_failure)
    }

    fn deny_pending_gate(
        &mut self,
        workflow_id: &str,
        intent_id: &str,
        gate_id: &str,
        gate_kind: TuiGateKind,
        lease: &SelectionLease,
    ) -> Result<TuiOutcome, TuiMutationFailure> {
        crate::app::patch_adapter::deny_pending_gate_for_tui(
            workflow_id,
            intent_id,
            gate_id,
            gate_kind,
            lease,
        )
        .map_err(classify_tui_mutation_failure)
    }

    fn resume_workflow(
        &mut self,
        workflow_id: &str,
        intent_id: &str,
        lease: &SelectionLease,
    ) -> Result<(), TuiMutationFailure> {
        crate::app::patch_adapter::resume_workflow_for_tui(workflow_id, intent_id, lease)
            .map_err(classify_tui_mutation_failure)
    }

    fn cancel_workflow(
        &mut self,
        workflow_id: &str,
        intent_id: &str,
        lease: &SelectionLease,
    ) -> Result<(), TuiMutationFailure> {
        crate::app::patch_adapter::cancel_workflow_for_tui(workflow_id, intent_id, lease)
            .map_err(classify_tui_mutation_failure)
    }

    fn resume_session(
        &mut self,
        session_id: &str,
        intent_id: &str,
        lease: &SelectionLease,
    ) -> Result<Option<String>, AppError> {
        crate::app::workflow_adapter::state::session_resume_report_for_tui(
            session_id, intent_id, lease,
        )
    }
}

fn classify_tui_mutation_failure(error: AppError) -> TuiMutationFailure {
    if crate::app::patch_adapter::is_stale_selection_error(&error) {
        return TuiMutationFailure::StaleSelection;
    }
    match error.message.as_str() {
        "internal.resume-inconclusive-effect" => TuiMutationFailure::ResumeInconclusiveEffect,
        "internal.resume-corrupt-state" => TuiMutationFailure::ResumeCorruptState,
        "internal.cancel-no-active-workflow" => TuiMutationFailure::CancelNoActiveWorkflow,
        message if message.starts_with("internal.cancel-terminal:") => {
            TuiMutationFailure::CancelTerminal(
                message
                    .trim_start_matches("internal.cancel-terminal:")
                    .to_string(),
            )
        }
        message if message.starts_with("internal.rollback-conflict:") => {
            TuiMutationFailure::RollbackConflict
        }
        _ => TuiMutationFailure::Other(error),
    }
}

impl TuiReadPort for TuiReadAdapter {
    fn state_snapshot(
        &mut self,
        max_ledger_events: usize,
    ) -> Result<crate::runtime_core::workflow::domain::snapshot::TuiStateSnapshot, AppError> {
        crate::app::workflow_adapter::state::tui_state_snapshot_read_only(max_ledger_events)
    }

    fn store_status(
        &mut self,
    ) -> Result<crate::runtime_core::observability::facade::StoreStatus, AppError> {
        crate::app::observability_adapter::status_read_only()
    }

    fn monitor_snapshot(
        &mut self,
        limit: usize,
    ) -> Result<crate::runtime_core::observability::facade::MonitorProjectionSnapshot, AppError>
    {
        crate::app::observability_adapter::monitor_snapshot_read_only(limit)
    }

    fn transcript_record(
        &mut self,
        event: &crate::runtime_core::workflow::storage_compat::ledger::ParsedLedgerEvent,
    ) -> Result<crate::runtime_core::workflow::storage_compat::transcript::TranscriptRecord, AppError>
    {
        transcript::record_from_event(event)
    }

    fn tool_output_view(
        &mut self,
        record: &crate::runtime_core::workflow::storage_compat::transcript::TranscriptRecord,
        artifact_id: &str,
    ) -> Result<crate::runtime_core::workflow::domain::transcript::ToolOutputView, AppError> {
        transcript::tool_output_view_from_canonical_record(record, artifact_id)
    }

    fn proposal_detail(
        &mut self,
        workflow: &crate::runtime_core::workflow::storage_compat::record::WorkflowRecord,
        proposal_id: &str,
        max_bytes: usize,
    ) -> Result<crate::runtime_core::patch::proposal::PatchProposalDetail, AppError> {
        crate::app::patch_adapter::proposal_detail_for_workflow_bounded(
            workflow,
            proposal_id,
            max_bytes,
        )
    }

    fn evidence_status(
        &mut self,
        max_entries: usize,
        max_bytes: u64,
    ) -> Result<crate::runtime_core::knowledge::evidence::EvidenceStoreStatus, AppError> {
        crate::app::evidence_adapter::store_status_bounded(max_entries, max_bytes)
    }

    fn content_hash(&mut self, value: &str) -> String {
        crate::app::workflow_adapter::state::sha256_text(value)
    }

    fn projection_status(&mut self, project_id: &str) -> ProjectionStatus {
        match transition::projection_lag_status_read_only(project_id) {
            transition::ProjectionLagReadStatus::Clear => ProjectionStatus::Clear,
            transition::ProjectionLagReadStatus::Lagging => ProjectionStatus::Lagging,
            transition::ProjectionLagReadStatus::Unavailable => ProjectionStatus::Unavailable,
        }
    }
}

pub(crate) fn canonical_read_page(request: TuiReadRequest) -> Result<TuiReadPage, AppError> {
    tui_read::read_tui_page(&mut TuiReadAdapter, request)
}

pub(crate) fn canonical_selection_lease(
    selected_object_id: &str,
) -> Result<SelectionLease, AppError> {
    tui_action::selection_lease(&mut TuiActionAdapter, selected_object_id)
}

pub(crate) fn canonical_gate_descriptor(
    workflow_id: &str,
) -> Result<(String, TuiGateKind), AppError> {
    tui_action::gate_descriptor(&mut TuiActionAdapter, workflow_id)
}

pub(crate) fn canonical_dispatch_intent(intent: TuiIntent) -> Result<TuiOutcome, AppError> {
    tui_action::dispatch_intent(&mut TuiActionAdapter, intent)
}

impl TuiRuntimePort for TuiRuntimeAdapter {
    fn read_tui_page(&mut self, request: TuiReadRequest) -> Result<TuiReadPage, AppError> {
        canonical_read_page(request)
    }

    fn new_tui_intent_id(&mut self) -> String {
        new_tui_intent_id()
    }

    fn tui_selection_lease(
        &mut self,
        selected_object_id: &str,
    ) -> Result<SelectionLease, AppError> {
        canonical_selection_lease(selected_object_id)
    }

    fn tui_gate_descriptor(
        &mut self,
        workflow_id: &str,
    ) -> Result<(String, TuiGateKind), AppError> {
        canonical_gate_descriptor(workflow_id)
    }

    fn dispatch_tui_intent(&mut self, intent: TuiIntent) -> Result<TuiOutcome, AppError> {
        canonical_dispatch_intent(intent)
    }
}

mod report_composition;
pub use report_composition::{
    approvals_report, diff_report, evidence_report, monitor_report, overview_report,
    sessions_report, transcript_report,
};

#[cfg(test)]
#[path = "tui_adapter/tests.rs"]
mod tests;
