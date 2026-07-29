//! Terminal-action transaction ordering.

use crate::foundation::error::AppError;

use super::contracts::{TerminalActionFault, TerminalActionTransactionPort, TransactionExecution};

pub(crate) fn execute_terminal_action_transaction(
    port: &mut impl TerminalActionTransactionPort,
    execution: TransactionExecution,
) -> Result<(), AppError> {
    let commit = execution == TransactionExecution::Commit;
    if commit {
        port.fault(TerminalActionFault::Journal)?;
    }
    port.append_event(0)?;
    if commit {
        port.fault(TerminalActionFault::Intent)?;
    }
    port.install_source()?;
    if commit {
        port.fault(TerminalActionFault::Source)?;
    }
    port.install_snapshot()?;
    port.append_event(1)?;
    if commit {
        port.fault(TerminalActionFault::Snapshot)?;
    }
    port.install_pointer()?;
    if commit {
        port.fault(TerminalActionFault::Pointer)?;
    }
    port.append_event(2)?;
    port.finish_events()?;
    if commit {
        port.fault(TerminalActionFault::Ledger)?;
    }
    port.install_current()?;
    if commit {
        port.fault(TerminalActionFault::Current)?;
    }
    port.converge()?;
    if commit {
        port.fault(TerminalActionFault::Projection)?;
        port.remove_journal()?;
    }
    Ok(())
}
