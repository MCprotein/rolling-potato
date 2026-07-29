//! Approval transaction ordering.

use crate::foundation::error::AppError;

use super::contracts::{
    ApprovalFault, ApprovalRevision, ApprovalTransactionPort, TransactionExecution,
};

pub(crate) fn execute_approval_transaction(
    port: &mut impl ApprovalTransactionPort,
    execution: TransactionExecution,
) -> Result<(), AppError> {
    let commit = execution == TransactionExecution::Commit;
    if commit {
        port.fault(ApprovalFault::T1)?;
    }
    port.append_event(0)?;
    if commit {
        port.fault(ApprovalFault::T2)?;
    }
    port.install_snapshot(ApprovalRevision::First)?;
    port.append_event(1)?;
    if commit {
        port.fault(ApprovalFault::T3BeforePointer)?;
    }
    port.install_pointer(ApprovalRevision::First)?;
    if commit {
        port.fault(ApprovalFault::T3)?;
    }
    for index in 2..5 {
        port.append_event(index)?;
    }
    if commit {
        port.fault(ApprovalFault::T4)?;
    }
    port.install_source()?;
    if commit {
        port.fault(ApprovalFault::T5)?;
    }
    for index in 5..8 {
        port.append_event(index)?;
    }
    if commit {
        port.fault(ApprovalFault::T6)?;
    }
    port.install_transcript()?;
    port.append_event(8)?;
    if commit {
        port.fault(ApprovalFault::T7)?;
    }
    port.install_snapshot(ApprovalRevision::Second)?;
    port.append_event(9)?;
    if commit {
        port.fault(ApprovalFault::T8BeforePointer)?;
    }
    port.install_pointer(ApprovalRevision::Second)?;
    if commit {
        port.fault(ApprovalFault::T8)?;
    }
    port.install_current()?;
    if commit {
        port.fault(ApprovalFault::T9)?;
    }
    port.finish_events()?;
    if let Err(error) = port.converge() {
        return Err(port.projection_repair_required(error));
    }
    if commit {
        port.fault(ApprovalFault::T10)?;
    }
    port.remove_projection_lag()?;
    port.validate_cleanup_authority()?;
    if commit {
        port.remove_journal()?;
    }
    Ok(())
}
