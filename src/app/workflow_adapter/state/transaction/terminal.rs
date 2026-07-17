use super::super::*;

pub(crate) struct TerminalActionRequest<'a> {
    pub intent_id: &'a str,
    pub intent_kind: &'a str,
    pub identity: &'a RuntimeIdentity,
    pub before: &'a WorkflowRecord,
    pub terminal: WorkflowRecord,
    pub audit_event_type: &'a str,
    pub audit_summary: &'a str,
    pub audit_details: &'a str,
    pub source: Option<PreparedTerminalSource>,
}

pub(crate) fn transition_project_current_state_prepared_terminal_action(
    transition_guard: &transition::TransitionGuard,
    workflow_guard: &WorkflowCheckpointGuard,
    request: TerminalActionRequest<'_>,
) -> Result<WorkflowRecord, AppError> {
    let TerminalActionRequest {
        intent_id,
        intent_kind,
        identity,
        before,
        terminal,
        audit_event_type,
        audit_summary,
        audit_details,
        source,
    } = request;
    let current_lease = current_state_lease_view_under_transition()?;
    let revision = workflow_guard.prepare_revision(before, terminal)?;
    let e0 = ledger::new_event_for(
        identity,
        "runtime.intent.accepted",
        "interactive runtime intent accepted",
        &format!(
            "intent_id={intent_id} intent_kind={intent_kind} workflow_id={}",
            before.workflow_id
        ),
    );
    let e2 = ledger::new_event_for(
        identity,
        audit_event_type,
        audit_summary,
        &format!(
            "intent_id={intent_id} workflow_id={} revision={} artifact_hash={} {audit_details}",
            revision.record.workflow_id, revision.record.revision, revision.record.artifact_hash
        ),
    );
    let events = vec![e0, revision.event.clone(), e2];
    let writer = ledger::LedgerWriterGuard::acquire()?;
    let ledger_binding = writer.binding()?;
    let planned = writer.plan_events(&events)?;
    let final_binding = ledger::LedgerBinding {
        event_count: planned[2].ordinal,
        event_id: Some(planned[2].event.event_id.clone()),
        event_hash: planned[2].event_hash.clone(),
    };
    let current =
        prepare_terminal_current_image_after(&revision.record, before.revision, &final_binding)?;
    let source_context = source.as_ref().map(|source| {
        (
            source.plan.clone(),
            source.before.as_slice(),
            source.proposed.as_slice(),
        )
    });
    let mut bundle = transition::prepare_terminal_action_bundle_with_context(
        intent_id,
        intent_kind,
        &before.workflow_id,
        source_context,
        transition::PreparedBundleContext {
            identity,
            lease: &current_lease,
            ledger_binding,
        },
    )?;
    transition::bind_planned_events(&mut bundle, &planned)?;
    transition::bind_additional_members(
        &mut bundle,
        vec![
            prepared_workflow_member(
                transition::PreparedMemberKind::WorkflowSnapshot,
                revision.snapshot_stored_path.clone(),
                revision.snapshot_member_id.clone(),
                None,
                events[1].event_id.clone(),
                revision.snapshot_bytes.clone(),
                "absent",
            ),
            prepared_workflow_member(
                transition::PreparedMemberKind::WorkflowPointer,
                revision.pointer_stored_path.clone(),
                revision.pointer_member_id.clone(),
                Some(revision.snapshot_member_id.clone()),
                events[1].event_id.clone(),
                revision.pointer_bytes.clone(),
                "file",
            ),
            state_transition_current_member(
                &current,
                &events[2].event_id,
                Some(revision.snapshot_member_id.clone()),
                "file",
            ),
        ],
    )?;
    let journal = transition_guard.commit(&bundle)?;
    let mut port = StateTerminalActionTransactionPort {
        transition_guard: Some(transition_guard),
        workflow_guard,
        bundle: &bundle,
        revision: &revision,
        current: &current,
        events: &events,
        journal: &journal,
        sink: writer.event_sink(&planned),
    };
    transaction_coordinator::execute_terminal_action_transaction(
        &mut port,
        TransactionExecution::Commit,
    )?;
    Ok(revision.record)
}

struct StateTerminalActionTransactionPort<'a> {
    transition_guard: Option<&'a transition::TransitionGuard>,
    workflow_guard: &'a WorkflowCheckpointGuard,
    bundle: &'a transition::PreparedSourceBundle,
    revision: &'a PreparedWorkflowRevision,
    current: &'a PreparedCurrentImage,
    events: &'a [ledger::LedgerEvent],
    journal: &'a std::path::Path,
    sink: ledger::EventSink<'a>,
}

