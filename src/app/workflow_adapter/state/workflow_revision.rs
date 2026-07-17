use super::*;

pub(crate) struct WorkflowCheckpointGuard {
    workflow_id: String,
    _lease: lease::RecoverableLease,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedWorkflowRevision {
    pub record: WorkflowRecord,
    pub snapshot_path: PathBuf,
    pub snapshot_stored_path: String,
    pub snapshot_bytes: String,
    pub snapshot_member_id: String,
    pub pointer_path: PathBuf,
    pub pointer_stored_path: String,
    pub pointer_bytes: String,
    pub pointer_member_id: String,
    pub event: ledger::LedgerEvent,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedCurrentImage {
    pub path: PathBuf,
    pub stored_path: String,
    pub bytes: String,
    pub artifact_id: String,
    pub revision: u64,
}

pub(crate) struct PreparedTerminalSource {
    pub plan: transition::SourceInstallV1,
    pub before: Vec<u8>,
    pub proposed: Vec<u8>,
}

pub fn create_workflow(request: &str) -> Result<WorkflowRecord, AppError> {
    ensure_layout()?;
    let identity = ledger::validated_current_identity()?;
    let record = WorkflowRecord::new(&identity, request);
    checkpoint_workflow(record, 0)
}

pub fn checkpoint_workflow(
    next: WorkflowRecord,
    expected_revision: u64,
) -> Result<WorkflowRecord, AppError> {
    validate_workflow_id(&next.workflow_id)?;
    let transition_guard = transition::TransitionGuard::acquire_for(
        &next.project_id,
        transition::CurrentStateIntent::CheckpointWorkflow,
    )?;
    checkpoint_workflow_under_transition(&transition_guard, next, expected_revision)
}

pub(crate) fn checkpoint_workflow_under_transition(
    transition_guard: &transition::TransitionGuard,
    next: WorkflowRecord,
    expected_revision: u64,
) -> Result<WorkflowRecord, AppError> {
    validate_workflow_id(&next.workflow_id)?;
    let workflow_guard = WorkflowCheckpointGuard::acquire(&next.workflow_id)?;
    let prepared = if expected_revision == 0 {
        workflow_guard.prepare_initial(next)?
    } else {
        let current = workflow_guard.load_current()?;
        if current.revision != expected_revision || current.artifact_hash != next.artifact_hash {
            return Err(AppError::blocked(format!(
                "workflow 저장 차단\n- 이유: revision/hash conflict\n- workflow id: {}\n- expected revision: {}\n- current revision: {}",
                next.workflow_id, expected_revision, current.revision
            )));
        }
        workflow_guard.prepare_revision(&current, next)?
    };
    let identity = workflow_identity(&prepared.record);
    let previous = read_valid_current_for_transition()?;
    if previous
        .as_ref()
        .is_some_and(|current| current.project_id != identity.project_id)
    {
        return Err(AppError::blocked(
            "workflow checkpoint current-state owner binding 불일치",
        ));
    }
    let intent_id = internal_transition_intent_id(&prepared.event);
    transition_project_current_state_under_guard(
        transition_guard,
        StateTransitionRequest {
            intent_id: &intent_id,
            intent: transition::CurrentStateIntent::CheckpointWorkflow,
            identity: &identity,
            event: &prepared.event,
            resume_source: None,
            active_workflow: Some(&prepared.record),
            previous: previous.as_ref(),
            workflow: Some((&workflow_guard, &prepared)),
        },
    )?;
    Ok(prepared.record)
}

impl WorkflowCheckpointGuard {
    pub(crate) fn acquire(workflow_id: &str) -> Result<Self, AppError> {
        validate_workflow_id(workflow_id)?;
        let lease = lease::RecoverableLease::acquire(
            paths::project_workflows_dir().join(format!("{workflow_id}.checkpoint.lock")),
            "workflow checkpoint",
        )?;
        recover_workflow_transaction(workflow_id)?;
        Ok(Self {
            workflow_id: workflow_id.to_string(),
            _lease: lease,
        })
    }

    pub(crate) fn load_current(&self) -> Result<WorkflowRecord, AppError> {
        load_workflow_under_transition(&self.workflow_id)
    }

    fn prepare_initial(
        &self,
        mut next: WorkflowRecord,
    ) -> Result<PreparedWorkflowRevision, AppError> {
        if next.workflow_id != self.workflow_id
            || next.revision != 0
            || paths::project_workflow_file(&self.workflow_id).exists()
        {
            return Err(AppError::blocked(
                "prepared initial workflow predecessor binding 불일치",
            ));
        }
        next.revision = 1;
        next.previous_hash = "none".to_string();
        next.artifact_hash = sha256_text(&workflow_payload(&next));
        build_prepared_workflow_revision(next)
    }

    pub(crate) fn load_recovery_current(
        &self,
        allowed_bindings: &[(u64, &str)],
    ) -> Result<WorkflowRecord, AppError> {
        let pointer_path = paths::project_workflow_file(&self.workflow_id);
        let pointer_bytes = read_regular_file_bounded(
            &pointer_path,
            MAX_WORKFLOW_POINTER_BYTES,
            "prepared recovery workflow pointer",
        )?;
        let pointer = parse_workflow_pointer(&pointer_path, &pointer_bytes)?;
        if pointer.workflow_id != self.workflow_id
            || !allowed_bindings.iter().any(|(revision, hash)| {
                *revision == pointer.committed_revision && *hash == pointer.artifact_hash
            })
        {
            return Err(AppError::blocked(
                "prepared recovery workflow pointer binding 불일치",
            ));
        }
        let snapshot_path =
            paths::project_workflow_snapshot_file(&self.workflow_id, pointer.committed_revision);
        let snapshot_bytes = read_regular_file_bounded(
            &snapshot_path,
            MAX_WORKFLOW_SNAPSHOT_BYTES,
            "prepared recovery workflow snapshot",
        )?;
        let record = parse_workflow_snapshot(&snapshot_path, &snapshot_bytes)?;
        if record.workflow_id != self.workflow_id
            || record.revision != pointer.committed_revision
            || record.artifact_hash != pointer.artifact_hash
            || render_workflow(&record) != snapshot_bytes
            || render_workflow_pointer_bytes(&record, pointer.schema_version)? != pointer_bytes
        {
            return Err(AppError::blocked(
                "prepared recovery workflow snapshot/pointer canonical binding 불일치",
            ));
        }
        Ok(record)
    }

    pub(crate) fn prepare_revision(
        &self,
        current: &WorkflowRecord,
        mut next: WorkflowRecord,
    ) -> Result<PreparedWorkflowRevision, AppError> {
        if current.workflow_id != self.workflow_id
            || next.workflow_id != self.workflow_id
            || next.revision != current.revision
            || next.artifact_hash != current.artifact_hash
        {
            return Err(AppError::blocked(
                "prepared workflow revision CAS binding 불일치",
            ));
        }
        next.previous_hash = current.artifact_hash.clone();
        next.revision = current
            .revision
            .checked_add(1)
            .ok_or_else(|| AppError::blocked("workflow revision overflow"))?;
        next.artifact_hash = sha256_text(&workflow_payload(&next));
        build_prepared_workflow_revision(next)
    }

    pub(super) fn install_snapshot(
        &self,
        prepared: &PreparedWorkflowRevision,
    ) -> Result<(), AppError> {
        if prepared.record.workflow_id != self.workflow_id
            || prepared.snapshot_path
                != paths::project_workflow_snapshot_file(
                    &self.workflow_id,
                    prepared.record.revision,
                )
        {
            return Err(AppError::blocked(
                "prepared workflow snapshot guard binding 불일치",
            ));
        }
        write_workflow_snapshot_bytes(&prepared.record, prepared.snapshot_bytes.as_bytes())
    }

    pub(super) fn install_pointer(
        &self,
        prepared: &PreparedWorkflowRevision,
    ) -> Result<(), AppError> {
        if prepared.record.workflow_id != self.workflow_id
            || prepared.pointer_path != paths::project_workflow_file(&self.workflow_id)
        {
            return Err(AppError::blocked(
                "prepared workflow pointer guard binding 불일치",
            ));
        }
        if prepared.pointer_path.exists() {
            let existing_bytes = read_regular_file_bounded(
                &prepared.pointer_path,
                MAX_WORKFLOW_POINTER_BYTES,
                "prepared workflow pointer reread",
            )?;
            let existing = parse_workflow_pointer(&prepared.pointer_path, &existing_bytes)?;
            if existing.workflow_id != self.workflow_id {
                return Err(AppError::blocked(
                    "prepared workflow pointer workflow binding 불일치",
                ));
            }
            if existing.committed_revision == prepared.record.revision {
                if existing_bytes == prepared.pointer_bytes
                    && existing.artifact_hash == prepared.record.artifact_hash
                {
                    return Ok(());
                }
                return Err(AppError::blocked(
                    "prepared workflow pointer immutable revision conflict",
                ));
            }
            if existing.committed_revision > prepared.record.revision {
                if existing.committed_revision
                    != prepared.record.revision.checked_add(1).unwrap_or(0)
                {
                    return Err(AppError::blocked(
                        "prepared workflow pointer newer revision 범위 불일치",
                    ));
                }
                let newer_path = paths::project_workflow_snapshot_file(
                    &self.workflow_id,
                    existing.committed_revision,
                );
                let newer_bytes = read_regular_file_bounded(
                    &newer_path,
                    MAX_WORKFLOW_SNAPSHOT_BYTES,
                    "prepared workflow pointer newer snapshot",
                )?;
                let newer = parse_workflow_snapshot(&newer_path, &newer_bytes)?;
                if newer.workflow_id != self.workflow_id
                    || newer.revision != existing.committed_revision
                    || newer.previous_hash != prepared.record.artifact_hash
                    || newer.artifact_hash != existing.artifact_hash
                    || render_workflow(&newer) != newer_bytes
                {
                    return Err(AppError::blocked(
                        "prepared workflow pointer newer chain binding 불일치",
                    ));
                }
                return Ok(());
            }
        }
        atomic_replace_bytes(&prepared.pointer_path, prepared.pointer_bytes.as_bytes())
    }
}

fn build_prepared_workflow_revision(
    next: WorkflowRecord,
) -> Result<PreparedWorkflowRevision, AppError> {
    let snapshot_bytes = render_workflow(&next);
    let pointer_bytes = render_workflow_pointer_bytes(&next, WORKFLOW_SCHEMA_VERSION)?;
    let identity = workflow_identity(&next);
    let event = workflow_checkpoint_event(&next, &identity);
    Ok(PreparedWorkflowRevision {
        snapshot_path: paths::project_workflow_snapshot_file(&next.workflow_id, next.revision),
        snapshot_stored_path: format!(
            ".rpotato/workflows/{}.snapshots/{:020}.json",
            next.workflow_id, next.revision
        ),
        snapshot_member_id: prepared_workflow_member_id(
            "rpotato.workflow-snapshot-member-id/v1",
            "workflow-snapshot",
            &next.workflow_id,
            next.revision,
        ),
        pointer_path: paths::project_workflow_file(&next.workflow_id),
        pointer_stored_path: format!(".rpotato/workflows/{}.json", next.workflow_id),
        pointer_member_id: prepared_workflow_member_id(
            "rpotato.workflow-pointer-member-id/v1",
            "workflow-pointer",
            &next.workflow_id,
            next.revision,
        ),
        pointer_bytes,
        snapshot_bytes,
        event,
        record: next,
    })
}

pub(crate) fn decode_prepared_workflow_revision(
    workflow_id: &str,
    snapshot_member: &transition::PreparedMember,
    pointer_member: &transition::PreparedMember,
    event: &ledger::LedgerEvent,
) -> Result<PreparedWorkflowRevision, AppError> {
    use transition::PreparedMemberKind;

    validate_workflow_id(workflow_id)?;
    if snapshot_member.kind != PreparedMemberKind::WorkflowSnapshot
        || pointer_member.kind != PreparedMemberKind::WorkflowPointer
        || snapshot_member.schema_version != WORKFLOW_SCHEMA_VERSION
        || pointer_member.schema_version != WORKFLOW_SCHEMA_VERSION
        || snapshot_member.expected_type != "absent"
        || pointer_member.expected_type != "file"
    {
        return Err(AppError::blocked(
            "prepared workflow member kind/schema/type 불일치",
        ));
    }
    let probe_path = paths::project_workflow_file(workflow_id);
    let record = parse_workflow_snapshot(&probe_path, &snapshot_member.bytes_utf8)?;
    let snapshot_path = paths::project_workflow_snapshot_file(workflow_id, record.revision);
    let pointer_path = paths::project_workflow_file(workflow_id);
    let snapshot_stored_path = format!(
        ".rpotato/workflows/{workflow_id}.snapshots/{:020}.json",
        record.revision
    );
    let pointer_stored_path = format!(".rpotato/workflows/{workflow_id}.json");
    let snapshot_member_id = prepared_workflow_member_id(
        "rpotato.workflow-snapshot-member-id/v1",
        "workflow-snapshot",
        workflow_id,
        record.revision,
    );
    let pointer_member_id = prepared_workflow_member_id(
        "rpotato.workflow-pointer-member-id/v1",
        "workflow-pointer",
        workflow_id,
        record.revision,
    );
    let pointer = parse_workflow_pointer(&pointer_path, &pointer_member.bytes_utf8)?;
    let expected_pointer_bytes = render_workflow_pointer_bytes(&record, WORKFLOW_SCHEMA_VERSION)?;
    let expected_event_details = workflow_checkpoint_event_details(&record);
    if record.workflow_id != workflow_id
        || render_workflow(&record) != snapshot_member.bytes_utf8
        || snapshot_member.path != snapshot_stored_path
        || pointer_member.path != pointer_stored_path
        || snapshot_member.binding.artifact_id.as_deref() != Some(snapshot_member_id.as_str())
        || pointer_member.binding.artifact_id.as_deref() != Some(pointer_member_id.as_str())
        || snapshot_member.binding.causal_id.is_some()
        || pointer_member.binding.causal_id.as_deref() != Some(snapshot_member_id.as_str())
        || snapshot_member.binding.event_id.as_deref() != Some(event.event_id.as_str())
        || pointer_member.binding.event_id.as_deref() != Some(event.event_id.as_str())
        || pointer.workflow_id != workflow_id
        || pointer.committed_revision != record.revision
        || pointer.artifact_hash != record.artifact_hash
        || pointer.schema_version != WORKFLOW_SCHEMA_VERSION
        || pointer_member.bytes_utf8 != expected_pointer_bytes
        || event.event_type != "workflow.checkpoint"
        || event.project_id != record.project_id
        || event.session_id != record.session_id
        || event.summary != "canonical workflow revision persisted"
        || event.details != expected_event_details
    {
        return Err(AppError::blocked(
            "prepared workflow snapshot/pointer/event binding 불일치",
        ));
    }
    Ok(PreparedWorkflowRevision {
        record,
        snapshot_path,
        snapshot_stored_path,
        snapshot_bytes: snapshot_member.bytes_utf8.clone(),
        snapshot_member_id,
        pointer_path,
        pointer_stored_path,
        pointer_bytes: pointer_member.bytes_utf8.clone(),
        pointer_member_id,
        event: event.clone(),
    })
}
