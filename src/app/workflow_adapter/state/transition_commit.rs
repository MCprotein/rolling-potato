use super::*;

use crate::adapters::filesystem::atomic_write::atomic_replace_bytes;

pub(super) struct CompactionBoundaryCommit<'a> {
    update: CompactionBoundaryUpdate<'a>,
    expected: Option<Option<&'a str>>,
}

impl CompactionBoundaryCommit<'_> {
    pub(super) fn preserve() -> Self {
        Self {
            update: CompactionBoundaryUpdate::Preserve,
            expected: None,
        }
    }

    pub(super) fn set<'a>(
        boundary: &'a str,
        expected: Option<&'a str>,
    ) -> CompactionBoundaryCommit<'a> {
        CompactionBoundaryCommit {
            update: CompactionBoundaryUpdate::Set(boundary),
            expected: Some(expected),
        }
    }
}

pub(super) fn read_valid_current_for_transition() -> Result<Option<CurrentStateSnapshot>, AppError>
{
    let path = paths::current_state_file();
    let body = match fs::read_to_string(&path) {
        Ok(body) => body,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(AppError::blocked(format!(
                "current-state transition precondition 읽기 실패: {err}"
            )))
        }
    };
    let snapshot = parse_current_state(&body, "current-state transition precondition")?;
    if snapshot.schema_version == 1 {
        promote_current_state_v1()?;
        return read_valid_current_for_transition();
    }
    Ok(Some(snapshot))
}

pub(super) fn internal_transition_intent_id(event: &ledger::LedgerEvent) -> String {
    format!("intent-{}", event.event_id)
}

pub(super) fn commit_state_event(
    intent_id: &str,
    intent: transition::CurrentStateIntent,
    identity: &RuntimeIdentity,
    event: &ledger::LedgerEvent,
    resume_source: Option<&str>,
    active_workflow_id: Option<&str>,
    compaction: CompactionBoundaryCommit<'_>,
) -> Result<PreparedCurrentImage, AppError> {
    let transition_guard = transition::TransitionGuard::acquire_for(&identity.project_id, intent)?;
    let previous = read_valid_current_for_transition()?;
    if previous
        .as_ref()
        .is_some_and(|snapshot| snapshot.project_id != identity.project_id)
    {
        return Err(AppError::blocked(
            "state transition current project binding 불일치",
        ));
    }
    if let Some(expected) = compaction.expected {
        let actual = previous
            .as_ref()
            .and_then(|snapshot| snapshot.compaction_boundary.as_deref());
        if actual != expected {
            return Err(AppError::blocked(
                "compaction boundary compare-and-set precondition 불일치",
            ));
        }
    }
    if intent == transition::CurrentStateIntent::Bootstrap {
        if let Some(snapshot) = previous.as_ref() {
            if snapshot.ledger_binding != ledger::validated_ledger_binding()? {
                return Err(AppError::blocked(
                    "bootstrap existing current/ledger binding 불일치",
                ));
            }
            return Ok(PreparedCurrentImage {
                path: paths::current_state_file(),
                stored_path: "state/current-state.json".to_string(),
                artifact_id: format!("current-image-{}", snapshot.artifact_hash),
                bytes: render_current_state_v2(snapshot),
                revision: snapshot.revision,
            });
        }
    }
    let active_workflow = active_workflow_id
        .map(load_workflow_under_transition)
        .transpose()?;
    if active_workflow
        .as_ref()
        .is_some_and(|workflow| workflow.session_id != identity.session_id)
    {
        return Err(AppError::blocked(
            "state transition active workflow session binding 불일치",
        ));
    }
    transition_project_current_state_under_guard(
        &transition_guard,
        StateTransitionRequest {
            intent_id,
            intent,
            identity,
            event,
            resume_source,
            active_workflow: active_workflow.as_ref(),
            previous: previous.as_ref(),
            compaction_boundary: compaction.update,
            workflow: None,
        },
    )
}

