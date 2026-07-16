//! Validated workflow, current-state, session, and read-only runtime views.

use crate::runtime_core::workflow::storage_compat::ledger::{
    LedgerBinding, ParsedLedgerEvent, RuntimeIdentity,
};
use crate::runtime_core::workflow::storage_compat::record::WorkflowRecord;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CurrentWorkflowBinding {
    pub(crate) workflow_id: String,
    pub(crate) revision: u64,
    pub(crate) artifact_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CurrentStateSnapshot {
    pub(crate) schema_version: u64,
    pub(crate) revision: u64,
    pub(crate) previous_artifact_hash: String,
    pub(crate) project_id: String,
    pub(crate) project_root: String,
    pub(crate) session_id: String,
    pub(crate) active_workflow: Option<CurrentWorkflowBinding>,
    pub(crate) parent_session_id: Option<String>,
    pub(crate) branch_from_event_id: Option<String>,
    pub(crate) compaction_boundary: Option<String>,
    pub(crate) resume_source: Option<String>,
    pub(crate) ledger_binding: LedgerBinding,
    pub(crate) artifact_hash: String,
    pub(crate) legacy_canonical_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CurrentStateLeaseView {
    pub revision: u64,
    pub artifact_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TuiStateSnapshot {
    pub identity: RuntimeIdentity,
    pub current_revision: u64,
    pub current_hash: String,
    pub ledger_binding: LedgerBinding,
    pub ledger_events: Vec<ParsedLedgerEvent>,
    pub active_workflow: Option<WorkflowRecord>,
    pub ledger_tail_truncated: bool,
    pub current_ledger_binding_stale: bool,
}
