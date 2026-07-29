use super::*;

pub(crate) fn validated_identity_from_current_state(
    body: &str,
    fresh: &RuntimeIdentity,
) -> Result<RuntimeIdentity, AppError> {
    let snapshot = parse_current_state(body, "current-state identity")?;
    if snapshot.project_id != fresh.project_id || snapshot.project_root != fresh.project_root {
        return Err(AppError::blocked(
            "current-state identity project binding 불일치",
        ));
    }
    Ok(RuntimeIdentity {
        project_id: snapshot.project_id,
        session_id: snapshot.session_id,
        project_root: snapshot.project_root,
    })
}

pub(crate) fn current_state_lease_view() -> Result<CurrentStateLeaseView, AppError> {
    let identity = ledger::validated_current_identity()?;
    let _transition_guard = transition::TransitionGuard::acquire_for(
        &identity.project_id,
        transition::CurrentStateIntent::RecoverWorkflow,
    )?;
    current_state_lease_view_under_transition()
}

pub(crate) fn tui_entry_initialization_required() -> Result<bool, AppError> {
    let path = paths::current_state_file();
    if !path.exists() {
        return Ok(true);
    }
    let body = read_regular_file_bounded(&path, 128 * 1024, "TUI current-state preflight")?;
    let snapshot = parse_current_state(&body, "TUI current-state preflight")?;
    if snapshot.schema_version != 2 {
        return Ok(true);
    }
    snapshot_domain::validated_tui_identity(&snapshot, &ledger::fresh_identity())?;
    Ok(snapshot.ledger_binding != ledger::validated_ledger_binding()?)
}

pub(in crate::app::workflow_adapter::state) fn migrate_matching_legacy_current_state(
) -> Result<(), AppError> {
    let current = paths::current_state_file();
    let legacy = paths::legacy_current_state_file();
    if current.exists() || !legacy.exists() {
        return Ok(());
    }
    let body = match read_regular_file_bounded(&legacy, 128 * 1024, "legacy current-state") {
        Ok(body) => body,
        Err(_) => return Ok(()),
    };
    let snapshot = match parse_current_state(&body, "legacy current-state migration") {
        Ok(snapshot) => snapshot,
        Err(_) => return Ok(()),
    };
    let fresh = ledger::fresh_identity();
    if snapshot.project_id != fresh.project_id || snapshot.project_root != fresh.project_root {
        return Ok(());
    }
    crate::adapters::filesystem::atomic_write::atomic_replace_bytes(&current, body.as_bytes())
}

pub(in crate::app::workflow_adapter::state) fn synchronize_current_state_ledger(
    identity: &RuntimeIdentity,
) -> Result<(), AppError> {
    let Some(snapshot) = read_valid_current_for_transition()? else {
        return Ok(());
    };
    if snapshot.ledger_binding == ledger::validated_ledger_binding()? {
        return Ok(());
    }
    let event = ledger::new_event_for(
        identity,
        "runtime.project.activated",
        "현재 프로젝트 상태 활성화",
        "다른 프로젝트 실행 뒤 canonical ledger binding 동기화",
    );
    let intent_id = internal_transition_intent_id(&event);
    commit_state_event(
        &intent_id,
        transition::CurrentStateIntent::RecordEvent,
        identity,
        &event,
        None,
        snapshot
            .active_workflow
            .as_ref()
            .map(|binding| binding.workflow_id.as_str()),
        CompactionBoundaryCommit::preserve(),
    )?;
    Ok(())
}

pub(crate) fn tui_state_snapshot_read_only(
    max_ledger_events: usize,
) -> Result<TuiStateSnapshot, AppError> {
    with_validation_gap_writes_suppressed(|| {
        let path = paths::current_state_file();
        let body = read_regular_file_bounded(&path, 128 * 1024, "TUI current-state")?;
        let snapshot = parse_current_state(&body, "TUI current-state read-only")?;
        if snapshot.schema_version != 2 {
            return Err(AppError::blocked(
                "TUI read-only current-state는 schema v2 canonical image가 필요합니다.",
            ));
        }
        let fresh = ledger::fresh_identity();
        let identity = snapshot_domain::validated_tui_identity(&snapshot, &fresh)?;
        let ledger_tail =
            ledger::read_runtime_tail_read_only(max_ledger_events.max(1), 2 * 1024 * 1024)?;
        let current_ledger_binding_stale = snapshot.ledger_binding != ledger_tail.binding;
        snapshot_domain::validate_ledger_ancestor(
            &snapshot.ledger_binding,
            &ledger_tail.binding,
            &ledger_tail.events,
        )?;
        let active_workflow = snapshot
            .active_workflow
            .as_ref()
            .map(|binding| load_workflow_read_only(binding, &identity, &ledger_tail.events))
            .transpose()?;
        Ok(TuiStateSnapshot {
            identity,
            current_revision: snapshot.revision,
            current_hash: snapshot.artifact_hash,
            ledger_binding: ledger_tail.binding,
            ledger_events: ledger_tail.events,
            active_workflow,
            ledger_tail_truncated: ledger_tail.truncated,
            current_ledger_binding_stale,
        })
    })
}

