use crate::foundation::error::AppError;
use crate::runtime_core::workflow::storage_compat::record::WorkflowRecord;
use crate::surfaces::tui::outcome::TuiOutcome;
use crate::surfaces::tui::runtime_bridge::{
    OneShotSecret, SelectionLease, SelectionObservation, TuiGateKind,
};

pub(crate) enum TuiMutationFailure {
    StaleSelection,
    ResumeInconclusiveEffect,
    ResumeCorruptState,
    CancelNoActiveWorkflow,
    CancelTerminal(String),
    RollbackConflict,
    Other(AppError),
}

pub(crate) trait TuiActionPort {
    fn selection_observation(&mut self) -> Result<SelectionObservation, AppError>;
    fn workflow(&mut self, workflow_id: &str) -> Result<WorkflowRecord, AppError>;
    fn approve_patch(
        &mut self,
        proposal_id: &str,
        token: &str,
        intent_id: &str,
        lease: &SelectionLease,
    ) -> Result<Option<OneShotSecret>, TuiMutationFailure>;
    fn approve_verification(
        &mut self,
        proposal_id: &str,
        token: &str,
        intent_id: &str,
        lease: &SelectionLease,
    ) -> Result<(), TuiMutationFailure>;
    fn deny_pending_gate(
        &mut self,
        workflow_id: &str,
        intent_id: &str,
        gate_id: &str,
        gate_kind: TuiGateKind,
        lease: &SelectionLease,
    ) -> Result<TuiOutcome, TuiMutationFailure>;
    fn resume_workflow(
        &mut self,
        workflow_id: &str,
        intent_id: &str,
        lease: &SelectionLease,
    ) -> Result<(), TuiMutationFailure>;
    fn cancel_workflow(
        &mut self,
        workflow_id: &str,
        intent_id: &str,
        lease: &SelectionLease,
    ) -> Result<(), TuiMutationFailure>;
    fn resume_session(
        &mut self,
        session_id: &str,
        intent_id: &str,
        lease: &SelectionLease,
    ) -> Result<Option<String>, AppError>;
}