pub(super) fn reconcile_invalid_current_under_guard(
    transition_guard: &transition::TransitionGuard,
    identity: &RuntimeIdentity,
    reason: &str,
    before_bytes: &str,
) -> Result<(ledger::LedgerEvent, PathBuf), AppError> {
    let event_type = match reason {
        "corrupt" => "state.reconcile.corrupt_recovered",
        "stale" => "state.reconcile.stale_recovered",
        _ => return Err(AppError::blocked("reconcile preserved reason 불일치")),
    };
    let event = ledger::new_event_for(
        identity,
        event_type,
        "invalid current-state preserved and recovered",
        &format!("reason={reason}"),
    );
    let intent_id = internal_transition_intent_id(&event);
    let writer = ledger::LedgerWriterGuard::acquire()?;
    let before_ledger = writer.binding()?;
    let planned = writer.plan_events(std::slice::from_ref(&event))?;
    let final_binding = ledger::LedgerBinding {
        event_count: planned[0].ordinal,
        event_id: Some(planned[0].event.event_id.clone()),
        event_hash: planned[0].event_hash.clone(),
    };
    let current_image = prepare_state_transition_current_image(
        identity,
        Some("state-reconcile"),
        None,
        &final_binding,
        None,
        CompactionBoundaryUpdate::Preserve,
    )?;
    let before_hash = sha256_bytes(before_bytes.as_bytes());
    let mut bundle = transition::prepare_state_transition_bundle(
        &intent_id,
        transition::CurrentStateIntent::Reconcile,
        identity,
        None,
        0,
        &before_hash,
        before_ledger,
    )?;
    transition::bind_planned_events(&mut bundle, &planned)?;
    let backup_path = format!("state/current-state.json.{reason}.{intent_id}");
    let backup = transition::PreparedMember {
        kind: transition::PreparedMemberKind::ToolOutput,
        path: backup_path,
        schema_version: 1,
        binding: transition::PreparedMemberBinding {
            artifact_id: Some(format!("state-backup-{before_hash}")),
            causal_id: None,
            source_key: None,
            event_id: Some(event.event_id.clone()),
        },
        bytes_utf8: before_bytes.to_string(),
        expected_type: "absent".to_string(),
        expected_identity: None,
        readonly: false,
        mode: 0o600,
        ownership: None,
        semantic_role_rank: 0,
    };
    let current = state_transition_current_member(&current_image, &event.event_id, None, "file");
    transition::bind_additional_members(&mut bundle, vec![backup, current])?;
    let journal = transition_guard.commit(&bundle)?;
    let mut port = StateReconcileTransactionPort {
        transition_guard,
        bundle: &bundle,
        current: &current_image,
        event: &event,
        journal: &journal,
        sink: writer.event_sink(&planned),
    };
    transaction_coordinator::execute_reconcile_transaction(&mut port)?;
    let backup = bundle
        .additional_members
        .first()
        .and_then(|member| {
            PathBuf::from(&member.path)
                .file_name()
                .map(|name| name.to_owned())
        })
        .map(|name| paths::current_state_dir().join(name))
        .ok_or_else(|| AppError::blocked("reconcile backup result path 불일치"))?;
    Ok((event, backup))
}

struct StateReconcileTransactionPort<'a> {
    transition_guard: &'a transition::TransitionGuard,
    bundle: &'a transition::PreparedSourceBundle,
    current: &'a PreparedCurrentImage,
    event: &'a ledger::LedgerEvent,
    journal: &'a std::path::Path,
    sink: ledger::EventSink<'a>,
}

