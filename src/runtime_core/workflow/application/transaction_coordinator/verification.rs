//! Verification transaction ordering.

use crate::foundation::error::AppError;

use super::contracts::{TransactionExecution, VerificationFault, VerificationTransactionPort};

pub(crate) fn execute_verification_transaction(
    port: &mut impl VerificationTransactionPort,
    execution: TransactionExecution,
) -> Result<(), AppError> {
    let commit = execution == TransactionExecution::Commit;
    if commit {
        port.fault(VerificationFault::V1)?;
    }
    port.append_event(0)?;
    if commit {
        port.fault(VerificationFault::V2)?;
    }
    port.install_snapshot()?;
    port.append_event(1)?;
    if commit {
        port.fault(VerificationFault::V3BeforePointer)?;
    }
    port.install_pointer()?;
    if commit {
        port.fault(VerificationFault::V3)?;
    }
    port.append_event(2)?;
    if commit {
        port.fault(VerificationFault::V4)?;
    }
    port.install_current()?;
    if commit {
        port.fault(VerificationFault::V5)?;
    }
    port.finish_events()?;
    port.converge()?;
    if commit {
        port.fault(VerificationFault::V6)?;
        port.remove_journal()?;
    }
    Ok(())
}
