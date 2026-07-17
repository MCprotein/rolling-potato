use super::*;

pub(crate) fn prepare_current_image(
    workflow: &WorkflowRecord,
    ledger_binding: &ledger::LedgerBinding,
) -> Result<PreparedCurrentImage, AppError> {
    let expected_previous_workflow_revision = workflow
        .revision
        .checked_sub(2)
        .ok_or_else(|| AppError::blocked("prepared current image workflow revision underflow"))?;
    prepare_current_image_after(
        workflow,
        expected_previous_workflow_revision,
        ledger_binding,
    )
}

pub(crate) fn prepare_current_image_after(
    workflow: &WorkflowRecord,
    expected_previous_workflow_revision: u64,
    ledger_binding: &ledger::LedgerBinding,
) -> Result<PreparedCurrentImage, AppError> {
    prepare_current_image_after_with_binding(
        workflow,
        expected_previous_workflow_revision,
        ledger_binding,
        true,
    )
}

pub(super) fn prepare_terminal_current_image_after(
    workflow: &WorkflowRecord,
    expected_previous_workflow_revision: u64,
    ledger_binding: &ledger::LedgerBinding,
) -> Result<PreparedCurrentImage, AppError> {
    prepare_current_image_after_with_binding(
        workflow,
        expected_previous_workflow_revision,
        ledger_binding,
        false,
    )
}

fn prepare_current_image_after_with_binding(
    workflow: &WorkflowRecord,
    expected_previous_workflow_revision: u64,
    ledger_binding: &ledger::LedgerBinding,
    keep_active: bool,
) -> Result<PreparedCurrentImage, AppError> {
    let path = paths::current_state_file();
    let body = fs::read_to_string(&path)
        .map_err(|err| AppError::blocked(format!("current image precondition 읽기 실패: {err}")))?;
    let previous = parse_current_state(&body, "prepared current image")?;
    if previous.schema_version != 2
        || previous.project_id != workflow.project_id
        || previous.session_id != workflow.session_id
        || previous.active_workflow.as_ref().is_none_or(|active| {
            active.workflow_id != workflow.workflow_id
                || active.revision != expected_previous_workflow_revision
        })
    {
        return Err(AppError::blocked(
            "prepared current image workflow predecessor binding 불일치",
        ));
    }
    let revision = previous
        .revision
        .checked_add(1)
        .ok_or_else(|| AppError::blocked("current-state revision overflow"))?;
    let mut snapshot = CurrentStateSnapshot {
        schema_version: 2,
        revision,
        previous_artifact_hash: previous.artifact_hash,
        project_id: previous.project_id,
        project_root: previous.project_root,
        session_id: previous.session_id,
        active_workflow: keep_active.then(|| CurrentWorkflowBinding {
            workflow_id: workflow.workflow_id.clone(),
            revision: workflow.revision,
            artifact_hash: workflow.artifact_hash.clone(),
        }),
        parent_session_id: previous.parent_session_id,
        branch_from_event_id: previous.branch_from_event_id,
        compaction_boundary: previous.compaction_boundary,
        resume_source: previous.resume_source,
        ledger_binding: ledger_binding.clone(),
        artifact_hash: String::new(),
        legacy_canonical_hash: None,
    };
    snapshot.artifact_hash = sha256_text(&render_current_state_v2_payload(&snapshot));
    let bytes = render_current_state_v2(&snapshot);
    Ok(PreparedCurrentImage {
        path,
        stored_path: "state/current-state.json".to_string(),
        artifact_id: format!("current-image-{}", snapshot.artifact_hash),
        bytes,
        revision,
    })
}