impl ReconcileTransactionPort for StateReconcileTransactionPort<'_> {
    fn fault(&mut self, point: StateTransitionFault) -> Result<(), AppError> {
        match point {
            StateTransitionFault::Journal => state_transition_fault("after-journal"),
            StateTransitionFault::Artifacts => state_transition_fault("after-artifacts"),
            StateTransitionFault::Ledger => state_transition_fault("after-ledger"),
            StateTransitionFault::Current => state_transition_fault("after-current"),
            StateTransitionFault::Projection => state_transition_fault("after-projection"),
            StateTransitionFault::CheckpointTransaction
            | StateTransitionFault::CheckpointSnapshot
            | StateTransitionFault::CheckpointLedger
            | StateTransitionFault::CheckpointPointer => Err(AppError::blocked(
                "reconcile transaction checkpoint fault 범위 불일치",
            )),
        }
    }

    fn install_backup(&mut self) -> Result<(), AppError> {
        install_prepared_reconcile_backup(self.bundle)
    }

    fn append_event(&mut self) -> Result<(), AppError> {
        self.sink
            .append_planned_under_guard(0, self.event)
            .map(|_| ())
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

pub(super) fn install_current_image(
    prepared: &PreparedCurrentImage,
    before_revision: u64,
    before_artifact_hash: &str,
) -> Result<(), AppError> {
    if prepared.path != paths::current_state_file() {
        return Err(AppError::blocked(
            "prepared current image path binding 불일치",
        ));
    }
    if prepared.revision <= before_revision {
        return Err(AppError::blocked(
            "prepared current image revision CAS 불일치",
        ));
    }
    if validate_current_state_recovery_cas(
        before_revision,
        before_artifact_hash,
        Some(&prepared.bytes),
    )? {
        return Ok(());
    }
    atomic_replace_bytes(&prepared.path, prepared.bytes.as_bytes())
}

pub(crate) fn validate_current_state_recovery_cas(
    before_revision: u64,
    before_artifact_hash: &str,
    completed_bytes: Option<&str>,
) -> Result<bool, AppError> {
    let path = paths::current_state_file();
    let existing = fs::read_to_string(&path)
        .map_err(|err| AppError::blocked(format!("current-state recovery CAS 읽기 실패: {err}")))?;
    if completed_bytes == Some(existing.as_str()) {
        return Ok(true);
    }
    let current = parse_current_state(&existing, "current-state recovery CAS")?;
    if current.revision != before_revision || current.artifact_hash != before_artifact_hash {
        return Err(AppError::blocked(
            "prepared current-state exact CAS conflict",
        ));
    }
    Ok(false)
}

pub(crate) fn decode_prepared_current_image(
    member: &transition::PreparedMember,
    workflow: &WorkflowRecord,
    ledger_binding: &ledger::LedgerBinding,
    causal_id: &str,
    event_id: &str,
) -> Result<PreparedCurrentImage, AppError> {
    use transition::PreparedMemberKind;

    if member.kind != PreparedMemberKind::CurrentImage
        || member.schema_version != 2
        || member.path != "state/current-state.json"
        || member.expected_type != "file"
        || member.binding.causal_id.as_deref() != Some(causal_id)
        || member.binding.event_id.as_deref() != Some(event_id)
    {
        return Err(AppError::blocked(
            "prepared current image member binding 불일치",
        ));
    }
    let snapshot = parse_current_state(&member.bytes_utf8, "prepared current image member")?;
    let Some(active) = snapshot.active_workflow.as_ref() else {
        return Err(AppError::blocked(
            "prepared current image active workflow 누락",
        ));
    };
    if snapshot.schema_version != 2
        || snapshot.project_id != workflow.project_id
        || snapshot.session_id != workflow.session_id
        || active.workflow_id != workflow.workflow_id
        || active.revision != workflow.revision
        || active.artifact_hash != workflow.artifact_hash
        || snapshot.ledger_binding != *ledger_binding
        || snapshot.artifact_hash != sha256_text(&render_current_state_v2_payload(&snapshot))
        || render_current_state_v2(&snapshot) != member.bytes_utf8
        || member.binding.artifact_id.as_deref()
            != Some(format!("current-image-{}", snapshot.artifact_hash).as_str())
    {
        return Err(AppError::blocked(
            "prepared current image canonical/workflow/ledger binding 불일치",
        ));
    }
    Ok(PreparedCurrentImage {
        path: paths::current_state_file(),
        stored_path: member.path.clone(),
        bytes: member.bytes_utf8.clone(),
        artifact_id: format!("current-image-{}", snapshot.artifact_hash),
        revision: snapshot.revision,
    })
}

pub(crate) fn decode_prepared_terminal_current_image(
    member: &transition::PreparedMember,
    workflow: &WorkflowRecord,
    ledger_binding: &ledger::LedgerBinding,
    causal_id: &str,
    event_id: &str,
) -> Result<PreparedCurrentImage, AppError> {
    use transition::PreparedMemberKind;

    if member.kind != PreparedMemberKind::CurrentImage
        || member.schema_version != 2
        || member.path != "state/current-state.json"
        || member.expected_type != "file"
        || member.binding.causal_id.as_deref() != Some(causal_id)
        || member.binding.event_id.as_deref() != Some(event_id)
    {
        return Err(AppError::blocked(
            "prepared terminal current member binding 불일치",
        ));
    }
    let snapshot = parse_current_state(&member.bytes_utf8, "prepared terminal current member")?;
    if snapshot.schema_version != 2
        || snapshot.project_id != workflow.project_id
        || snapshot.session_id != workflow.session_id
        || snapshot.active_workflow.is_some()
        || snapshot.ledger_binding != *ledger_binding
        || snapshot.artifact_hash != sha256_text(&render_current_state_v2_payload(&snapshot))
        || render_current_state_v2(&snapshot) != member.bytes_utf8
        || member.binding.artifact_id.as_deref()
            != Some(format!("current-image-{}", snapshot.artifact_hash).as_str())
    {
        return Err(AppError::blocked(
            "prepared terminal current canonical/workflow/ledger binding 불일치",
        ));
    }
    Ok(PreparedCurrentImage {
        path: paths::current_state_file(),
        stored_path: member.path.clone(),
        bytes: member.bytes_utf8.clone(),
        artifact_id: format!("current-image-{}", snapshot.artifact_hash),
        revision: snapshot.revision,
    })
}
