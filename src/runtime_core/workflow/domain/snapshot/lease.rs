//! Current-state identity and lease validation.

use crate::foundation::error::AppError;
use crate::runtime_core::workflow::storage_compat::ledger::{LedgerBinding, RuntimeIdentity};
use crate::runtime_core::workflow::storage_compat::record::WorkflowRecord;

use super::types::{CurrentStateLeaseView, CurrentStateSnapshot};

pub(crate) fn validate_snapshot_identity(
    snapshot: &CurrentStateSnapshot,
    identity: &RuntimeIdentity,
) -> Result<(), AppError> {
    if snapshot.project_id == identity.project_id && snapshot.session_id == identity.session_id {
        Ok(())
    } else {
        Err(AppError::blocked(
            "selection current-state identity binding 불일치",
        ))
    }
}

pub(crate) fn validate_current_lease(
    snapshot: &CurrentStateSnapshot,
    current_ledger: &LedgerBinding,
    active_workflow: Option<&WorkflowRecord>,
) -> Result<CurrentStateLeaseView, AppError> {
    if &snapshot.ledger_binding != current_ledger {
        return Err(AppError::blocked(
            "current-state lease 차단\n- code: selection.stale-ledger-binding\n- 동작: ledger와 current-state가 수렴하기 전 선택 권한을 만들지 않았습니다.",
        ));
    }
    match (snapshot.active_workflow.as_ref(), active_workflow) {
        (Some(binding), Some(workflow))
            if binding.workflow_id == workflow.workflow_id
                && binding.revision == workflow.revision
                && binding.artifact_hash == workflow.artifact_hash => {}
        (Some(_), _) | (None, Some(_)) => {
            return Err(AppError::blocked(
                "current-state lease 차단\n- code: selection.stale-workflow-binding\n- 동작: workflow pointer와 current-state가 일치하지 않습니다.",
            ));
        }
        (None, None) => {}
    }
    Ok(CurrentStateLeaseView {
        revision: snapshot.revision,
        artifact_hash: snapshot.artifact_hash.clone(),
    })
}
