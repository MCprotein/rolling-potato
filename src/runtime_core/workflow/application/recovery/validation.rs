//! Evidence validation for pending workflow transaction recovery.

use crate::foundation::error::AppError;

use super::contracts::{PendingWorkflowTransaction, RecoveryArtifact, WorkflowRecoveryPort};

pub(super) enum RecoveryDisposition {
    AlreadyCommitted,
    Prepared,
}

pub(super) fn validate_pending_transaction(
    port: &impl WorkflowRecoveryPort,
    workflow_id: &str,
    transaction: &PendingWorkflowTransaction,
) -> Result<RecoveryDisposition, AppError> {
    let record = &transaction.record;
    if record.workflow_id != workflow_id {
        return Err(port.corrupt(workflow_id, RecoveryArtifact::Transaction));
    }

    if let Some(pointer) = port.load_pointer(workflow_id)? {
        if pointer.workflow_id != workflow_id {
            return Err(port.corrupt(workflow_id, RecoveryArtifact::Pointer));
        }
        if pointer.committed_revision == record.revision
            && pointer.artifact_hash == record.artifact_hash
        {
            if pointer.schema_version != transaction.schema_version {
                return Err(port.corrupt(workflow_id, RecoveryArtifact::Transaction));
            }
            port.validate_chain(
                workflow_id,
                pointer.committed_revision,
                pointer.schema_version,
            )?;
            return Ok(RecoveryDisposition::AlreadyCommitted);
        }

        let schema_transition_allowed = pointer.schema_version <= transaction.schema_version;
        if pointer.committed_revision.checked_add(1) != Some(record.revision)
            || record.previous_hash != pointer.artifact_hash
            || !schema_transition_allowed
        {
            return Err(port.corrupt(workflow_id, RecoveryArtifact::Transaction));
        }
        let checkpoints = port.checkpoints(workflow_id)?;
        if checkpoints.len() != pointer.committed_revision as usize
            && checkpoints.len() != record.revision as usize
        {
            return Err(port.corrupt(workflow_id, RecoveryArtifact::Transaction));
        }
        let committed = port.validate_chain_with_checkpoints(
            workflow_id,
            pointer.committed_revision,
            pointer.schema_version,
            &checkpoints[..pointer.committed_revision as usize],
        )?;
        if committed.artifact_hash != pointer.artifact_hash
            || committed.project_id != record.project_id
            || committed.session_id != record.session_id
            || committed.action_id != record.action_id
        {
            return Err(port.corrupt(workflow_id, RecoveryArtifact::Transaction));
        }
        if checkpoints.len() == record.revision as usize {
            let pending = &checkpoints[record.revision as usize - 1];
            if pending.revision != record.revision
                || pending.artifact_hash != record.artifact_hash
                || pending.previous_hash != record.previous_hash
            {
                return Err(port.corrupt(workflow_id, RecoveryArtifact::Transaction));
            }
        }
    } else {
        let checkpoints = port.checkpoints(workflow_id)?;
        if record.revision != 1
            || record.previous_hash != "none"
            || checkpoints.len() > 1
            || checkpoints.first().is_some_and(|checkpoint| {
                checkpoint.revision != record.revision
                    || checkpoint.artifact_hash != record.artifact_hash
                    || checkpoint.previous_hash != record.previous_hash
            })
        {
            return Err(port.corrupt(workflow_id, RecoveryArtifact::Transaction));
        }
        if record.project_id != port.current_identity()?.project_id {
            return Err(port.corrupt(workflow_id, RecoveryArtifact::Transaction));
        }
    }

    if !port.checkpoint_exists(workflow_id, record.revision, &record.artifact_hash)? {
        return Err(AppError::blocked(
            "legacy workflow transaction recovery 차단\n- 이유: exact prepared semantic event가 없습니다.\n- 동작: transaction 증거를 보존했습니다.",
        ));
    }

    Ok(RecoveryDisposition::Prepared)
}
