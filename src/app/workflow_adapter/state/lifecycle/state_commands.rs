use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateInit {
    pub identity: RuntimeIdentity,
    pub created_paths: Vec<PathBuf>,
    pub store: StoreStatus,
}

pub fn initialize() -> Result<StateInit, AppError> {
    let created_paths = ensure_layout()?;
    migrate_matching_legacy_current_state()?;
    let identity = ledger::validated_current_identity()?;
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
            CompactionBoundaryCommit::preserve(),
        )?;
    } else {
        synchronize_current_state_ledger(&identity)?;
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
        CompactionBoundaryCommit::preserve(),
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
        CompactionBoundaryCommit::preserve(),
    )?;

    Ok(format!(
        "cancel 결과\n- active workflow: 없음\n- ledger event: {}\n- ledger: {}\n- 동작: 취소할 실행이 없어 no-op event만 기록했습니다.",
        event.event_id,
        paths::runtime_ledger_file().display()
    ))
}

pub fn workflow_ownership_summary() -> &'static str {
    "active workflow는 current-state가 소유하고 skill/plugin/TUI는 parent workflow pointer를 받아야 합니다."
}
