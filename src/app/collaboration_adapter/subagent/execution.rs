use std::time::Duration;

use crate::adapters::filesystem::{layout as paths, lease};
use crate::app::workflow_adapter::{ledger, state};
use crate::foundation::error::AppError;
use crate::runtime_core::collaboration::subagent::{SubagentRecordV1, SubagentStatus};

use super::{
    append_lifecycle_event, checkpoint_record, display_list, load_record, records_for_parent,
    AdmittedLaunch, AdmittedTeamMember,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkerGeneration {
    pub backend_event_id: String,
    pub effective_max_tokens: u32,
    pub response: String,
}

pub(crate) struct PreparedTeamMember {
    pub lane: u32,
    pub member_id: String,
    prepared: PreparedLaunch,
}

impl PreparedTeamMember {
    pub fn subagent_id(&self) -> &str {
        &self.prepared.running.subagent_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompletedTeamMember {
    pub lane: u32,
    pub member_id: String,
    pub record: SubagentRecordV1,
    pub summary: String,
}

pub(crate) fn terminalize_interrupted_team_members(
    subagent_ids: &[String],
) -> Result<Vec<SubagentRecordV1>, AppError> {
    let mut execution_leases = Vec::new();
    for subagent_id in subagent_ids {
        let current = load_record(subagent_id)?;
        if !current.status.is_terminal() {
            execution_leases.push(lease::RecoverableLease::acquire(
                paths::project_subagent_execution_lock(subagent_id),
                "subagent interrupted recovery",
            )?);
        }
    }
    let Some(first_id) = subagent_ids.first() else {
        return Ok(Vec::new());
    };
    let first = load_record(first_id)?;
    let _parent_lease = lease::RecoverableLease::acquire_with_wait(
        paths::project_subagent_parent_lock(&first.parent_workflow_id),
        "subagent parent admission",
        Duration::from_secs(5),
    )?;
    let mut recovered = Vec::with_capacity(subagent_ids.len());
    for subagent_id in subagent_ids {
        let current = load_record(subagent_id)?;
        let terminal = match current.status {
            SubagentStatus::Requested | SubagentStatus::Admitted => terminalize_locked(
                &current,
                SubagentStatus::Cancelled,
                "team-interrupted-before-send",
                "team.subagent.cancelled",
            )?,
            SubagentStatus::Running => terminalize_locked(
                &current,
                SubagentStatus::Failed,
                "interrupted-no-replay",
                "team.subagent.failed",
            )?,
            _ => current,
        };
        recovered.push(terminal);
    }
    drop(execution_leases);
    Ok(recovered)
}

pub(crate) fn execute_admitted_team_member_with(
    member: AdmittedTeamMember,
    runner: impl FnOnce(&str, u32, u32) -> Result<WorkerGeneration, AppError>,
) -> Result<CompletedTeamMember, AppError> {
    let completed = dispatch_admitted(member.admitted, &member.task, false, runner)?;
    Ok(CompletedTeamMember {
        lane: member.lane,
        member_id: member.member_id,
        record: completed.record,
        summary: completed.summary,
    })
}

pub(crate) fn prepare_team_members(
    members: Vec<AdmittedTeamMember>,
) -> Result<Vec<PreparedTeamMember>, AppError> {
    let subagent_ids = members
        .iter()
        .map(|member| member.subagent_id().to_string())
        .collect::<Vec<_>>();
    let mut prepared_members = Vec::with_capacity(members.len());
    for member in members {
        match prepare_admitted_launch(member.admitted, member.task) {
            Ok(prepared) => prepared_members.push(PreparedTeamMember {
                lane: member.lane,
                member_id: member.member_id,
                prepared,
            }),
            Err(error) => {
                rollback_team_preparation(&subagent_ids)?;
                return Err(error);
            }
        }
    }
    Ok(prepared_members)
}

pub(crate) fn execute_prepared_team_member_with(
    member: PreparedTeamMember,
    runner: impl FnOnce(&str, u32, u32) -> Result<WorkerGeneration, AppError>,
) -> Result<CompletedTeamMember, AppError> {
    let completed = execute_prepared_launch(member.prepared, false, runner)?;
    Ok(CompletedTeamMember {
        lane: member.lane,
        member_id: member.member_id,
        record: completed.record,
        summary: completed.summary,
    })
}

#[derive(Debug)]
pub(super) struct CompletedLaunch {
    pub(super) record: SubagentRecordV1,
    pub(super) context: crate::context::ContextPack,
    pub(super) summary: String,
}

struct PreparedLaunch {
    _execution_lease: lease::RecoverableLease,
    running: SubagentRecordV1,
    context: crate::context::ContextPack,
    task: String,
}

pub(super) fn dispatch_admitted(
    admitted: AdmittedLaunch,
    task: &str,
    merge_parent: bool,
    runner: impl FnOnce(&str, u32, u32) -> Result<WorkerGeneration, AppError>,
) -> Result<CompletedLaunch, AppError> {
    let prepared = prepare_admitted_launch(admitted, task.to_string())?;
    execute_prepared_launch(prepared, merge_parent, runner)
}

fn prepare_admitted_launch(
    admitted: AdmittedLaunch,
    task: String,
) -> Result<PreparedLaunch, AppError> {
    let execution_lease = lease::RecoverableLease::acquire(
        paths::project_subagent_execution_lock(&admitted.record.subagent_id),
        "subagent execution",
    )?;
    let (running, context) = prepare_running(&admitted)?;
    Ok(PreparedLaunch {
        _execution_lease: execution_lease,
        running,
        context,
        task,
    })
}

fn execute_prepared_launch(
    prepared: PreparedLaunch,
    merge_parent: bool,
    runner: impl FnOnce(&str, u32, u32) -> Result<WorkerGeneration, AppError>,
) -> Result<CompletedLaunch, AppError> {
    let PreparedLaunch {
        _execution_lease,
        running,
        context,
        task,
    } = prepared;
    let prompt = render_worker_prompt(&running, &task, &context);
    let generation = match runner(&prompt, running.requested_max_tokens, running.timeout_ms) {
        Ok(generation) => generation,
        Err(error) => {
            let terminal = terminalize_running_error(&running, &error)?;
            return Err(AppError {
                code: error.code,
                message: format!(
                    "{}\n- subagent id: {}\n- subagent status: {}\n- partial output: discarded",
                    error.message,
                    terminal.subagent_id,
                    terminal.status.as_str()
                ),
            });
        }
    };
    complete_generation(running, context, generation, merge_parent)
}

fn rollback_team_preparation(subagent_ids: &[String]) -> Result<(), AppError> {
    let Some(first_id) = subagent_ids.first() else {
        return Ok(());
    };
    let first = load_record(first_id)?;
    let _parent_lease = lease::RecoverableLease::acquire_with_wait(
        paths::project_subagent_parent_lock(&first.parent_workflow_id),
        "subagent parent admission",
        Duration::from_secs(5),
    )?;
    for subagent_id in subagent_ids {
        let current = load_record(subagent_id)?;
        if !current.status.is_terminal() {
            terminalize_locked(
                &current,
                SubagentStatus::Cancelled,
                "team-prepare-rollback",
                "team.subagent.cancelled",
            )?;
        }
    }
    Ok(())
}

pub(super) fn prepare_running(
    admitted: &AdmittedLaunch,
) -> Result<(SubagentRecordV1, crate::context::ContextPack), AppError> {
    let record = &admitted.record;
    let _parent_lease = lease::RecoverableLease::acquire_with_wait(
        paths::project_subagent_parent_lock(&record.parent_workflow_id),
        "subagent parent admission",
        Duration::from_secs(5),
    )?;
    if state::active_workflow_id()?.as_deref() != Some(record.parent_workflow_id.as_str()) {
        return Err(AppError::blocked(
            "subagent dispatch 차단: active parent pointer 변경",
        ));
    }
    let workflow_guard = state::WorkflowCheckpointGuard::acquire(&record.parent_workflow_id)?;
    let parent = workflow_guard.load_current()?;
    if parent.is_terminal()
        || parent.revision != record.parent_revision
        || parent.artifact_hash != record.parent_artifact_hash
        || parent.project_id != record.project_id
        || parent.session_id != record.session_id
    {
        return Err(AppError::blocked(
            "subagent dispatch 차단: exact parent binding 변경",
        ));
    }
    let current = load_record(&record.subagent_id)?;
    if current != *record || current.status != SubagentStatus::Admitted {
        return Err(AppError::blocked(
            "subagent dispatch 차단: admitted state binding 변경",
        ));
    }
    let context =
        crate::context::verify_declared_context_pack(&admitted.context, &current.read_paths)?;
    let mut running = current.clone();
    running.transition_to(SubagentStatus::Running, None)?;
    let running = checkpoint_record(running, current.revision)?;
    append_lifecycle_event(
        &ledger::validated_current_identity()?,
        &running,
        "team.subagent.started",
        "subagent started",
    )?;
    Ok((running, context))
}

fn complete_generation(
    running: SubagentRecordV1,
    context: crate::context::ContextPack,
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
    let context = match crate::context::verify_declared_context_pack(&context, &running.read_paths)
    {
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

fn terminalize_running_error(
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

pub(super) fn terminalize_locked(
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

pub(super) fn merge_completed_result(completed: &SubagentRecordV1) -> Result<(), AppError> {
    crate::app::collaboration_adapter::subagent_result::verify_completed_artifacts(completed)?;
    let parent = state::load_workflow(&completed.parent_workflow_id)?;
    if parent.project_id != completed.project_id || parent.session_id != completed.session_id {
        return Err(AppError::blocked(
            "subagent parent merge owner binding 불일치",
        ));
    }
    let parent_has_evidence = workflow_has_evidence(&parent, &completed.evidence_id);
    let prior_merge = ledger::read_runtime_events()?.into_iter().find(|event| {
        event.event_type == "team.subagent.result-merged"
            && detail_token(&event.details, "subagent_id") == Some(completed.subagent_id.as_str())
    });
    if let Some(prior) = prior_merge {
        if detail_token(&prior.details, "result_hash")
            == Some(completed.result_artifact_hash.as_str())
            && parent_has_evidence
        {
            return Ok(());
        }
        return Err(AppError::blocked(
            "subagent second different result merge 차단",
        ));
    }
    let exact_parent_binding = parent.revision == completed.parent_revision
        && parent.artifact_hash == completed.parent_artifact_hash
        && !parent.is_terminal();
    if !exact_parent_binding && !parent_has_evidence {
        return Err(AppError::blocked(
            "subagent parent merge exact binding 불일치",
        ));
    }
    if exact_parent_binding && !parent_has_evidence {
        let mut evidence = workflow_evidence(&parent);
        evidence.push(completed.evidence_id.clone());
        let mut merged = parent.clone();
        merged.skill_evidence = evidence.join(",");
        state::checkpoint_workflow(merged, parent.revision)?;
    }
    let installed_parent = state::load_workflow(&completed.parent_workflow_id)?;
    if installed_parent.project_id != completed.project_id
        || installed_parent.session_id != completed.session_id
        || !workflow_has_evidence(&installed_parent, &completed.evidence_id)
    {
        return Err(AppError::blocked(
            "subagent parent evidence install binding 불일치",
        ));
    }
    append_lifecycle_event(
        &ledger::validated_current_identity()?,
        completed,
        "team.subagent.result-merged",
        "subagent result merged",
    )?;
    Ok(())
}

fn workflow_evidence(parent: &state::WorkflowRecord) -> Vec<String> {
    parent
        .skill_evidence
        .split(',')
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn workflow_has_evidence(parent: &state::WorkflowRecord, evidence_id: &str) -> bool {
    parent
        .skill_evidence
        .split(',')
        .any(|value| value == evidence_id)
}

pub(super) fn recover_completed_parent_merges(parent_workflow_id: &str) -> Result<(), AppError> {
    for record in records_for_parent(parent_workflow_id)? {
        if record.status == SubagentStatus::Completed {
            merge_completed_result(&record)?;
        }
    }
    Ok(())
}

fn detail_token<'a>(details: &'a str, key: &str) -> Option<&'a str> {
    details
        .split_whitespace()
        .find_map(|token| token.strip_prefix(&format!("{key}=")))
}

fn render_worker_prompt(
    record: &SubagentRecordV1,
    task: &str,
    context: &crate::context::ContextPack,
) -> String {
    format!(
        "You are one bounded {} subagent. Return exactly one canonical compact JSON object and no surrounding text.\n\
         Required key order: schema_version, subagent_id, parent_workflow_id, role, status, summary, findings, patch_proposal, evidence_refs, validation_gaps, suggested_next_action.\n\
         Use schema_version=1, status=completed, subagent_id={}, parent_workflow_id={}, role={}.\n\
         evidence_refs must contain only declared source pointers listed below. patch_proposal must be null unless the executor render_diff capability is declared.\n\
         Never execute commands, apply patches, reveal secrets, or claim unperformed validation.\n\
         Declared tools: {}\nDeclared write ownership: {}\nTask:\n{}\n\n{}",
        record.role.as_str(),
        record.subagent_id,
        record.parent_workflow_id,
        record.role.as_str(),
        record.declared_tools.join(", "),
        display_list(&record.write_paths),
        task,
        context.prompt_section(),
    )
}
