use super::*;

pub fn load_workflow(workflow_id: &str) -> Result<WorkflowRecord, AppError> {
    let identity = ledger::validated_current_identity()?;
    let _transition_guard = transition::TransitionGuard::acquire_for(
        &identity.project_id,
        transition::CurrentStateIntent::RecoverWorkflow,
    )?;
    load_workflow_under_transition(workflow_id)
}

pub(crate) fn load_workflow_revision(
    workflow_id: &str,
    revision: u64,
) -> Result<WorkflowRecord, AppError> {
    if revision == 0 {
        return Err(AppError::blocked(
            "workflow historical revision은 1 이상이어야 합니다.",
        ));
    }
    let identity = ledger::validated_current_identity()?;
    let _transition_guard = transition::TransitionGuard::acquire_for(
        &identity.project_id,
        transition::CurrentStateIntent::RecoverWorkflow,
    )?;
    let latest = load_workflow_under_transition(workflow_id)?;
    if revision > latest.revision {
        return Err(AppError::blocked("workflow historical revision 범위 오류"));
    }
    let path = paths::project_workflow_snapshot_file(workflow_id, revision);
    let body = read_regular_file_bounded(
        &path,
        MAX_WORKFLOW_SNAPSHOT_BYTES,
        "historical workflow snapshot",
    )?;
    let record = parse_workflow_snapshot(&path, &body)?;
    if record.workflow_id != workflow_id
        || record.revision != revision
        || record.project_id != identity.project_id
        || render_workflow(&record) != body
    {
        return Err(corrupt_workflow(&path));
    }
    Ok(record)
}

pub(super) fn load_workflow_under_transition(
    workflow_id: &str,
) -> Result<WorkflowRecord, AppError> {
    validate_workflow_id(workflow_id)?;
    recover_workflow_transaction(workflow_id)?;
    let pointer_path = paths::project_workflow_file(workflow_id);
    let pointer = read_regular_file_bounded(
        &pointer_path,
        MAX_WORKFLOW_POINTER_BYTES,
        "committed workflow pointer",
    )?;
    let pointer = parse_workflow_pointer(&pointer_path, &pointer)?;
    if pointer.workflow_id != workflow_id || pointer.committed_revision == 0 {
        return Err(corrupt_workflow(&pointer_path));
    }
    let record = validate_workflow_chain(
        workflow_id,
        pointer.committed_revision,
        pointer.schema_version,
    )?;
    let identity = ledger::validated_current_identity()?;
    if record.artifact_hash != pointer.artifact_hash || record.project_id != identity.project_id {
        return Err(corrupt_workflow(&pointer_path));
    }
    Ok(record)
}

pub fn active_workflow_id() -> Result<Option<String>, AppError> {
    let identity = ledger::validated_current_identity()?;
    let transition_guard = transition::TransitionGuard::acquire_for(
        &identity.project_id,
        transition::CurrentStateIntent::RepairWorkflowPointer,
    )?;
    active_workflow_id_under_transition(&transition_guard)
}

