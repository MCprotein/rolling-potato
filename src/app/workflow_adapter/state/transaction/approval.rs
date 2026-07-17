use super::super::*;

pub(crate) struct PreparedApprovalTransition<'a> {
    pub transition_guard: Option<&'a transition::TransitionGuard>,
    pub workflow_guard: &'a WorkflowCheckpointGuard,
    pub writer: &'a ledger::LedgerWriterGuard,
    pub planned: &'a [ledger::PlannedEvent],
    pub bundle: &'a transition::PreparedSourceBundle,
    pub r1: &'a PreparedWorkflowRevision,
    pub r2: &'a PreparedWorkflowRevision,
    pub transcript: &'a transcript::PreparedTranscriptTurn,
    pub current: &'a PreparedCurrentImage,
    pub events: &'a [ledger::LedgerEvent],
}

pub(crate) fn transition_project_current_state_prepared_approval(
    prepared: PreparedApprovalTransition<'_>,
) -> Result<(), AppError> {
    let transition_guard = prepared
        .transition_guard
        .ok_or_else(|| AppError::blocked("prepared approval transition guard 누락"))?;
    let journal = transition_guard.commit(prepared.bundle)?;
    execute_prepared_approval(prepared, &journal, TransactionExecution::Commit)
}

pub(crate) fn recover_project_current_state_prepared_approval(
    prepared: PreparedApprovalTransition<'_>,
    journal: &std::path::Path,
) -> Result<(), AppError> {
    let lag_path = transition::projection_lag_path(prepared.bundle)?;
    let mut port = ApprovalProjectionRecoveryPort {
        prepared: Some(prepared),
        journal,
        lag_path,
    };
    projection_barrier::recover_through_projection_barrier(&mut port)
}

struct ApprovalProjectionRecoveryPort<'a> {
    prepared: Option<PreparedApprovalTransition<'a>>,
    journal: &'a std::path::Path,
    lag_path: PathBuf,
}

impl ApprovalProjectionRecoveryPort<'_> {
    fn prepared(&self) -> &PreparedApprovalTransition<'_> {
        self.prepared
            .as_ref()
            .expect("approval recovery port retains prepared transition")
    }
}

impl ProjectionBarrierRecoveryPort for ApprovalProjectionRecoveryPort<'_> {
    fn lag_exists(&self) -> bool {
        self.lag_path.exists()
    }

    fn lag_temp_exists(&self) -> bool {
        self.lag_path.with_extension("json.tmp").exists()
    }

    fn target_is_converged(&self) -> Result<bool, AppError> {
        let prepared = self.prepared();
        prepared
            .writer
            .prepared_target_is_converged(prepared.bundle, self.journal)
    }

    fn install_lag(&self) -> Result<PathBuf, AppError> {
        let prepared = self.prepared();
        transition::install_projection_lag(prepared.bundle).map_err(|error| {
            AppError::blocked(format!(
                "projection lag install 실패\n- code: projection.lag-install-failed\n- intent: {}\n- error: {}",
                prepared.bundle.intent_id, error.message,
            ))
        })
    }

    fn repair_required(&self, lag: &std::path::Path) -> AppError {
        AppError::blocked(format!(
            "projection repair 필요\n- code: projection.repair-required\n- intent: {}\n- lag: {}\n- error: interrupted repair requires a durable lag marker",
            self.prepared().bundle.intent_id,
            lag.display()
        ))
    }

    fn resume_recovery(&mut self) -> Result<(), AppError> {
        let prepared = self
            .prepared
            .take()
            .expect("approval recovery executes at most once");
        execute_prepared_approval(prepared, self.journal, TransactionExecution::Recovery)
    }
}

fn execute_prepared_approval(
    prepared: PreparedApprovalTransition<'_>,
    journal: &std::path::Path,
    execution: TransactionExecution,
) -> Result<(), AppError> {
    let PreparedApprovalTransition {
        transition_guard,
        workflow_guard,
        writer,
        planned,
        bundle,
        r1,
        r2,
        transcript,
        current,
        events,
    } = prepared;
    let mut port = StateApprovalTransactionPort {
        transition_guard,
        workflow_guard,
        bundle,
        r1,
        r2,
        transcript,
        current,
        events,
        journal,
        sink: writer.event_sink(planned),
    };
    transaction_coordinator::execute_approval_transaction(&mut port, execution)
}

struct StateApprovalTransactionPort<'a> {
    transition_guard: Option<&'a transition::TransitionGuard>,
    workflow_guard: &'a WorkflowCheckpointGuard,
    bundle: &'a transition::PreparedSourceBundle,
    r1: &'a PreparedWorkflowRevision,
    r2: &'a PreparedWorkflowRevision,
    transcript: &'a transcript::PreparedTranscriptTurn,
    current: &'a PreparedCurrentImage,
    events: &'a [ledger::LedgerEvent],
    journal: &'a std::path::Path,
    sink: ledger::EventSink<'a>,
}

impl ApprovalTransactionPort for StateApprovalTransactionPort<'_> {
    fn fault(&mut self, point: ApprovalFault) -> Result<(), AppError> {
        crate::patch::approval_transaction_fault(point.as_str())
    }

    fn append_event(&mut self, index: usize) -> Result<(), AppError> {
        let event = self
            .events
            .get(index)
            .ok_or_else(|| AppError::blocked("prepared approval event index 범위 초과"))?;
        self.sink
            .append_planned_under_guard(index, event)
            .map(|_| ())
    }

    fn install_snapshot(&mut self, revision: ApprovalRevision) -> Result<(), AppError> {
        let prepared = match revision {
            ApprovalRevision::First => self.r1,
            ApprovalRevision::Second => self.r2,
        };
        self.workflow_guard.install_snapshot(prepared)
    }

    fn install_pointer(&mut self, revision: ApprovalRevision) -> Result<(), AppError> {
        let prepared = match revision {
            ApprovalRevision::First => self.r1,
            ApprovalRevision::Second => self.r2,
        };
        self.workflow_guard.install_pointer(prepared)
    }

    fn install_source(&mut self) -> Result<(), AppError> {
        install_prepared_source_bundle(self.bundle, self.journal)
    }

    fn install_transcript(&mut self) -> Result<(), AppError> {
        transcript::install_prepared_no_stream_tool_turn(self.transcript)
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
        crate::patch::approval_projection_fault()
            .and_then(|_| self.sink.converge_prepared(self.bundle, self.journal))
    }

    fn projection_repair_required(&mut self, convergence_error: AppError) -> AppError {
        match transition::install_projection_lag(self.bundle) {
            Ok(lag) => AppError::blocked(format!(
                "projection repair 필요\n- code: projection.repair-required\n- intent: {}\n- lag: {}\n- error: {}",
                self.bundle.intent_id,
                lag.display(),
                convergence_error.message,
            )),
            Err(lag_error) => AppError::blocked(format!(
                "projection lag install 실패\n- code: projection.lag-install-failed\n- intent: {}\n- converge error: {}\n- lag error: {}",
                self.bundle.intent_id, convergence_error.message, lag_error.message,
            )),
        }
    }

    fn remove_projection_lag(&mut self) -> Result<(), AppError> {
        transition::remove_projection_lag(self.bundle)
    }

    fn validate_cleanup_authority(&mut self) -> Result<(), AppError> {
        transition::validate_committed_bundle_cleanup_authority(self.bundle, self.journal)
    }

    fn remove_journal(&mut self) -> Result<(), AppError> {
        self.transition_guard
            .ok_or_else(|| AppError::blocked("prepared approval cleanup guard 누락"))?
            .remove(self.bundle, self.journal)
    }
}
