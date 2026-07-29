use super::*;

pub fn session_list_report() -> Result<String, AppError> {
    let identity = ledger::validated_current_identity()?;
    ensure_layout()?;
    let sessions = observability::session_history(20)?;
    if sessions.is_empty() {
        return Ok(format!(
            "session history\n- project: {}\n- sessions: 없음\n- 다음 단계: `rpotato init` 또는 `rpotato session new`로 세션을 시작하세요.",
            identity.project_root
        ));
    }

    let rows = sessions
        .iter()
        .map(format_session_row)
        .collect::<Vec<_>>()
        .join("\n");

    Ok(format!(
        "session history\n- project: {}\n- current session: {}\n- resume: `rpotato session resume <session-id>` 또는 `rpotato resume <session-id>`\n{}",
        identity.project_root, identity.session_id, rows
    ))
}

pub fn session_new_report() -> Result<String, AppError> {
    session_new_report_for_intent(&new_tui_intent_id())
}

pub(crate) fn session_new_report_for_intent(intent_id: &str) -> Result<String, AppError> {
    if !intent_id.starts_with("intent-")
        || !intent_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
    {
        return Err(AppError::blocked("session new intent id 형식 불일치"));
    }
    ensure_layout()?;
    let current_identity = ledger::validated_current_identity()?;
    let observed = read_valid_current_for_transition()?;
    ensure_runtime_evidence_file()?;
    let transition_guard = transition::TransitionGuard::acquire_for(
        &current_identity.project_id,
        transition::CurrentStateIntent::StartSession,
    )?;
    if let Some(existing) = ledger::read_runtime_events()?.into_iter().find(|event| {
        event.event_type == "session.new"
            && tui_detail_value(&event.details, "intent_id") == Some(intent_id)
    }) {
        return Ok(session_new_success_report(
            &existing.session_id,
            &existing.event_id,
        ));
    }
    let previous = read_valid_current_for_transition()?;
    let same_predecessor = match (&observed, &previous) {
        (None, None) => true,
        (Some(observed), Some(previous)) => {
            previous.revision == observed.revision
                && previous.artifact_hash == observed.artifact_hash
                && previous.session_id == observed.session_id
        }
        _ => false,
    };
    if !same_predecessor {
        return Err(AppError::blocked(
            "session new stale predecessor 차단: current-state가 선택 이후 변경되었습니다.",
        ));
    }
    let identity = RuntimeIdentity {
        project_id: current_identity.project_id,
        session_id: format!(
            "session-{}",
            &sha256_text(&format!("rpotato.session-new/v1\0{intent_id}"))[..24]
        ),
        project_root: current_identity.project_root,
    };
    let event = ledger::new_event_for(
        &identity,
        "session.new",
        "새 session 시작",
        &format!(
            "intent_id={intent_id} predecessor_revision={} predecessor_hash={}",
            previous.as_ref().map_or(0, |snapshot| snapshot.revision),
            previous
                .as_ref()
                .map_or("missing", |snapshot| snapshot.artifact_hash.as_str())
        ),
    );
    transition_project_current_state_under_guard(
        &transition_guard,
        StateTransitionRequest {
            intent_id,
            intent: transition::CurrentStateIntent::StartSession,
            identity: &identity,
            event: &event,
            resume_source: None,
            active_workflow: None,
            previous: previous.as_ref(),
            compaction_boundary: CompactionBoundaryUpdate::Preserve,
            workflow: None,
        },
    )?;
    observability::initialize(&identity)?;

    Ok(session_new_success_report(
        &identity.session_id,
        &event.event_id,
    ))
}

fn session_new_success_report(session_id: &str, event_id: &str) -> String {
    format!(
        "session new 결과\n- session id: {}\n- current state: {}\n- ledger event: {}\n- 동작: 이후 명령은 이 session id로 ledger와 SQLite projection에 이어 기록됩니다.",
        session_id,
        paths::current_state_file().display(),
        event_id
    )
}

pub fn session_resume_preflight(session_id: &str) -> Result<Option<String>, AppError> {
    ensure_layout()?;
    let identity = ledger::validated_current_identity()?;
    let _transition_guard = transition::TransitionGuard::acquire_for(
        &identity.project_id,
        transition::CurrentStateIntent::SelectSession,
    )?;
    session_resume_preflight_under_transition(session_id, &identity)
}

fn session_resume_preflight_under_transition(
    session_id: &str,
    identity: &RuntimeIdentity,
) -> Result<Option<String>, AppError> {
    let canonical_session = ledger::read_runtime_events()?
        .into_iter()
        .any(|event| event.project_id == identity.project_id && event.session_id == session_id);
    if !canonical_session {
        return snapshot_domain::validate_session_resume_target(session_id, false, false, None);
    }
    let projected_session = observability::session_entry(session_id)?.is_some();
    if !projected_session {
        return snapshot_domain::validate_session_resume_target(session_id, true, false, None);
    }
    let active_workflow = discover_active_workflow()?
        .map(|workflow_id| load_workflow_under_transition(&workflow_id))
        .transpose()?;
    snapshot_domain::validate_session_resume_target(
        session_id,
        canonical_session,
        projected_session,
        active_workflow.as_ref(),
    )
}

pub fn session_resume_report(session_id: &str) -> Result<String, AppError> {
    session_resume_report_with_precondition(session_id, None, None)?
        .ok_or_else(|| AppError::blocked("internal session resume precondition unexpectedly stale"))
}

