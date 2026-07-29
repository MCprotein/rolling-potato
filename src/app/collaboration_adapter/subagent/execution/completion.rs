use super::*;

pub(super) fn complete_generation(
    running: SubagentRecordV1,
    context: crate::app::context_adapter::ContextPack,
    generation: WorkerGeneration,
    merge_parent: bool,
) -> Result<CompletedLaunch, AppError> {
    let _parent_lease = lease::RecoverableLease::acquire_with_wait(
        paths::project_subagent_parent_lock(&running.parent_workflow_id),
        "subagent parent admission",
        Duration::from_secs(5),
    )?;
    let current = load_record(&running.subagent_id)?;
    if current.status == SubagentStatus::Cancelled {
        return Err(AppError::blocked(format!(
            "subagent completion 폐기\n- subagent id: {}\n- 이유: cancellation이 먼저 terminal state를 획득했습니다.",
            current.subagent_id
        )));
    }
    if current != running || current.status != SubagentStatus::Running {
        return Err(AppError::blocked(
            "subagent completion 차단: running state binding 변경",
        ));
    }
    let parent = state::load_workflow(&running.parent_workflow_id)?;
    if parent.is_terminal()
        || parent.revision != running.parent_revision
        || parent.artifact_hash != running.parent_artifact_hash
        || parent.project_id != running.project_id
        || parent.session_id != running.session_id
    {
        let blocked = terminalize_locked(
            &current,
            SubagentStatus::Blocked,
            "stale-parent",
            "team.subagent.blocked",
        )?;
        return Err(AppError::blocked(format!(
            "subagent result merge 차단: stale parent\n- subagent id: {}\n- status: {}",
            blocked.subagent_id,
            blocked.status.as_str()
        )));
    }
    let context = match crate::app::context_adapter::verify_declared_context_pack(
        &context,
        &running.read_paths,
    ) {
        Ok(context) => context,
        Err(error) => {
            terminalize_locked(
                &current,
                SubagentStatus::Blocked,
                "stale-context",
                "team.subagent.blocked",
            )?;
            return Err(error);
        }
    };
    if generation.effective_max_tokens == 0
        || generation.effective_max_tokens > running.requested_max_tokens
        || generation.backend_event_id.is_empty()
    {
        terminalize_locked(
            &current,
            SubagentStatus::Blocked,
            "invalid-backend-metadata",
            "team.subagent.blocked",
        )?;
        return Err(AppError::blocked("subagent backend metadata binding 오류"));
    }
    let stored = match crate::app::collaboration_adapter::subagent_result::parse_and_store(
        &running,
        &context,
        &generation.response,
    ) {
        Ok(stored) => stored,
        Err(error) => {
            terminalize_locked(
                &current,
                SubagentStatus::Blocked,
                "invalid-result",
                "team.subagent.blocked",
            )?;
            return Err(AppError::blocked(format!(
                "subagent result 검증 차단\n- subagent id: {}\n- reason: {}",
                running.subagent_id, error.message
            )));
        }
    };
    crate::app::collaboration_adapter::subagent_result::verify_stored_artifacts(&running, &stored)?;
    let mut completed = current.clone();
    completed.backend_event_id = generation.backend_event_id;
    completed.effective_max_tokens = generation.effective_max_tokens;
    completed.result_artifact_id = stored.result_artifact_id.clone();
    completed.result_artifact_hash = stored.result_artifact_hash.clone();
    completed.evidence_id = stored.evidence_id.clone();
    completed.evidence_hash = stored.evidence_hash.clone();
    completed.transition_to(SubagentStatus::Completed, None)?;
    let completed = checkpoint_record(completed, current.revision)?;
    let identity = ledger::validated_current_identity()?;
    append_lifecycle_event(
        &identity,
        &completed,
        "team.subagent.completed",
        "subagent completed",
    )?;
    if merge_parent {
        merge_completed_result(&completed)?;
    }
    Ok(CompletedLaunch {
        record: completed,
        context,
        summary: stored.result.summary,
    })
}

pub(super) fn terminalize_running_error(
    running: &SubagentRecordV1,
    error: &AppError,
) -> Result<SubagentRecordV1, AppError> {
    let _parent_lease = lease::RecoverableLease::acquire_with_wait(
        paths::project_subagent_parent_lock(&running.parent_workflow_id),
        "subagent parent admission",
        Duration::from_secs(5),
    )?;
    let current = load_record(&running.subagent_id)?;
    if current.status.is_terminal() {
        return Ok(current);
    }
    if current != *running || current.status != SubagentStatus::Running {
        return Err(AppError::blocked(
            "subagent backend failure state binding 변경",
        ));
    }
    let (status, failure_code, event_type) = classify_backend_error(error);
    terminalize_locked(&current, status, failure_code, event_type)
}

pub(in crate::app::collaboration_adapter::subagent) fn terminalize_locked(
    current: &SubagentRecordV1,
    status: SubagentStatus,
    failure_code: &str,
    event_type: &str,
) -> Result<SubagentRecordV1, AppError> {
    let mut terminal = current.clone();
    terminal.transition_to(status, Some(failure_code))?;
    let terminal = checkpoint_record(terminal, current.revision)?;
    append_lifecycle_event(
        &ledger::validated_current_identity()?,
        &terminal,
        event_type,
        "subagent terminal failure",
    )?;
    Ok(terminal)
}

fn classify_backend_error(error: &AppError) -> (SubagentStatus, &'static str, &'static str) {
    if error.message.contains("제한 시간 초과") || error.message.contains("timed-out") {
        (
            SubagentStatus::TimedOut,
            "backend-timeout",
            "team.subagent.timed-out",
        )
    } else if error.message.contains("취소됨") || error.message.contains("cancelled") {
        (
            SubagentStatus::Cancelled,
            "backend-cancelled",
            "team.subagent.cancelled",
        )
    } else if error.code == 3 {
        (
            SubagentStatus::Blocked,
            "backend-blocked",
            "team.subagent.blocked",
        )
    } else {
        (
            SubagentStatus::Failed,
            "backend-failed",
            "team.subagent.failed",
        )
    }
}
