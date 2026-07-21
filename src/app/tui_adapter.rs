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
    new_tui_intent_id, SelectionLease, TuiBackendStatus, TuiGateKind, TuiIntent, TuiReadBudget,
    TuiReadPage, TuiReadRequest, TuiStatusSnapshot,
};
use crate::surfaces::tui::setup::{self, PreparedTuiModel, TuiSetupPort};

pub fn run_auto() -> Result<(), AppError> {
    if capability::attached() {
        let mut terminal = NativeTerminal::new();
        controller::run_controller(&mut terminal, &mut TuiRuntimeAdapter)
    } else {
        crate::surfaces::cli::render::emit_report(&overview_report()?);
        Ok(())
    }
}

pub fn run_interactive() -> Result<(), AppError> {
    let mut terminal = NativeTerminal::explicit_line_mode();
    controller::run_controller(&mut terminal, &mut TuiRuntimeAdapter)
}

pub fn run_setup() -> Result<(), AppError> {
    let mut terminal = NativeTerminal::new();
    setup::run_setup(&mut terminal, &mut TuiSetupAdapter)
}

pub fn setup_required() -> bool {
    if cfg!(debug_assertions)
        && std::env::var_os("RPOTATO_TEST_SKIP_SETUP").as_deref() == Some(std::ffi::OsStr::new("1"))
    {
        return false;
    }
    crate::app::inference_adapter::model::configured_model_id().is_none()
}

struct TuiRuntimeAdapter;

pub(crate) struct TuiReadAdapter;

struct TuiActionAdapter;

struct TuiSetupAdapter;

impl TuiSetupPort for TuiSetupAdapter {
    fn model_options(&mut self) -> Vec<crate::surfaces::tui::runtime_bridge::TuiModelOption> {
        crate::app::inference_adapter::model::setup_options()
    }

    fn ensure_backend(&mut self) -> Result<String, AppError> {
        crate::app::inference_adapter::backend::ensure_installed_report()
    }

    fn prepare_model(&mut self, id: &str) -> Result<PreparedTuiModel, AppError> {
        let prepared = crate::app::inference_adapter::model::prepare_setup_model(id)?;
        Ok(PreparedTuiModel {
            id: prepared.id,
            artifact_path: prepared.artifact_path.display().to_string(),
        })
    }

    fn start_model(&mut self, model: &PreparedTuiModel) -> Result<String, AppError> {
        let snapshot = crate::app::inference_adapter::backend::runtime_snapshot()?;
        if snapshot.status != "stopped" {
            crate::app::inference_adapter::backend::stop_report()?;
        }
        crate::app::inference_adapter::backend::start_report(
            &model.artifact_path,
            Some(setup::DEFAULT_CONTEXT_TOKENS),
        )
    }
}

fn ensure_runtime_ready() -> Result<(), AppError> {
    let snapshot = crate::app::inference_adapter::backend::runtime_snapshot()?;
    if snapshot.status == "ready" {
        return Ok(());
    }
    if snapshot.status == "stale" {
        crate::app::inference_adapter::backend::stop_report()?;
    }
    let model_path =
        crate::app::inference_adapter::model::default_artifact_path().map_err(|err| {
            if err.message.contains("기본 모델이 선택되지 않았습니다") {
                AppError::blocked(
                    "모델이 선택되지 않았습니다. TUI에서 /model을 입력해 모델을 선택하세요.",
                )
            } else {
                err
            }
        })?;
    crate::app::inference_adapter::backend::ensure_installed_report()?;
    crate::app::inference_adapter::backend::start_report(
        &model_path.display().to_string(),
        Some(setup::DEFAULT_CONTEXT_TOKENS),
    )?;
    Ok(())
}

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
    fn read_tui_status(&mut self) -> Result<TuiStatusSnapshot, AppError> {
        let backend = crate::app::inference_adapter::backend::runtime_snapshot()?;
        let identity = crate::app::workflow_adapter::ledger::validated_current_identity()?;
        let latest = crate::app::observability_adapter::latest_model_run_for_session_read_only(
            &identity.session_id,
        )
        .ok()
        .flatten();
        let model = backend
            .model_id
            .clone()
            .or_else(crate::app::inference_adapter::model::configured_model_id)
            .or_else(|| latest.as_ref().map(|run| run.model_id.clone()))
            .unwrap_or_else(|| "미선택".to_string());
        let latest_matches_model = latest.as_ref().is_some_and(|run| run.model_id == model);
        let context_tokens_used = latest
            .as_ref()
            .filter(|_| latest_matches_model)
            .and_then(|run| run.context_tokens_used);
        let context_limit_tokens = backend.context_limit_tokens.or_else(|| {
            latest
                .as_ref()
                .filter(|_| latest_matches_model)
                .and_then(|run| run.context_limit_tokens)
        });
        let backend = match backend.status {
            "ready" => TuiBackendStatus::Ready,
            "stale" => TuiBackendStatus::Stale,
            "stopped" => TuiBackendStatus::Stopped,
            _ => TuiBackendStatus::Unavailable,
        };
        Ok(TuiStatusSnapshot {
            model,
            context_tokens_used,
            context_limit_tokens,
            has_compaction_checkpoint:
                crate::app::workflow_adapter::state::current_compaction_boundary(
                    &identity.session_id,
                )?
                .is_some(),
            backend,
            session_id: identity.session_id,
        })
    }

    fn compact_context(&mut self) -> Result<String, AppError> {
        Ok(crate::app::context_adapter::compact_manually()?.report())
    }

    fn submit_request(&mut self, request: &str) -> Result<String, AppError> {
        ensure_runtime_ready()?;
        crate::app::runtime_adapter::agent_run_report(request)
    }

    fn model_options(&mut self) -> Vec<crate::surfaces::tui::runtime_bridge::TuiModelOption> {
        crate::app::inference_adapter::model::setup_options()
    }

    fn setup_model(&mut self, id: &str) -> Result<String, AppError> {
        crate::app::inference_adapter::backend::ensure_installed_report()?;
        let prepared = crate::app::inference_adapter::model::prepare_setup_model(id)?;
        let snapshot = crate::app::inference_adapter::backend::runtime_snapshot()?;
        if snapshot.status != "stopped" {
            crate::app::inference_adapter::backend::stop_report()?;
        }
        crate::app::inference_adapter::backend::start_report(
            &prepared.artifact_path.display().to_string(),
            Some(setup::DEFAULT_CONTEXT_TOKENS),
        )?;
        Ok(format!(
            "모델 변경 완료\n- model: {}\n- context: {}\n- backend: ready",
            prepared.id,
            setup::DEFAULT_CONTEXT_TOKENS
        ))
    }

    fn doctor_report(&mut self) -> String {
        crate::app::runtime_adapter::doctor_report()
    }

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