fn active_workflow_id_under_transition(
    transition_guard: &transition::TransitionGuard,
) -> Result<Option<String>, AppError> {
    let discovered = discover_active_workflow()?;
    let path = paths::current_state_file();
    if !path.exists() {
        if let Some(workflow_id) = discovered.as_deref() {
            let workflow = load_workflow_under_transition(workflow_id)?;
            let identity = workflow_identity(&workflow);
            let event = ledger::new_event_for(
                &identity,
                "workflow.pointer.recovered",
                "active workflow pointer recovered",
                &format!("workflow_id={workflow_id} predecessor=missing"),
            );
            let intent_id = internal_transition_intent_id(&event);
            transition_project_current_state_under_guard(
                transition_guard,
                StateTransitionRequest {
                    intent_id: &intent_id,
                    intent: transition::CurrentStateIntent::RepairWorkflowPointer,
                    identity: &identity,
                    event: &event,
                    resume_source: Some("workflow-pointer-recovery"),
                    active_workflow: Some(&workflow),
                    previous: None,
                    compaction_boundary: CompactionBoundaryUpdate::Preserve,
                    workflow: None,
                },
            )?;
        }
        return Ok(discovered);
    }
    let body = fs::read_to_string(&path).map_err(|err| {
        AppError::runtime(format!(
            "current-state를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    let snapshot = parse_current_state(&body, "current-state")?;
    if snapshot.project_id != ledger::fresh_identity().project_id {
        return Err(AppError::blocked(
            "current-state schema/project binding이 손상되었습니다.",
        ));
    }
    let pointer = snapshot
        .active_workflow
        .as_ref()
        .map(|workflow| workflow.workflow_id.clone());
    match (pointer, discovered) {
        (None, None) => Ok(None),
        (None, Some(workflow_id)) => {
            let workflow = load_workflow_under_transition(&workflow_id)?;
            let identity = workflow_identity(&workflow);
            let event = ledger::new_event_for(
                &identity,
                "workflow.pointer.recovered",
                "active workflow pointer recovered",
                &format!("workflow_id={workflow_id} predecessor=null"),
            );
            let intent_id = internal_transition_intent_id(&event);
            transition_project_current_state_under_guard(
                transition_guard,
                StateTransitionRequest {
                    intent_id: &intent_id,
                    intent: transition::CurrentStateIntent::RepairWorkflowPointer,
                    identity: &identity,
                    event: &event,
                    resume_source: Some("workflow-pointer-recovery"),
                    active_workflow: Some(&workflow),
                    previous: Some(&snapshot),
                    compaction_boundary: CompactionBoundaryUpdate::Preserve,
                    workflow: None,
                },
            )?;
            Ok(Some(workflow_id))
        }
        (Some(pointer), Some(workflow_id)) if pointer == workflow_id => Ok(Some(workflow_id)),
        (Some(pointer), None) => {
            let workflow = load_workflow_under_transition(&pointer)?;
            if !workflow.is_terminal() {
                return Err(AppError::blocked(
                    "workflow resume 차단\n- 이유: current pointer와 전체 artifact scan이 충돌합니다.",
                ));
            }
            if matches!(workflow.phase.as_str(), "failed" | "cancelled") {
                clear_terminal_workflow_pointer_under_transition(transition_guard, &workflow)?;
                return Ok(None);
            }
            Ok(Some(pointer))
        }
        _ => Err(AppError::blocked(
            "workflow resume 차단\n- 이유: current pointer와 non-terminal artifact가 충돌합니다.\n- 동작: fail-closed; backend와 side effect를 실행하지 않습니다.",
        )),
    }
}

pub(crate) fn clear_terminal_workflow_pointer(workflow: &WorkflowRecord) -> Result<(), AppError> {
    let transition_guard = transition::TransitionGuard::acquire_for(
        &workflow.project_id,
        transition::CurrentStateIntent::ClearTerminalWorkflow,
    )?;
    clear_terminal_workflow_pointer_under_transition(&transition_guard, workflow)
}

pub(crate) fn clear_terminal_workflow_pointer_under_transition(
    transition_guard: &transition::TransitionGuard,
    workflow: &WorkflowRecord,
) -> Result<(), AppError> {
    if !workflow.is_terminal() {
        return Err(AppError::blocked(
            "terminal workflow pointer cleanup 차단: workflow가 terminal이 아닙니다.",
        ));
    }
    let path = paths::current_state_file();
    let body = fs::read_to_string(&path).map_err(|err| {
        AppError::blocked(format!("terminal pointer current-state 읽기 실패: {err}"))
    })?;
    let snapshot = parse_current_state(&body, "current-state")?;
    match snapshot.active_workflow.as_ref() {
        Some(binding) if binding.workflow_id == workflow.workflow_id => {}
        None => return Ok(()),
        _ => {
            return Err(AppError::blocked(
                "terminal workflow pointer cleanup 차단: current pointer conflict",
            ))
        }
    }
    let identity = workflow_identity(workflow);
    let event = ledger::new_event_for(
        &identity,
        "workflow.pointer.cleared",
        "terminal workflow pointer cleared",
        &format!(
            "workflow_id={} revision={} artifact_hash={}",
            workflow.workflow_id, workflow.revision, workflow.artifact_hash
        ),
    );
    let intent_id = internal_transition_intent_id(&event);
    transition_project_current_state_under_guard(
        transition_guard,
        StateTransitionRequest {
            intent_id: &intent_id,
            intent: transition::CurrentStateIntent::ClearTerminalWorkflow,
            identity: &identity,
            event: &event,
            resume_source: Some("terminal-pointer-cleanup"),
            active_workflow: None,
            previous: Some(&snapshot),
            compaction_boundary: CompactionBoundaryUpdate::Preserve,
            workflow: None,
        },
    )
    .map(|_| ())
}

pub(crate) fn record_tui_workflow_resume_receipt_under_transition(
    transition_guard: &transition::TransitionGuard,
    workflow: &WorkflowRecord,
    intent_id: &str,
    active_workflow: Option<&WorkflowRecord>,
) -> Result<String, AppError> {
    let previous = read_valid_current_for_transition()?
        .ok_or_else(|| AppError::blocked("workflow resume current-state 누락"))?;
    if previous.project_id != workflow.project_id
        || previous.session_id != workflow.session_id
        || active_workflow.is_some_and(|active| active.workflow_id != workflow.workflow_id)
    {
        return Err(AppError::blocked(
            "workflow resume receipt current/workflow binding 불일치",
        ));
    }
    let identity = workflow_identity(workflow);
    let event = ledger::new_event_for(
        &identity,
        "workflow.resume.accepted",
        "TUI workflow resume accepted",
        &format!("intent_id={intent_id} workflow_id={}", workflow.workflow_id),
    );
    transition_project_current_state_under_guard(
        transition_guard,
        StateTransitionRequest {
            intent_id,
            intent: transition::CurrentStateIntent::Resume,
            identity: &identity,
            event: &event,
            resume_source: Some("interactive-tui"),
            active_workflow,
            previous: Some(&previous),
            compaction_boundary: CompactionBoundaryUpdate::Preserve,
            workflow: None,
        },
    )?;
    Ok(event.event_id)
}

pub(crate) fn record_workflow_event_under_transition(
    transition_guard: &transition::TransitionGuard,
    workflow: &WorkflowRecord,
    event_type: &str,
    summary: &str,
    details: &str,
) -> Result<String, AppError> {
    let previous = read_valid_current_for_transition()?
        .ok_or_else(|| AppError::blocked("workflow event current-state 누락"))?;
    let Some(active) = previous.active_workflow.as_ref() else {
        return Err(AppError::blocked(
            "workflow event current/workflow binding 누락",
        ));
    };
    if previous.project_id != workflow.project_id
        || previous.session_id != workflow.session_id
        || active.workflow_id != workflow.workflow_id
        || active.revision != workflow.revision
        || active.artifact_hash != workflow.artifact_hash
    {
        return Err(AppError::blocked(
            "workflow event current/workflow binding 불일치",
        ));
    }
    let identity = workflow_identity(workflow);
    let event = ledger::new_event_for(&identity, event_type, summary, details);
    let intent_id = internal_transition_intent_id(&event);
    transition_project_current_state_under_guard(
        transition_guard,
        StateTransitionRequest {
            intent_id: &intent_id,
            intent: transition::CurrentStateIntent::RecordEvent,
            identity: &identity,
            event: &event,
            resume_source: Some("workflow-event-recovery"),
            active_workflow: Some(workflow),
            previous: Some(&previous),
            compaction_boundary: CompactionBoundaryUpdate::Preserve,
            workflow: None,
        },
    )?;
    Ok(event.event_id)
}

pub(super) fn discover_active_workflow() -> Result<Option<String>, AppError> {
    let entries = match fs::read_dir(paths::project_workflows_dir()) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(AppError::runtime(format!(
                "workflow directory를 읽지 못했습니다: {err}"
            )))
        }
    };
    let mut workflow_ids = std::collections::BTreeSet::new();
    for entry in entries {
        let path = entry
            .map_err(|err| AppError::runtime(format!("workflow entry read 실패: {err}")))?
            .path();
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| corrupt_workflow(&path))?;
        let workflow_id = name
            .strip_suffix(".json")
            .or_else(|| name.strip_suffix(".txn"))
            .or_else(|| name.strip_suffix(".snapshots"));
        let Some(workflow_id) = workflow_id else {
            continue;
        };
        validate_workflow_id(workflow_id)?;
        workflow_ids.insert(workflow_id.to_string());
    }
    let mut active = Vec::new();
    for workflow_id in workflow_ids {
        if !paths::project_workflow_file(&workflow_id).exists()
            && !paths::project_workflow_transaction_file(&workflow_id).exists()
        {
            return Err(AppError::blocked(format!(
                "workflow scan 차단\n- 이유: committed pointer와 transaction이 없는 snapshot artifact\n- workflow id: {workflow_id}"
            )));
        }
        let workflow = load_workflow_under_transition(&workflow_id)?;
        if !workflow.is_terminal() {
            active.push(workflow.workflow_id);
        }
    }
    match active.as_slice() {
        [] => Ok(None),
        [workflow_id] => Ok(Some(workflow_id.clone())),
        _ => Err(AppError::blocked(
            "workflow resume 차단\n- 이유: 여러 non-terminal canonical workflow가 충돌합니다.\n- 동작: fail-closed; backend와 side effect를 실행하지 않습니다.",
        )),
    }
}
