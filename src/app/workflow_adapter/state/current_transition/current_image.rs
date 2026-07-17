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

pub(in super::super) fn prepare_terminal_current_image_after(
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

pub(in super::super) fn prepare_state_transition_current_image(
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

pub(in super::super) fn state_transition_current_member(
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

pub(super) fn prepared_state_current_image(
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

pub(in super::super) fn validate_state_transition_current_cas(
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
