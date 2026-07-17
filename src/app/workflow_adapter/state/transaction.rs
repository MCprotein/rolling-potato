use super::*;

mod approval;
mod terminal;

pub(crate) use approval::{
    recover_project_current_state_prepared_approval,
    transition_project_current_state_prepared_approval, PreparedApprovalTransition,
};
pub(crate) use terminal::{
    recover_project_current_state_prepared_terminal_action,
    transition_project_current_state_prepared_terminal_action, TerminalActionRequest,
};

pub(crate) struct PreparedVerificationTransition<'a> {
    pub transition_guard: Option<&'a transition::TransitionGuard>,
    pub workflow_guard: &'a WorkflowCheckpointGuard,
    pub writer: &'a ledger::LedgerWriterGuard,
    pub planned: &'a [ledger::PlannedEvent],
    pub bundle: &'a transition::PreparedSourceBundle,
    pub revision: &'a PreparedWorkflowRevision,
    pub current: &'a PreparedCurrentImage,
    pub events: &'a [ledger::LedgerEvent],
}

pub(crate) fn transition_project_current_state_prepared_verification(
    prepared: PreparedVerificationTransition<'_>,
) -> Result<(), AppError> {
    let transition_guard = prepared
        .transition_guard
        .ok_or_else(|| AppError::blocked("prepared verification transition guard 누락"))?;
    let journal = transition_guard.commit(prepared.bundle)?;
    execute_prepared_verification(prepared, &journal, TransactionExecution::Commit)
}

pub(crate) fn recover_project_current_state_prepared_verification(
    prepared: PreparedVerificationTransition<'_>,
    journal: &std::path::Path,
) -> Result<(), AppError> {
    execute_prepared_verification(prepared, journal, TransactionExecution::Recovery)
}

fn execute_prepared_verification(
    prepared: PreparedVerificationTransition<'_>,
    journal: &std::path::Path,
    execution: TransactionExecution,
) -> Result<(), AppError> {
    let PreparedVerificationTransition {
        transition_guard,
        workflow_guard,
        writer,
        planned,
        bundle,
        revision,
        current,
        events,
    } = prepared;
    let mut port = StateVerificationTransactionPort {
        transition_guard,
        workflow_guard,
        bundle,
        revision,
        current,
        events,
        journal,
        sink: writer.event_sink(planned),
    };
    transaction_coordinator::execute_verification_transaction(&mut port, execution)
}

struct StateVerificationTransactionPort<'a> {
    transition_guard: Option<&'a transition::TransitionGuard>,
    workflow_guard: &'a WorkflowCheckpointGuard,
    bundle: &'a transition::PreparedSourceBundle,
    revision: &'a PreparedWorkflowRevision,
    current: &'a PreparedCurrentImage,
    events: &'a [ledger::LedgerEvent],
    journal: &'a std::path::Path,
    sink: ledger::EventSink<'a>,
}

impl VerificationTransactionPort for StateVerificationTransactionPort<'_> {
    fn fault(&mut self, point: VerificationFault) -> Result<(), AppError> {
        crate::patch::verification_approval_transaction_fault(point.as_str())
    }

    fn append_event(&mut self, index: usize) -> Result<(), AppError> {
        let event = self
            .events
            .get(index)
            .ok_or_else(|| AppError::blocked("prepared verification event index 범위 초과"))?;
        self.sink
            .append_planned_under_guard(index, event)
            .map(|_| ())
    }

    fn install_snapshot(&mut self) -> Result<(), AppError> {
        self.workflow_guard.install_snapshot(self.revision)
    }

    fn install_pointer(&mut self) -> Result<(), AppError> {
        self.workflow_guard.install_pointer(self.revision)
    }

    fn install_current(&mut self) -> Result<(), AppError> {
        install_current_image(
            self.current,
            self.bundle.current_revision,
            &self.bundle.current_artifact_hash,
        )
    }

    fn finish_events(&mut self) -> Result<(), AppError> {
        self.sink.finish()
    }

    fn converge(&mut self) -> Result<(), AppError> {
        self.sink.converge_prepared(self.bundle, self.journal)
    }

    fn remove_journal(&mut self) -> Result<(), AppError> {
        self.transition_guard
            .ok_or_else(|| AppError::blocked("prepared verification cleanup guard 누락"))?
            .remove(self.bundle, self.journal)
    }
}
