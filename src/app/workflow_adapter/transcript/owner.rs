//! Transcript stream identity and session-scoped append entrypoint.

use crate::app::context_adapter::SourcePointer;
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::runtime_core::workflow::storage_compat::transcript::TranscriptRecord;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TranscriptOwner {
    pub(crate) project_id: String,
    pub(crate) session_id: String,
    pub(crate) stream_id: String,
}

impl TranscriptOwner {
    pub(super) fn for_workflow(workflow: &state::WorkflowRecord) -> Self {
        Self {
            project_id: workflow.project_id.clone(),
            session_id: workflow.session_id.clone(),
            stream_id: workflow.workflow_id.clone(),
        }
    }
}

pub(crate) fn record_session_turn(
    owner: &TranscriptOwner,
    kind: &str,
    causal_id: &str,
    content: &str,
    source_pointers: &[SourcePointer],
) -> Result<TranscriptRecord, AppError> {
    super::record_turn(
        owner,
        None,
        kind,
        causal_id,
        content,
        source_pointers,
        None,
        None,
    )
}
