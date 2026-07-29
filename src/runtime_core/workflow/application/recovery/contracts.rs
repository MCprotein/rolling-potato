//! Persistence contracts shared by workflow recovery owners.

use crate::foundation::error::AppError;
use crate::runtime_core::workflow::storage_compat::ledger::{RuntimeIdentity, WorkflowCheckpoint};
use crate::runtime_core::workflow::storage_compat::record::{WorkflowPointer, WorkflowRecord};

pub(crate) struct PendingWorkflowTransaction {
    pub schema_version: u64,
    pub record: WorkflowRecord,
    pub body: String,
}

#[derive(Clone, Copy)]
pub(crate) enum RecoveryArtifact {
    Transaction,
    Pointer,
}

pub(crate) trait WorkflowRecoveryPort {
    fn load_transaction(
        &self,
        workflow_id: &str,
    ) -> Result<Option<PendingWorkflowTransaction>, AppError>;

    fn load_pointer(&self, workflow_id: &str) -> Result<Option<WorkflowPointer>, AppError>;

    fn checkpoints(&self, workflow_id: &str) -> Result<Vec<WorkflowCheckpoint>, AppError>;

    fn validate_chain(
        &self,
        workflow_id: &str,
        committed_revision: u64,
        expected_latest_schema: u64,
    ) -> Result<WorkflowRecord, AppError>;

    fn validate_chain_with_checkpoints(
        &self,
        workflow_id: &str,
        committed_revision: u64,
        expected_latest_schema: u64,
        checkpoints: &[WorkflowCheckpoint],
    ) -> Result<WorkflowRecord, AppError>;

    fn current_identity(&self) -> Result<RuntimeIdentity, AppError>;

    fn checkpoint_exists(
        &self,
        workflow_id: &str,
        revision: u64,
        artifact_hash: &str,
    ) -> Result<bool, AppError>;

    fn install_snapshot(&self, record: &WorkflowRecord, body: &[u8]) -> Result<(), AppError>;

    fn install_pointer(&self, record: &WorkflowRecord, schema_version: u64)
        -> Result<(), AppError>;

    fn remove_transaction(&self, workflow_id: &str) -> Result<(), AppError>;

    fn corrupt(&self, workflow_id: &str, artifact: RecoveryArtifact) -> AppError;
}

pub(crate) trait PreparedStateRecoveryPort {
    fn install_reconcile_backup(&mut self) -> Result<(), AppError>;

    fn install_workflow_snapshot(&mut self) -> Result<(), AppError>;

    fn append_event(&mut self) -> Result<(), AppError>;

    fn install_workflow_pointer(&mut self) -> Result<(), AppError>;

    fn finish_events(&mut self) -> Result<(), AppError>;

    fn validate_ledger_binding(&mut self) -> Result<(), AppError>;

    fn install_current_state(&mut self) -> Result<(), AppError>;

    fn converge_projections(&mut self) -> Result<(), AppError>;
}