pub(crate) fn session_resume_report_for_tui(
    session_id: &str,
    intent_id: &str,
    lease: &SelectionLease,
) -> Result<Option<String>, AppError> {
    session_resume_report_with_precondition(session_id, Some(intent_id), Some(lease))
}

fn session_resume_report_with_precondition(
    session_id: &str,
    supplied_intent_id: Option<&str>,
    lease: Option<&SelectionLease>,
) -> Result<Option<String>, AppError> {
    let project_id = match lease {
        Some(lease) => lease.project_id.clone(),
        None => ledger::validated_current_identity()?.project_id,
    };
    let transition_guard = transition::TransitionGuard::acquire_for(
        &project_id,
        transition::CurrentStateIntent::SelectSession,
    )?;
    let identity = ledger::validated_current_identity()?;
    if let Some(intent_id) = supplied_intent_id {
        if let Some(event_id) = existing_session_selection_receipt(intent_id, session_id)? {
            let session = observability::session_entry(session_id)?
                .ok_or_else(|| AppError::blocked("committed session selection projection 누락"))?;
            return Ok(Some(render_session_resume_report(&session, &event_id)));
        }
    }
    if let Some(lease) = lease {
        if !selection_lease_matches_under_transition(session_id, lease, &identity)? {
            return Ok(None);
        }
    }
    session_resume_preflight_under_transition(session_id, &identity)?;
    let Some(session) = observability::session_entry(session_id)? else {
        return Err(AppError::blocked(format!(
            "session resume 차단\n- session id: {}\n- 이유: session projection을 찾지 못했습니다.",
            session_id
        )));
    };
    let active_workflow = discover_active_workflow()?
        .map(|workflow_id| load_workflow_under_transition(&workflow_id))
        .transpose()?;

    let resumed = RuntimeIdentity {
        project_id: identity.project_id,
        session_id: session.session_id.clone(),
        project_root: identity.project_root,
    };
    let event = ledger::new_event_for(
        &resumed,
        "session.resume.selected",
        "session history에서 resume target 선택",
        &format!(
            "selected_session_id={} intent_id={}",
            session.session_id,
            supplied_intent_id.unwrap_or("internal")
        ),
    );
    let intent_id = supplied_intent_id
        .map(str::to_string)
        .unwrap_or_else(|| internal_transition_intent_id(&event));
    let previous = read_valid_current_for_transition()?
        .ok_or_else(|| AppError::blocked("session resume current-state 누락"))?;
    transition_project_current_state_under_guard(
        &transition_guard,
        StateTransitionRequest {
            intent_id: &intent_id,
            intent: transition::CurrentStateIntent::SelectSession,
            identity: &resumed,
            event: &event,
            resume_source: Some("session-history"),
            active_workflow: active_workflow.as_ref(),
            previous: Some(&previous),
            compaction_boundary: CompactionBoundaryUpdate::Preserve,
            workflow: None,
        },
    )?;
    let committed_session = observability::session_entry(session_id)?
        .ok_or_else(|| AppError::blocked("committed session selection projection 누락"))?;

    Ok(Some(render_session_resume_report(
        &committed_session,
        &event.event_id,
    )))
}

fn existing_session_selection_receipt(
    intent_id: &str,
    session_id: &str,
) -> Result<Option<String>, AppError> {
    let intent_marker = format!("intent_id={intent_id}");
    let selected_marker = format!("selected_session_id={session_id}");
    let mut matching_intent = None;
    for event in ledger::read_runtime_events()?
        .into_iter()
        .filter(|event| event.event_type == "session.resume.selected")
    {
        let fields = event.details.split_ascii_whitespace().collect::<Vec<_>>();
        if fields.contains(&intent_marker.as_str()) {
            if !fields.contains(&selected_marker.as_str()) || matching_intent.is_some() {
                return Err(AppError::blocked(
                    "session selection intent receipt binding 충돌",
                ));
            }
            matching_intent = Some(event.event_id);
        }
    }
    Ok(matching_intent)
}

fn selection_lease_matches_under_transition(
    session_id: &str,
    lease: &SelectionLease,
    identity: &RuntimeIdentity,
) -> Result<bool, AppError> {
    let Some(current) = read_valid_current_for_transition()? else {
        return Ok(false);
    };
    if lease.project_id != identity.project_id
        || lease.project_id != current.project_id
        || lease.session_id != current.session_id
        || lease.active_session_id != current.session_id
        || lease.selected_object_id != session_id
        || lease.current_revision != current.revision
        || lease.current_hash != current.artifact_hash
    {
        return Ok(false);
    }
    let observed = current
        .active_workflow
        .as_ref()
        .map(|binding| ObservedWorkflow {
            workflow_id: binding.workflow_id.clone(),
            revision: binding.revision,
            hash: binding.artifact_hash.clone(),
        });
    if observed != lease.active_workflow {
        return Ok(false);
    }
    if let Some(binding) = current.active_workflow {
        let workflow = load_workflow_under_transition(&binding.workflow_id)?;
        if workflow.revision != binding.revision || workflow.artifact_hash != binding.artifact_hash
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn render_session_resume_report(session: &SessionHistoryEntry, event_id: &str) -> String {
    format!(
        "session resume 결과\n- selected session: {}\n- events: {}\n- last event: {}\n- current state: {}\n- ledger event: {}\n- 동작: 선택한 session id를 기록했습니다. Runtime wrapper는 검증된 같은-session workflow checkpoint만 계속하며 새 model turn은 자동 생성하지 않습니다.",
        session.session_id,
        session.event_count,
        session
            .last_summary
            .clone()
            .unwrap_or_else(|| "없음".to_string()),
        paths::current_state_file().display(),
        event_id
    )
}
