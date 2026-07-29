//! State-transition and reconcile transaction ordering.

use crate::foundation::error::AppError;

use super::contracts::{
    ReconcileTransactionPort, StateTransitionFault, StateTransitionTransactionPort,
};

pub(crate) fn execute_state_transition(
    port: &mut impl StateTransitionTransactionPort,
    checkpoint: bool,
) -> Result<(), AppError> {
    port.fault(StateTransitionFault::Journal)?;
    if checkpoint {
        port.fault(StateTransitionFault::CheckpointTransaction)?;
    }
    port.install_snapshot()?;
    if checkpoint {
        port.fault(StateTransitionFault::CheckpointSnapshot)?;
    }
    port.fault(StateTransitionFault::Artifacts)?;
    port.append_event()?;
    port.fault(StateTransitionFault::Ledger)?;
    if checkpoint {
        port.fault(StateTransitionFault::CheckpointLedger)?;
    }
    port.install_pointer()?;
    if checkpoint {
        port.fault(StateTransitionFault::CheckpointPointer)?;
    }
    port.finish_events()?;
    port.install_current()?;
    port.fault(StateTransitionFault::Current)?;
    port.converge()?;
    port.fault(StateTransitionFault::Projection)?;
    port.remove_journal()
}

pub(crate) fn execute_reconcile_transaction(
    port: &mut impl ReconcileTransactionPort,
) -> Result<(), AppError> {
    port.fault(StateTransitionFault::Journal)?;
    port.install_backup()?;
    port.fault(StateTransitionFault::Artifacts)?;
    port.append_event()?;
    port.fault(StateTransitionFault::Ledger)?;
    port.finish_events()?;
    port.install_current()?;
    port.fault(StateTransitionFault::Current)?;
    port.converge()?;
    port.fault(StateTransitionFault::Projection)?;
    port.remove_journal()
}