pub(super) fn prepare_state_transition_current_image(
    identity: &RuntimeIdentity,
    resume_source: Option<&str>,
    active_workflow: Option<&WorkflowRecord>,
    final_binding: &ledger::LedgerBinding,
    previous: Option<&CurrentStateSnapshot>,
) -> Result<PreparedCurrentImage, AppError> {
    let revision = previous.map_or(Ok(1), |snapshot| {
        snapshot
            .revision
            .checked_add(1)
            .ok_or_else(|| AppError::blocked("current-state revision overflow"))
    })?;
    let mut snapshot = CurrentStateSnapshot {
        schema_version: 2,
        revision,
        previous_artifact_hash: previous
            .map(|snapshot| snapshot.artifact_hash.clone())
            .unwrap_or_else(|| "none".to_string()),
        project_id: identity.project_id.clone(),
        project_root: identity.project_root.clone(),
        session_id: identity.session_id.clone(),
        active_workflow: active_workflow.map(|workflow| CurrentWorkflowBinding {
            workflow_id: workflow.workflow_id.clone(),
            revision: workflow.revision,
            artifact_hash: workflow.artifact_hash.clone(),
        }),
        parent_session_id: previous.and_then(|snapshot| snapshot.parent_session_id.clone()),
        branch_from_event_id: previous.and_then(|snapshot| snapshot.branch_from_event_id.clone()),
        compaction_boundary: previous.and_then(|snapshot| snapshot.compaction_boundary.clone()),
        resume_source: resume_source.map(str::to_string),
        ledger_binding: final_binding.clone(),
        artifact_hash: String::new(),
        legacy_canonical_hash: None,
    };
    snapshot.artifact_hash = sha256_text(&render_current_state_v2_payload(&snapshot));
    let bytes = render_current_state_v2(&snapshot);
    Ok(PreparedCurrentImage {
        path: paths::current_state_file(),
        stored_path: "state/current-state.json".to_string(),
        artifact_id: format!("current-image-{}", snapshot.artifact_hash),
        bytes,
        revision,
    })
}

pub(super) fn state_transition_current_member(
    prepared: &PreparedCurrentImage,
    event_id: &str,
    causal_id: Option<String>,
    expected_type: &str,
) -> transition::PreparedMember {
    transition::PreparedMember {
        kind: transition::PreparedMemberKind::CurrentImage,
        path: prepared.stored_path.clone(),
        schema_version: 2,
        binding: transition::PreparedMemberBinding {
            artifact_id: Some(prepared.artifact_id.clone()),
            causal_id,
            source_key: None,
            event_id: Some(event_id.to_string()),
        },
        bytes_utf8: prepared.bytes.clone(),
        expected_type: expected_type.to_string(),
        expected_identity: None,
        readonly: false,
        mode: 0o600,
        ownership: None,
        semantic_role_rank: 0,
    }
}

pub(crate) fn validate_prepared_state_current_member(
    bundle: &transition::PreparedSourceBundle,
    member: &transition::PreparedMember,
) -> Result<(), AppError> {
    let snapshot = parse_current_state(&member.bytes_utf8, "prepared state current member")?;
    let final_chain = bundle
        .event_chain_plan
        .last()
        .ok_or_else(|| AppError::blocked("prepared state current final event 누락"))?;
    let expected_revision = bundle
        .current_revision
        .checked_add(1)
        .ok_or_else(|| AppError::blocked("prepared state current revision overflow"))?;
    let expected_previous = if bundle.current_revision == 0 {
        "none"
    } else {
        bundle.current_artifact_hash.as_str()
    };
    let expected_type = if bundle.current_revision == 0 && bundle.current_artifact_hash == "missing"
    {
        "absent"
    } else {
        "file"
    };
    let final_binding = ledger::LedgerBinding {
        event_count: final_chain.ordinal,
        event_id: Some(final_chain.event_id.clone()),
        event_hash: final_chain.event_hash.clone(),
    };
    if member.kind != transition::PreparedMemberKind::CurrentImage
        || member.path != "state/current-state.json"
        || member.schema_version != 2
        || member.expected_type != expected_type
        || member.binding.event_id.as_deref() != Some(final_chain.event_id.as_str())
        || snapshot.schema_version != 2
        || snapshot.revision != expected_revision
        || snapshot.previous_artifact_hash != expected_previous
        || snapshot.project_id != bundle.project_id
        || snapshot.session_id != bundle.session_id
        || snapshot.ledger_binding != final_binding
        || snapshot.artifact_hash != sha256_text(&render_current_state_v2_payload(&snapshot))
        || render_current_state_v2(&snapshot) != member.bytes_utf8
        || member.binding.artifact_id.as_deref()
            != Some(format!("current-image-{}", snapshot.artifact_hash).as_str())
    {
        return Err(AppError::blocked(
            "prepared state current canonical/binding 불일치",
        ));
    }
    if let Some(active) = snapshot.active_workflow.as_ref() {
        validate_workflow_id(&active.workflow_id)?;
        if active.revision == 0 || active.artifact_hash.len() != 64 {
            return Err(AppError::blocked(
                "prepared state current active workflow binding 불일치",
            ));
        }
    }
    Ok(())
}