impl TerminalActionTransactionPort for StateTerminalActionTransactionPort<'_> {
    fn fault(&mut self, point: TerminalActionFault) -> Result<(), AppError> {
        terminal_action_fault(point.as_str())
    }

    fn append_event(&mut self, index: usize) -> Result<(), AppError> {
        let event = self
            .events
            .get(index)
            .ok_or_else(|| AppError::blocked("prepared terminal event index 범위 초과"))?;
        self.sink
            .append_planned_under_guard(index, event)
            .map(|_| ())
    }

    fn install_source(&mut self) -> Result<(), AppError> {
        if self.bundle.source_install.is_some() {
            install_prepared_source_bundle(self.bundle, self.journal)?;
        }
        Ok(())
    }

    fn install_snapshot(&mut self) -> Result<(), AppError> {
        self.workflow_guard.install_snapshot(self.revision)
    }

    fn install_pointer(&mut self) -> Result<(), AppError> {
        self.workflow_guard.install_pointer(self.revision)
    }

    fn finish_events(&mut self) -> Result<(), AppError> {
        self.sink.finish()
    }

    fn install_current(&mut self) -> Result<(), AppError> {
        install_current_image(
            self.current,
            self.bundle.current_revision,
            &self.bundle.current_artifact_hash,
        )
    }

    fn converge(&mut self) -> Result<(), AppError> {
        self.sink.converge_prepared(self.bundle, self.journal)
    }

    fn remove_journal(&mut self) -> Result<(), AppError> {
        self.transition_guard
            .ok_or_else(|| AppError::blocked("prepared terminal cleanup guard 누락"))?
            .remove(self.bundle, self.journal)
    }
}

pub(crate) fn recover_project_current_state_prepared_terminal_action(
    bundle: &transition::PreparedSourceBundle,
    journal: &std::path::Path,
) -> Result<(), AppError> {
    let planned = transition::planned_events(bundle)?;
    if planned.len() != 3 || bundle.additional_members.len() != 3 {
        return Err(AppError::blocked(
            "prepared terminal recovery exact shape 불일치",
        ));
    }
    let workflow_id = bundle
        .workflow_id
        .as_deref()
        .ok_or_else(|| AppError::blocked("prepared terminal workflow id 누락"))?;
    let revision = decode_prepared_workflow_revision(
        workflow_id,
        &bundle.additional_members[0],
        &bundle.additional_members[1],
        &bundle.semantic_events[1],
    )?;
    let final_binding = ledger::LedgerBinding {
        event_count: planned[2].ordinal,
        event_id: Some(planned[2].event.event_id.clone()),
        event_hash: planned[2].event_hash.clone(),
    };
    let current = decode_prepared_terminal_current_image(
        &bundle.additional_members[2],
        &revision.record,
        &final_binding,
        &revision.snapshot_member_id,
        &bundle.semantic_events[2].event_id,
    )?;
    validate_current_state_recovery_cas(
        bundle.current_revision,
        &bundle.current_artifact_hash,
        Some(&current.bytes),
    )?;
    if bundle.source_install.is_some() {
        validate_prepared_source_parent(bundle)?;
    }
    let workflow_guard = WorkflowCheckpointGuard::acquire(workflow_id)?;
    let predecessor_revision = revision
        .record
        .revision
        .checked_sub(1)
        .ok_or_else(|| AppError::blocked("prepared terminal predecessor underflow"))?;
    let installed = workflow_guard.load_recovery_current(&[
        (predecessor_revision, revision.record.previous_hash.as_str()),
        (
            revision.record.revision,
            revision.record.artifact_hash.as_str(),
        ),
    ])?;
    let predecessor = installed.revision == predecessor_revision
        && installed.artifact_hash == revision.record.previous_hash;
    if installed != revision.record && !predecessor {
        return Err(AppError::blocked(
            "prepared terminal workflow predecessor conflict",
        ));
    }
    let writer = ledger::LedgerWriterGuard::acquire()?;
    let mut port = StateTerminalActionTransactionPort {
        transition_guard: None,
        workflow_guard: &workflow_guard,
        bundle,
        revision: &revision,
        current: &current,
        events: &bundle.semantic_events,
        journal,
        sink: writer.event_sink(&planned),
    };
    transaction_coordinator::execute_terminal_action_transaction(
        &mut port,
        TransactionExecution::Recovery,
    )
}

fn terminal_action_fault(point: &str) -> Result<(), AppError> {
    if cfg!(debug_assertions)
        && std::env::var("RPOTATO_TEST_TERMINAL_ACTION_FAULT").as_deref() == Ok(point)
    {
        return Err(AppError::runtime(format!(
            "injected terminal action fault: {point}"
        )));
    }
    Ok(())
}
