//! Installation and cleanup orchestration for validated recovery transactions.

use crate::foundation::error::AppError;

use super::contracts::WorkflowRecoveryPort;
use super::validation::{validate_pending_transaction, RecoveryDisposition};

pub(super) fn recover_workflow_transaction(
    port: &impl WorkflowRecoveryPort,
    workflow_id: &str,
) -> Result<(), AppError> {
    let Some(transaction) = port.load_transaction(workflow_id)? else {
        return Ok(());
    };

    match validate_pending_transaction(port, workflow_id, &transaction)? {
        RecoveryDisposition::AlreadyCommitted => port.remove_transaction(workflow_id),
        RecoveryDisposition::Prepared => {
            port.install_snapshot(&transaction.record, transaction.body.as_bytes())?;
            port.install_pointer(&transaction.record, transaction.schema_version)?;
            port.remove_transaction(workflow_id)
        }
    }
}