fn prepared_state_current_image(
    bundle: &transition::PreparedSourceBundle,
) -> Result<PreparedCurrentImage, AppError> {
    let member = bundle
        .additional_members
        .last()
        .ok_or_else(|| AppError::blocked("prepared state current member 누락"))?;
    validate_prepared_state_current_member(bundle, member)?;
    let snapshot = parse_current_state(&member.bytes_utf8, "prepared state current decode")?;
    Ok(PreparedCurrentImage {
        path: paths::current_state_file(),
        stored_path: member.path.clone(),
        artifact_id: format!("current-image-{}", snapshot.artifact_hash),
        bytes: member.bytes_utf8.clone(),
        revision: snapshot.revision,
    })
}

pub(super) fn validate_state_transition_current_cas(
    bundle: &transition::PreparedSourceBundle,
    completed_bytes: &str,
) -> Result<bool, AppError> {
    let path = paths::current_state_file();
    let existing = match fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => {
            return Err(AppError::blocked(format!(
                "prepared state current CAS 읽기 실패: {err}"
            )))
        }
    };
    if existing.as_deref() == Some(completed_bytes.as_bytes()) {
        return Ok(true);
    }
    if bundle.current_revision == 0 {
        match (bundle.current_artifact_hash.as_str(), existing.as_deref()) {
            ("missing", None) => return Ok(false),
            (hash, Some(bytes)) if sha256_bytes(bytes) == hash => return Ok(false),
            _ => {
                return Err(AppError::blocked(
                    "prepared state current missing/preserved CAS conflict",
                ))
            }
        }
    }
    let body = existing
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .ok_or_else(|| AppError::blocked("prepared state current UTF-8/CAS conflict"))?;
    let current = parse_current_state(&body, "prepared state current CAS")?;
    if current.revision != bundle.current_revision
        || current.artifact_hash != bundle.current_artifact_hash
    {
        return Err(AppError::blocked(
            "prepared state current exact CAS conflict",
        ));
    }
    Ok(false)
}

pub(crate) fn recover_prepared_state_transition(
    bundle: &transition::PreparedSourceBundle,
) -> Result<(), AppError> {
    let planned = transition::planned_events(bundle)?;
    let current_image = prepared_state_current_image(bundle)?;
    validate_state_transition_current_cas(bundle, &current_image.bytes)?;
    let workflow = if bundle.intent_kind == "checkpoint-workflow" {
        let workflow_id = bundle
            .workflow_id
            .as_deref()
            .ok_or_else(|| AppError::blocked("prepared checkpoint workflow id 누락"))?;
        let guard = WorkflowCheckpointGuard::acquire(workflow_id)?;
        let prepared = decode_prepared_workflow_revision(
            workflow_id,
            &bundle.additional_members[0],
            &bundle.additional_members[1],
            &bundle.semantic_events[0],
        )?;
        Some((guard, prepared))
    } else {
        None
    };
    let writer = ledger::LedgerWriterGuard::acquire()?;
    let expected_binding = ledger::LedgerBinding {
        event_count: planned[0].ordinal,
        event_id: Some(planned[0].event.event_id.clone()),
        event_hash: planned[0].event_hash.clone(),
    };
    let mut port = StateTransitionRecoveryPort {
        bundle,
        current_image: &current_image,
        workflow: workflow.as_ref(),
        writer: &writer,
        sink: writer.event_sink(&planned),
        expected_binding,
    };
    workflow_recovery::recover_prepared_state_transition(&mut port)
}

struct StateTransitionRecoveryPort<'a> {
    bundle: &'a transition::PreparedSourceBundle,
    current_image: &'a PreparedCurrentImage,
    workflow: Option<&'a (WorkflowCheckpointGuard, PreparedWorkflowRevision)>,
    writer: &'a ledger::LedgerWriterGuard,
    sink: ledger::EventSink<'a>,
    expected_binding: ledger::LedgerBinding,
}

impl PreparedStateRecoveryPort for StateTransitionRecoveryPort<'_> {
    fn install_reconcile_backup(&mut self) -> Result<(), AppError> {
        install_prepared_reconcile_backup(self.bundle)
    }

    fn install_workflow_snapshot(&mut self) -> Result<(), AppError> {
        if let Some((guard, prepared)) = self.workflow {
            guard.install_snapshot(prepared)?;
        }
        Ok(())
    }

    fn append_event(&mut self) -> Result<(), AppError> {
        self.sink
            .append_planned_under_guard(0, &self.bundle.semantic_events[0])
            .map(|_| ())
    }

    fn install_workflow_pointer(&mut self) -> Result<(), AppError> {
        if let Some((guard, prepared)) = self.workflow {
            guard.install_pointer(prepared)?;
        }
        Ok(())
    }

    fn finish_events(&mut self) -> Result<(), AppError> {
        self.sink.finish()
    }

    fn validate_ledger_binding(&mut self) -> Result<(), AppError> {
        if self.writer.binding()? != self.expected_binding {
            return Err(AppError::blocked(
                "prepared state transition ledger successor conflict",
            ));
        }
        Ok(())
    }

    fn install_current_state(&mut self) -> Result<(), AppError> {
        if !validate_state_transition_current_cas(self.bundle, &self.current_image.bytes)? {
            atomic_replace_bytes(
                &self.current_image.path,
                self.current_image.bytes.as_bytes(),
            )?;
        }
        Ok(())
    }

    fn converge_projections(&mut self) -> Result<(), AppError> {
        self.sink.converge_derived(&self.bundle.project_id)
    }
}

