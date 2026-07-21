use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateInit {
    pub identity: RuntimeIdentity,
    pub created_paths: Vec<PathBuf>,
    pub store: StoreStatus,
}

pub fn initialize() -> Result<StateInit, AppError> {
    let identity = ledger::validated_current_identity()?;
    let created_paths = ensure_layout()?;
    ensure_runtime_evidence_file()?;
    if !paths::current_state_file().exists() {
        let event = ledger::new_event_for(
            &identity,
            "runtime.init",
            "runtime state 초기화",
            "app/project state layout 생성 또는 확인",
        );
        let intent_id = internal_transition_intent_id(&event);
        commit_state_event(
            &intent_id,
            transition::CurrentStateIntent::Bootstrap,
            &identity,
            &event,
            None,
            None,
            CompactionBoundaryUpdate::Preserve,
            None,
        )?;
    }

    let store = observability::initialize(&identity)?;

    Ok(StateInit {
        identity,
        created_paths,
        store,
    })
}

pub fn status_report() -> Result<String, AppError> {
    let active = active_workflow_id()?.unwrap_or_else(|| "없음".to_string());
    let current_state = read_current_state_summary()?;
    let store = observability::status()?;
    let recovered = store
        .recovered_from
        .as_ref()
        .map(|path| format!("\n- recovered corrupt db: {}", path.display()))
        .unwrap_or_default();

    Ok(format!(
        "state 상태\n- app state dir: {}\n- project state dir: {}\n- runtime ledger: {}\n- project session ledger: {}\n- current state: {}\n- observability db: {}\n- schema migration: v{}\n- ledger events: {}\n- sessions: {}\n- workflows: {}\n- transcript records: {}\n- active workflow: {}\n- transcript parent/branch pointer: current-state schema에 null로 보존\n- evidence stale policy: {}{}",
        paths::state_dir().display(),
        paths::project_state_dir().display(),
        paths::runtime_ledger_file().display(),
        paths::project_session_ledger_file().display(),
        current_state,
        store.path.display(),
        store.migration_version,
        store.ledger_events,
        store.sessions,
        store.workflows,
        store.transcript_records,
        active,
        crate::app::evidence_adapter::stale_policy_summary(),
        recovered
    ))
}

pub fn reconcile_report() -> Result<String, AppError> {
    ensure_layout()?;
    let identity = match ledger::validated_current_identity() {
        Ok(identity) => identity,
        Err(_) => ledger::fresh_identity(),
    };
    let transition_guard = transition::TransitionGuard::acquire_for(
        &identity.project_id,
        transition::CurrentStateIntent::Reconcile,
    )?;
    let status = current_state_status(&identity)?;
    let (outcome, event_id) = match status {
        CurrentStateStatus::CleanNoActiveWorkflow | CurrentStateStatus::CleanActiveWorkflow => {
            (ReconcileOutcome::Clean, "없음".to_string())
        }
        CurrentStateStatus::Missing => {
            let event = ledger::new_event_for(
                &identity,
                "state.reconcile.created",
                "current-state 생성",
                "current-state reconcile 완료",
            );
            let intent_id = internal_transition_intent_id(&event);
            transition_project_current_state_under_guard(
                &transition_guard,
                StateTransitionRequest {
                    intent_id: &intent_id,
                    intent: transition::CurrentStateIntent::Reconcile,
                    identity: &identity,
                    event: &event,
                    resume_source: Some("state-reconcile"),
                    active_workflow: None,
                    previous: None,
                    compaction_boundary: CompactionBoundaryUpdate::Preserve,
                    workflow: None,
                },
            )?;
            (ReconcileOutcome::Created, event.event_id)
        }
        CurrentStateStatus::Corrupt | CurrentStateStatus::StaleProject => {
            let before = fs::read_to_string(paths::current_state_file()).map_err(|err| {
                AppError::blocked(format!(
                    "reconcile preserved current-state 읽기 실패: {err}"
                ))
            })?;
            let reason = if status == CurrentStateStatus::Corrupt {
                "corrupt"
            } else {
                "stale"
            };
            let (event, backup) = reconcile_invalid_current_under_guard(
                &transition_guard,
                &identity,
                reason,
                &before,
            )?;
            let outcome = if reason == "corrupt" {
                ReconcileOutcome::RecoveredCorrupt(backup)
            } else {
                ReconcileOutcome::RecoveredStale(backup)
            };
            (outcome, event.event_id)
        }
    };
    let summary = outcome.summary();
    observability::initialize(&identity)?;

    Ok(format!(
        "state reconcile 결과\n- outcome: {}\n- current state: {}\n- ledger event: {}\n- 동작: stale/corrupt current-state를 발견하면 기존 파일을 보존 이동하고 새 current-state를 기록합니다.",
        summary,
        paths::current_state_file().display(),
        event_id
    ))
}