fn load_workflow_read_only(
    binding: &CurrentWorkflowBinding,
    identity: &RuntimeIdentity,
    ledger_events: &[ledger::ParsedLedgerEvent],
) -> Result<WorkflowRecord, AppError> {
    validate_workflow_id(&binding.workflow_id)?;
    let transaction = paths::project_workflow_transaction_file(&binding.workflow_id);
    if transaction.exists() {
        return Err(AppError::blocked(
            "TUI workflow read-only view는 pending recovery transaction을 실행하지 않습니다.",
        ));
    }
    let pointer_path = paths::project_workflow_file(&binding.workflow_id);
    let pointer_body = read_regular_file_bounded(&pointer_path, 64 * 1024, "TUI workflow pointer")?;
    let pointer = parse_workflow_pointer(&pointer_path, &pointer_body)?;
    snapshot_domain::validate_read_only_pointer(binding, &pointer)?;
    let snapshot_path =
        paths::project_workflow_snapshot_file(&binding.workflow_id, binding.revision);
    let snapshot_body =
        read_regular_file_bounded(&snapshot_path, 512 * 1024, "TUI workflow snapshot")?;
    if workflow_snapshot_schema(&snapshot_path, &snapshot_body)? != pointer.schema_version {
        return Err(AppError::blocked(
            "TUI workflow pointer/snapshot schema binding 불일치",
        ));
    }
    let workflow = parse_workflow_snapshot(&snapshot_path, &snapshot_body)?;
    snapshot_domain::validate_read_only_workflow(binding, identity, &workflow, ledger_events)?;
    Ok(workflow)
}

pub(in crate::app::workflow_adapter::state) fn tui_detail_value<'a>(
    details: &'a str,
    key: &str,
) -> Option<&'a str> {
    details.split_ascii_whitespace().find_map(|part| {
        let (candidate, value) = part.split_once('=')?;
        (candidate == key).then_some(value)
    })
}

fn with_validation_gap_writes_suppressed<T>(
    action: impl FnOnce() -> Result<T, AppError>,
) -> Result<T, AppError> {
    SUPPRESS_VALIDATION_GAP_WRITES.with(|flag| {
        let previous = flag.replace(true);
        let result = action();
        flag.set(previous);
        result
    })
}

pub(crate) fn current_state_lease_view_under_transition() -> Result<CurrentStateLeaseView, AppError>
{
    let path = paths::current_state_file();
    let body = fs::read_to_string(&path)
        .map_err(|err| AppError::blocked(format!("current-state lease 읽기 실패: {err}")))?;
    let snapshot = parse_current_state(&body, "current-state lease")?;
    if snapshot.schema_version == 1 {
        promote_current_state_v1()?;
        return current_state_lease_view_under_transition();
    }
    let (events, current_ledger) = {
        let ledger_guard = ledger::LedgerWriterGuard::acquire()?;
        (ledger_guard.events()?, ledger_guard.binding()?)
    };
    if snapshot.ledger_binding != current_ledger {
        snapshot_domain::validate_selection_ledger_suffix(
            &snapshot.ledger_binding,
            &current_ledger,
            &events,
        )?;
    }
    let active_workflow = snapshot
        .active_workflow
        .as_ref()
        .map(|binding| load_workflow_under_transition(&binding.workflow_id))
        .transpose()?;
    snapshot_domain::validate_current_lease(
        &snapshot,
        &snapshot.ledger_binding,
        active_workflow.as_ref(),
    )
}

fn selection_observation_under_transition() -> Result<SelectionObservation, AppError> {
    let identity = ledger::validated_current_identity()?;
    let lease = current_state_lease_view_under_transition()?;
    let body = fs::read_to_string(paths::current_state_file())
        .map_err(|err| AppError::blocked(format!("selection current-state 읽기 실패: {err}")))?;
    let snapshot = parse_current_state(&body, "selection current-state")?;
    snapshot_domain::validate_snapshot_identity(&snapshot, &identity)?;
    let active = snapshot
        .active_workflow
        .as_ref()
        .map(|binding| load_workflow_under_transition(&binding.workflow_id))
        .transpose()?;
    Ok(SelectionObservation {
        project_id: identity.project_id,
        session_id: identity.session_id,
        current_revision: lease.revision,
        current_hash: lease.artifact_hash,
        active_workflow: active.map(|workflow| ObservedWorkflow {
            workflow_id: workflow.workflow_id,
            revision: workflow.revision,
            hash: workflow.artifact_hash,
        }),
    })
}

pub(crate) fn tui_lease_matches_workflow_under_transition(
    lease: &SelectionLease,
    workflow_id: &str,
) -> Result<bool, AppError> {
    let observation = selection_observation_under_transition()?;
    Ok(lease_matches_active_workflow(
        lease,
        workflow_id,
        &observation,
    ))
}

pub(crate) fn tui_lease_matches_terminal_selection_under_transition(
    lease: &SelectionLease,
    workflow_id: &str,
) -> Result<bool, AppError> {
    let observation = selection_observation_under_transition()?;
    Ok(lease_matches_terminal_selection(
        lease,
        workflow_id,
        &observation,
    ))
}