pub(super) fn install_prepared_reconcile_backup(
    bundle: &transition::PreparedSourceBundle,
) -> Result<(), AppError> {
    if bundle.intent_kind != "reconcile"
        || bundle.current_revision != 0
        || bundle.current_artifact_hash == "missing"
    {
        return Ok(());
    }
    let member = bundle
        .additional_members
        .first()
        .ok_or_else(|| AppError::blocked("prepared reconcile backup member 누락"))?;
    let basename = PathBuf::from(&member.path)
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| AppError::blocked("prepared reconcile backup basename 불일치"))?
        .to_string();
    let path = paths::state_dir().join(basename);
    if path.exists() {
        let existing = fs::read(&path).map_err(|err| {
            AppError::blocked(format!("prepared reconcile backup reread 실패: {err}"))
        })?;
        if existing != member.bytes_utf8.as_bytes() {
            return Err(AppError::blocked(
                "prepared reconcile backup immutable conflict",
            ));
        }
        return Ok(());
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&path)
        .map_err(|err| AppError::runtime(format!("prepared reconcile backup 생성 실패: {err}")))?;
    file.write_all(member.bytes_utf8.as_bytes())
        .map_err(|err| AppError::runtime(format!("prepared reconcile backup 쓰기 실패: {err}")))?;
    file.sync_all()
        .map_err(|err| AppError::runtime(format!("prepared reconcile backup sync 실패: {err}")))?;
    sync_parent(&path)
}

pub(super) struct StateTransitionRequest<'a> {
    pub(super) intent_id: &'a str,
    pub(super) intent: transition::CurrentStateIntent,
    pub(super) identity: &'a RuntimeIdentity,
    pub(super) event: &'a ledger::LedgerEvent,
    pub(super) resume_source: Option<&'a str>,
    pub(super) active_workflow: Option<&'a WorkflowRecord>,
    pub(super) previous: Option<&'a CurrentStateSnapshot>,
    pub(super) workflow: Option<(&'a WorkflowCheckpointGuard, &'a PreparedWorkflowRevision)>,
}

pub(super) fn transition_project_current_state_under_guard(
    transition_guard: &transition::TransitionGuard,
    request: StateTransitionRequest<'_>,
) -> Result<PreparedCurrentImage, AppError> {
    let StateTransitionRequest {
        intent_id,
        intent,
        identity,
        event,
        resume_source,
        active_workflow,
        previous,
        workflow,
    } = request;
    let writer = ledger::LedgerWriterGuard::acquire()?;
    let before_ledger = writer.binding()?;
    let planned = writer.plan_events(std::slice::from_ref(event))?;
    let final_binding = ledger::LedgerBinding {
        event_count: planned[0].ordinal,
        event_id: Some(planned[0].event.event_id.clone()),
        event_hash: planned[0].event_hash.clone(),
    };
    let current_image = prepare_state_transition_current_image(
        identity,
        resume_source,
        active_workflow,
        &final_binding,
        previous,
    )?;
    let current_revision = previous.map_or(0, |snapshot| snapshot.revision);
    let current_artifact_hash = previous
        .map(|snapshot| snapshot.artifact_hash.as_str())
        .unwrap_or("missing");
    let mut bundle = transition::prepare_state_transition_bundle(
        intent_id,
        intent,
        identity,
        workflow.map(|(_, prepared)| prepared.record.workflow_id.as_str()),
        current_revision,
        current_artifact_hash,
        before_ledger,
    )?;
    transition::bind_planned_events(&mut bundle, &planned)?;
    let expected_type = if previous.is_some() { "file" } else { "absent" };
    let mut members = Vec::new();
    let causal_id = workflow.map(|(_, prepared)| prepared.snapshot_member_id.clone());
    if let Some((_, prepared)) = workflow {
        members.push(prepared_workflow_member(
            transition::PreparedMemberKind::WorkflowSnapshot,
            prepared.snapshot_stored_path.clone(),
            prepared.snapshot_member_id.clone(),
            None,
            event.event_id.clone(),
            prepared.snapshot_bytes.clone(),
            "absent",
        ));
        members.push(prepared_workflow_member(
            transition::PreparedMemberKind::WorkflowPointer,
            prepared.pointer_stored_path.clone(),
            prepared.pointer_member_id.clone(),
            Some(prepared.snapshot_member_id.clone()),
            event.event_id.clone(),
            prepared.pointer_bytes.clone(),
            "file",
        ));
    }
    members.push(state_transition_current_member(
        &current_image,
        &event.event_id,
        causal_id,
        expected_type,
    ));
    transition::bind_additional_members(&mut bundle, members)?;
    let journal = transition_guard.commit(&bundle)?;
    let checkpoint = intent == transition::CurrentStateIntent::CheckpointWorkflow;
    let mut port = StateTransitionTransactionAdapter {
        transition_guard,
        bundle: &bundle,
        current: &current_image,
        workflow,
        event,
        journal: &journal,
        sink: writer.event_sink(&planned),
    };
    transaction_coordinator::execute_state_transition(&mut port, checkpoint)?;
    Ok(current_image)
}