pub fn resume_report() -> Result<String, AppError> {
    ensure_layout()?;
    if let Some(workflow_id) = active_workflow_id()? {
        return crate::app::patch_adapter::resume_workflow_report(&workflow_id);
    }
    let identity = ledger::validated_current_identity()?;
    observability::initialize(&identity)?;
    let status = current_state_status(&identity)?;
    let (event_type, summary, action) = match status {
        CurrentStateStatus::CleanNoActiveWorkflow => (
            "workflow.resume.noop",
            "active workflow 없는 resume 요청",
            "재개할 workflow가 없어 no-op event만 기록했습니다.",
        ),
        CurrentStateStatus::CleanActiveWorkflow => (
            "workflow.resume.detected",
            "resume 대상 감지",
            "active workflow pointer를 발견했습니다. agent loop resume은 후속 phase에서 실행됩니다.",
        ),
        CurrentStateStatus::Missing => (
            "workflow.resume.blocked",
            "current-state 누락으로 resume 차단",
            "current-state가 없어 먼저 state reconcile이 필요합니다.",
        ),
        CurrentStateStatus::Corrupt => (
            "workflow.resume.blocked",
            "current-state 손상으로 resume 차단",
            "current-state가 손상되어 먼저 state reconcile이 필요합니다.",
        ),
        CurrentStateStatus::StaleProject => (
            "workflow.resume.blocked",
            "다른 project current-state로 resume 차단",
            "current-state project id가 현재 project와 달라 먼저 state reconcile이 필요합니다.",
        ),
    };

    let event = ledger::new_event_for(&identity, event_type, summary, action);
    let intent_id = internal_transition_intent_id(&event);
    commit_state_event(
        &intent_id,
        transition::CurrentStateIntent::Resume,
        &identity,
        &event,
        None,
        None,
        CompactionBoundaryUpdate::Preserve,
        None,
    )?;

    Ok(format!(
        "state resume 결과\n- outcome: {}\n- ledger event: {}\n- 동작: {}",
        summary, event.event_id, action
    ))
}

pub fn cancel_report() -> Result<String, AppError> {
    ensure_layout()?;
    if let Some(workflow_id) = active_workflow_id()? {
        return crate::app::patch_adapter::cancel_workflow_report(&workflow_id);
    }
    let identity = ledger::validated_current_identity()?;
    observability::initialize(&identity)?;
    let event = ledger::new_event_for(
        &identity,
        "workflow.cancel.noop",
        "active workflow 없는 cancel 요청",
        "active_workflow=null",
    );
    let intent_id = internal_transition_intent_id(&event);
    commit_state_event(
        &intent_id,
        transition::CurrentStateIntent::Cancel,
        &identity,
        &event,
        None,
        None,
        CompactionBoundaryUpdate::Preserve,
        None,
    )?;

    Ok(format!(
        "cancel 결과\n- active workflow: 없음\n- ledger event: {}\n- ledger: {}\n- 동작: 취소할 실행이 없어 no-op event만 기록했습니다.",
        event.event_id,
        paths::runtime_ledger_file().display()
    ))
}

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

pub(super) fn session_new_report_for_intent(intent_id: &str) -> Result<String, AppError> {
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

pub fn record_event(event_type: &str, summary: &str, details: &str) -> Result<String, AppError> {
    ensure_layout()?;
    if !paths::current_state_file().exists() {
        initialize()?;
    }
    let identity = ledger::validated_current_identity()?;
    let event = ledger::new_event_for(&identity, event_type, summary, details);
    let event_id = event.event_id.clone();
    let active_workflow = read_valid_current_for_transition()?
        .and_then(|snapshot| snapshot.active_workflow)
        .map(|binding| binding.workflow_id);
    let intent_id = internal_transition_intent_id(&event);
    commit_state_event(
        &intent_id,
        transition::CurrentStateIntent::RecordEvent,
        &identity,
        &event,
        None,
        active_workflow.as_deref(),
        CompactionBoundaryUpdate::Preserve,
        None,
    )?;
    Ok(event_id)
}

pub(crate) fn current_compaction_boundary(session_id: &str) -> Result<Option<String>, AppError> {
    let identity = ledger::validated_current_identity()?;
    let Some(snapshot) = read_valid_current_for_transition()? else {
        return Ok(None);
    };
    if snapshot.project_id != identity.project_id || snapshot.session_id != session_id {
        return Err(AppError::blocked(
            "compaction boundary current project/session binding 불일치",
        ));
    }
    Ok(snapshot.compaction_boundary)
}

pub(crate) fn record_compaction_boundary(
    artifact_path: &str,
    artifact_hash: &str,
    boundary_record_id: &str,
    expected_previous_artifact_path: Option<String>,
) -> Result<String, AppError> {
    ensure_layout()?;
    if !paths::current_state_file().exists() {
        initialize()?;
    }
    let identity = ledger::validated_current_identity()?;
    let expected_prefix = format!(
        "state/compactions/{}/{}/",
        identity.project_id, identity.session_id
    );
    if !artifact_path.starts_with(&expected_prefix)
        || !artifact_path.ends_with(".json")
        || artifact_path
            .split('/')
            .any(|part| part.is_empty() || part == "..")
        || artifact_path.bytes().any(|byte| byte.is_ascii_whitespace())
        || artifact_hash.len() != 64
        || !artifact_hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || boundary_record_id.is_empty()
        || boundary_record_id
            .bytes()
            .any(|byte| byte.is_ascii_whitespace())
    {
        return Err(AppError::blocked(
            "compaction boundary artifact path/hash/record binding 불일치",
        ));
    }
    let event = ledger::new_event_for(
        &identity,
        "context.compacted",
        "context compaction checkpoint committed",
        &format!(
            "artifact_path={artifact_path} artifact_hash={artifact_hash} boundary_record_id={boundary_record_id}"
        ),
    );
    let event_id = event.event_id.clone();
    let active_workflow = read_valid_current_for_transition()?
        .and_then(|snapshot| snapshot.active_workflow)
        .map(|binding| binding.workflow_id);
    let intent_id = internal_transition_intent_id(&event);
    commit_state_event(
        &intent_id,
        transition::CurrentStateIntent::RecordEvent,
        &identity,
        &event,
        Some("context-compaction"),
        active_workflow.as_deref(),
        CompactionBoundaryUpdate::Set(artifact_path),
        Some(expected_previous_artifact_path.as_deref()),
    )?;
    Ok(event_id)
}

pub fn workflow_ownership_summary() -> &'static str {
    "active workflow는 current-state가 소유하고 skill/plugin/TUI는 parent workflow pointer를 받아야 합니다."
}
