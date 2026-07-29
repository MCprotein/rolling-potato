//! Ordered recovery of prepared state and its projections.

use crate::foundation::error::AppError;

use super::contracts::PreparedStateRecoveryPort;

pub(super) fn recover_prepared_state_transition(
    port: &mut impl PreparedStateRecoveryPort,
) -> Result<(), AppError> {
    port.install_reconcile_backup()?;
    port.install_workflow_snapshot()?;
    port.append_event()?;
    port.install_workflow_pointer()?;
    port.finish_events()?;
    port.validate_ledger_binding()?;
    port.install_current_state()?;
    port.converge_projections()
}