struct StateTransitionTransactionAdapter<'a> {
    transition_guard: &'a transition::TransitionGuard,
    bundle: &'a transition::PreparedSourceBundle,
    current: &'a PreparedCurrentImage,
    workflow: Option<(&'a WorkflowCheckpointGuard, &'a PreparedWorkflowRevision)>,
    event: &'a ledger::LedgerEvent,
    journal: &'a std::path::Path,
    sink: ledger::EventSink<'a>,
}

impl StateTransitionTransactionPort for StateTransitionTransactionAdapter<'_> {
    fn fault(&mut self, point: StateTransitionFault) -> Result<(), AppError> {
        match point {
            StateTransitionFault::Journal => state_transition_fault("after-journal"),
            StateTransitionFault::CheckpointTransaction => checkpoint_fault("after-transaction"),
            StateTransitionFault::CheckpointSnapshot => checkpoint_fault("after-snapshot"),
            StateTransitionFault::Artifacts => state_transition_fault("after-artifacts"),
            StateTransitionFault::Ledger => state_transition_fault("after-ledger"),
            StateTransitionFault::CheckpointLedger => checkpoint_fault("after-ledger"),
            StateTransitionFault::CheckpointPointer => checkpoint_fault("after-pointer"),
            StateTransitionFault::Current => state_transition_fault("after-current"),
            StateTransitionFault::Projection => state_transition_fault("after-projection"),
        }
    }

    fn install_snapshot(&mut self) -> Result<(), AppError> {
        if let Some((guard, prepared)) = self.workflow {
            guard.install_snapshot(prepared)?;
        }
        Ok(())
    }

    fn append_event(&mut self) -> Result<(), AppError> {
        self.sink
            .append_planned_under_guard(0, self.event)
            .map(|_| ())
    }

    fn install_pointer(&mut self) -> Result<(), AppError> {
        if let Some((guard, prepared)) = self.workflow {
            guard.install_pointer(prepared)?;
        }
        Ok(())
    }

    fn finish_events(&mut self) -> Result<(), AppError> {
        self.sink.finish()
    }

    fn install_current(&mut self) -> Result<(), AppError> {
        if !validate_state_transition_current_cas(self.bundle, &self.current.bytes)? {
            atomic_replace_bytes(&self.current.path, self.current.bytes.as_bytes())?;
        }
        Ok(())
    }

    fn converge(&mut self) -> Result<(), AppError> {
        self.sink.converge_derived(&self.bundle.project_id)
    }

    fn remove_journal(&mut self) -> Result<(), AppError> {
        self.transition_guard.remove(self.bundle, self.journal)
    }
}

pub(super) fn prepared_workflow_member(
    kind: transition::PreparedMemberKind,
    path: String,
    artifact_id: String,
    causal_id: Option<String>,
    event_id: String,
    bytes_utf8: String,
    expected_type: &str,
) -> transition::PreparedMember {
    transition::PreparedMember {
        kind,
        path,
        schema_version: WORKFLOW_SCHEMA_VERSION,
        binding: transition::PreparedMemberBinding {
            artifact_id: Some(artifact_id),
            causal_id,
            source_key: None,
            event_id: Some(event_id),
        },
        bytes_utf8,
        expected_type: expected_type.to_string(),
        expected_identity: None,
        readonly: false,
        mode: 0o600,
        ownership: None,
        semantic_role_rank: 0,
    }
}
