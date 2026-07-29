//! Facade for workflow transaction ordering and event-sequence validation.

mod approval;
mod contracts;
mod event_sequence;
mod state_transition;
mod terminal_action;
mod verification;

pub(crate) use approval::execute_approval_transaction;
pub(crate) use contracts::{
    ApprovalFault, ApprovalRevision, ApprovalTransactionPort, ReconcileTransactionPort,
    StateTransitionFault, StateTransitionTransactionPort, TerminalActionFault,
    TerminalActionTransactionPort, TransactionExecution, VerificationFault,
    VerificationTransactionPort,
};
pub(crate) use event_sequence::{PlannedEvent, TransactionCoordinator};
pub(crate) use state_transition::{execute_reconcile_transaction, execute_state_transition};
pub(crate) use terminal_action::execute_terminal_action_transaction;
pub(crate) use verification::execute_verification_transaction;

#[cfg(test)]
#[path = "transaction_coordinator/tests.rs"]
mod tests;
