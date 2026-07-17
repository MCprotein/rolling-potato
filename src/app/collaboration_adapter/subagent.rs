use crate::app::inference_adapter::backend;
use crate::app::workflow_adapter::ledger;
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
pub(crate) use crate::runtime_core::collaboration::subagent::*;
use crate::{adapters::filesystem::layout as paths, adapters::filesystem::lease};
#[cfg(test)]
use std::fs;

mod execution;
mod persistence;

use execution::{dispatch_admitted, recover_completed_parent_merges, terminalize_locked};
pub(crate) use execution::{
    execute_admitted_team_member_with, execute_prepared_team_member_with, prepare_team_members,
    terminalize_interrupted_team_members, CompletedTeamMember, WorkerGeneration,
};
#[cfg(test)]
use execution::{merge_completed_result, prepare_running};

use persistence::latest_active_parent_record;
pub(crate) use persistence::records_for_parent;
pub use persistence::{checkpoint_record, create_record, load_record};

#[derive(Debug)]
pub(crate) struct AdmittedLaunch {
    record: SubagentRecordV1,
    context: crate::app::context_adapter::ContextPack,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TeamMemberLaunch {
    pub lane: u32,
    pub member_id: String,
    pub role: String,
    pub task: String,
    pub declared_tools: Vec<String>,
    pub read_paths: Vec<String>,
    pub write_paths: Vec<String>,
    pub timeout_ms: u32,
    pub max_tokens: u32,
}

#[derive(Debug)]
pub(crate) struct AdmittedTeamMember {
    pub lane: u32,
    pub member_id: String,
    task: String,
    admitted: AdmittedLaunch,
}

impl AdmittedTeamMember {
    pub fn subagent_id(&self) -> &str {
        &self.admitted.record.subagent_id
    }
}

pub fn launch_report(
    role: &str,
    task: &str,
    declared_tools: &[String],
    read_paths: &[String],
    write_paths: &[String],
    timeout_ms: Option<u32>,
    max_tokens: Option<u32>,
) -> Result<String, AppError> {
    let launch = validate_launch(
        role,
        task,
        declared_tools,
        read_paths,
        write_paths,
        timeout_ms,
        max_tokens,
    )?;
    backend::preflight_chat_ready()?;
    let admitted = admit_launch(launch)?;
    let completed = dispatch_admitted(admitted, task, true, |prompt, max_tokens, timeout_ms| {
        let run = backend::chat_once_bounded(prompt, max_tokens, timeout_ms)?;
        Ok(WorkerGeneration {
            backend_event_id: run.ledger_event,
            effective_max_tokens: run.effective_max_tokens,
            response: run.response,
        })
    })?;
    let record = &completed.record;
    Ok(format!(
        "subagent launch\n- status: {}\n- subagent id: {}\n- parent workflow: {}\n- parent revision: {}\n- role: {}\n- task hash: {}\n- tools: {}\n- read paths: {}\n- write paths: {}\n- timeout ms: {}\n- requested max tokens: {}\n- effective max tokens: {}\n- context origin: {}\n- context files: {}\n- context chars: {}\n- source pointers: {}\n- result artifact: {}\n- evidence: {}\n- summary: {}\n- boundary: child output was strictly parsed and recorded; no command or patch was executed.",
        record.status.as_str(),
        record.subagent_id,
        record.parent_workflow_id,
        record.parent_revision,
        record.role.as_str(),
        record.task_hash,
        record.declared_tools.join(", "),
        record.read_paths.join(", "),
        display_list(&record.write_paths),
        record.timeout_ms,
        record.requested_max_tokens,
        record.effective_max_tokens,
        completed.context.origin,
        completed.context.files_read,
        completed.context.chars_read,
        completed.context.pointer_summary(),
        record.result_artifact_id,
        record.evidence_id,
        completed.summary,
    ))
}

pub(crate) fn admit_team_members(
    parent_workflow_id: &str,
    parent_revision: u64,
    parent_artifact_hash: &str,
    members: Vec<TeamMemberLaunch>,
) -> Result<Vec<AdmittedTeamMember>, AppError> {
    if members.is_empty() {
        return Err(AppError::blocked("team execution member가 없습니다."));
    }
    let mut prepared = Vec::with_capacity(members.len());
    for member in members {
        let launch = validate_launch(
            &member.role,
            &member.task,
            &member.declared_tools,
            &member.read_paths,
            &member.write_paths,
            Some(member.timeout_ms),
            Some(member.max_tokens),
        )?;
        let context = crate::app::context_adapter::build_declared_context_pack(&launch.read_paths)?;
        prepared.push((member, launch, context));
    }

    recover_completed_parent_merges(parent_workflow_id)?;
    let identity = ledger::validated_current_identity()?;
    let _parent_lease = lease::RecoverableLease::acquire(
        paths::project_subagent_parent_lock(parent_workflow_id),
        "team member admission",
    )?;
    if state::active_workflow_id()?.as_deref() != Some(parent_workflow_id) {
        return Err(AppError::blocked(
            "team member admission 차단: active parent pointer 변경",
        ));
    }
    let workflow_guard = state::WorkflowCheckpointGuard::acquire(parent_workflow_id)?;
    let parent = workflow_guard.load_current()?;
    if parent.is_terminal()
        || parent.project_id != identity.project_id
        || parent.session_id != identity.session_id
        || parent.revision != parent_revision
        || parent.artifact_hash != parent_artifact_hash
    {
        return Err(AppError::blocked(
            "team member admission 차단: exact parent binding 변경",
        ));
    }
    if let Some(existing) = records_for_parent(parent_workflow_id)?
        .into_iter()
        .find(|record| !record.status.is_terminal())
    {
        return Err(AppError::blocked(format!(
            "team member admission 차단: 기존 non-terminal child가 있습니다.\n- subagent id: {}\n- status: {}",
            existing.subagent_id,
            existing.status.as_str()
        )));
    }

    let mut admitted_members = Vec::with_capacity(prepared.len());
    for (member, launch, context) in prepared {
        let result = (|| {
            let requested = create_record(SubagentRecordV1::new(
                &parent.project_id,
                &parent.session_id,
                &parent.workflow_id,
                parent.revision,
                &parent.artifact_hash,
                launch,
            )?)?;
            append_lifecycle_event(
                &identity,
                &requested,
                "team.subagent.requested",
                "team member requested",
            )?;
            let mut admitted = requested.clone();
            admitted.transition_to(SubagentStatus::Admitted, None)?;
            let admitted = checkpoint_record(admitted, requested.revision)?;
            append_lifecycle_event(
                &identity,
                &admitted,
                "team.subagent.admitted",
                "team member admitted",
            )?;
            Ok(AdmittedTeamMember {
                lane: member.lane,
                member_id: member.member_id,
                task: member.task,
                admitted: AdmittedLaunch {
                    record: admitted,
                    context,
                },
            })
        })();
        match result {
            Ok(admitted) => admitted_members.push(admitted),
            Err(error) => {
                for admitted in &admitted_members {
                    let current = load_record(&admitted.admitted.record.subagent_id)?;
                    if !current.status.is_terminal() {
                        terminalize_locked(
                            &current,
                            SubagentStatus::Cancelled,
                            "team-admission-rollback",
                            "team.subagent.cancelled",
                        )?;
                    }
                }
                return Err(error);
            }
        }
    }
    Ok(admitted_members)
}

pub(crate) fn resume_admitted_team_member(
    member: TeamMemberLaunch,
    subagent_id: &str,
) -> Result<AdmittedTeamMember, AppError> {
    let launch = validate_launch(
        &member.role,
        &member.task,
        &member.declared_tools,
        &member.read_paths,
        &member.write_paths,
        Some(member.timeout_ms),
        Some(member.max_tokens),
    )?;
    let record = load_record(subagent_id)?;
    if record.status != SubagentStatus::Admitted
        || record.role != launch.role
        || record.task_hash != launch.task_hash
        || record.declared_tools != launch.declared_tools
        || record.read_paths != launch.read_paths
        || record.write_paths != launch.write_paths
        || record.timeout_ms != launch.timeout_ms
        || record.requested_max_tokens != launch.requested_max_tokens
    {
        return Err(AppError::blocked(
            "team admitted recovery immutable launch binding 불일치",
        ));
    }
    let context = crate::app::context_adapter::build_declared_context_pack(&record.read_paths)?;
    Ok(AdmittedTeamMember {
        lane: member.lane,
        member_id: member.member_id,
        task: member.task,
        admitted: AdmittedLaunch { record, context },
    })
}

pub fn status_report(subagent_id: Option<&str>) -> Result<String, AppError> {
    let record = match subagent_id {
        Some(subagent_id) => load_record(subagent_id)?,
        None => latest_active_parent_record()?,
    };
    Ok(render_status_report(&record, "read-only"))
}

pub fn cancel_report(subagent_id: &str) -> Result<String, AppError> {
    let initial = load_record(subagent_id)?;
    let identity = ledger::validated_current_identity()?;
    if initial.project_id != identity.project_id || initial.session_id != identity.session_id {
        return Err(AppError::blocked(
            "subagent cancel owner binding 불일치\n- 동작: 다른 project/session child를 변경하지 않았습니다.",
        ));
    }
    let _parent_lease = lease::RecoverableLease::acquire(
        paths::project_subagent_parent_lock(&initial.parent_workflow_id),
        "subagent parent admission",
    )?;
    let current = load_record(subagent_id)?;
    if current.status == SubagentStatus::Cancelled {
        return Ok(render_status_report(&current, "already-cancelled-no-op"));
    }
    if current.status.is_terminal() {
        return Ok(render_status_report(&current, "terminal-preserved-no-op"));
    }
    if current.status == SubagentStatus::Running {
        backend::cancel_generation_report()?;
    }
    let mut cancelled = current.clone();
    cancelled.transition_to(SubagentStatus::Cancelled, Some("user-cancelled"))?;
    let cancelled = checkpoint_record(cancelled, current.revision)?;
    append_lifecycle_event(
        &identity,
        &cancelled,
        "team.subagent.cancelled",
        "subagent cancelled",
    )?;
    Ok(render_status_report(&cancelled, "cancelled"))
}

fn admit_launch(launch: ValidatedLaunch) -> Result<AdmittedLaunch, AppError> {
    let identity = ledger::validated_current_identity()?;
    let parent_workflow_id = state::active_workflow_id()?.ok_or_else(|| {
        AppError::blocked(
            "subagent admission 차단\n- 이유: active non-terminal parent workflow가 없습니다.",
        )
    })?;
    let _parent_lease = lease::RecoverableLease::acquire(
        paths::project_subagent_parent_lock(&parent_workflow_id),
        "subagent parent admission",
    )?;
    if state::active_workflow_id()?.as_deref() != Some(parent_workflow_id.as_str()) {
        return Err(AppError::blocked(
            "subagent admission 차단\n- 이유: active parent pointer가 admission 중 변경되었습니다.",
        ));
    }
    recover_completed_parent_merges(&parent_workflow_id)?;
    let workflow_guard = state::WorkflowCheckpointGuard::acquire(&parent_workflow_id)?;
    let parent = workflow_guard.load_current()?;
    if parent.is_terminal()
        || parent.project_id != identity.project_id
        || parent.session_id != identity.session_id
        || parent.revision == 0
        || !is_sha256(&parent.artifact_hash)
    {
        return Err(AppError::blocked(
            "subagent admission 차단\n- 이유: parent project/session/revision/hash binding이 active non-terminal 상태가 아닙니다.",
        ));
    }
    if let Some(existing) = records_for_parent(&parent.workflow_id)?
        .into_iter()
        .find(|record| !record.status.is_terminal())
    {
        if existing.status == SubagentStatus::Running {
            match lease::RecoverableLease::acquire(
                paths::project_subagent_execution_lock(&existing.subagent_id),
                "subagent execution",
            ) {
                Ok(_recovery_lease) => {
                    terminalize_locked(
                        &existing,
                        SubagentStatus::Failed,
                        "interrupted-no-replay",
                        "team.subagent.failed",
                    )?;
                }
                Err(error) if error.message.contains("subagent execution lock 차단") => {
                    append_lifecycle_event(
                        &identity,
                        &existing,
                        "team.subagent.blocked",
                        "subagent admission blocked",
                    )?;
                    return Err(AppError::blocked(format!(
                        "subagent admission 차단\n- 이유: parent당 non-terminal child는 하나만 허용합니다.\n- existing child: {}\n- existing status: {}",
                        existing.subagent_id,
                        existing.status.as_str()
                    )));
                }
                Err(error) => return Err(error),
            }
        } else {
            append_lifecycle_event(
                &identity,
                &existing,
                "team.subagent.blocked",
                "subagent admission blocked",
            )?;
            return Err(AppError::blocked(format!(
                "subagent admission 차단\n- 이유: parent당 non-terminal child는 하나만 허용합니다.\n- existing child: {}\n- existing status: {}",
                existing.subagent_id,
                existing.status.as_str()
            )));
        }
    }
    let context = crate::app::context_adapter::build_declared_context_pack(&launch.read_paths)?;
    let requested = create_record(SubagentRecordV1::new(
        &parent.project_id,
        &parent.session_id,
        &parent.workflow_id,
        parent.revision,
        &parent.artifact_hash,
        launch,
    )?)?;
    append_lifecycle_event(
        &identity,
        &requested,
        "team.subagent.requested",
        "subagent requested",
    )?;
    let mut admitted = requested.clone();
    admitted.transition_to(SubagentStatus::Admitted, None)?;
    let admitted = checkpoint_record(admitted, requested.revision)?;
    append_lifecycle_event(
        &identity,
        &admitted,
        "team.subagent.admitted",
        "subagent admitted",
    )?;
    Ok(AdmittedLaunch {
        record: admitted,
        context,
    })
}

fn append_lifecycle_event(
    identity: &ledger::RuntimeIdentity,
    record: &SubagentRecordV1,
    event_type: &str,
    summary: &str,
) -> Result<String, AppError> {
    if record.project_id != identity.project_id || record.session_id != identity.session_id {
        return Err(AppError::blocked("subagent ledger owner binding 불일치"));
    }
    let event = ledger::new_event_for(
        identity,
        event_type,
        summary,
        &format!(
            "subagent_id={} parent_workflow_id={} parent_revision={} revision={} status={} role={} task_hash={} artifact_hash={} result_hash={}",
            record.subagent_id,
            record.parent_workflow_id,
            record.parent_revision,
            record.revision,
            record.status.as_str(),
            record.role.as_str(),
            record.task_hash,
            record.artifact_hash,
            display_value(&record.result_artifact_hash),
        ),
    );
    ledger::append_event(&event)?;
    Ok(event.event_id)
}

fn render_status_report(record: &SubagentRecordV1, action: &str) -> String {
    format!(
        "subagent status\n- action: {action}\n- subagent id: {}\n- status: {}\n- revision: {}\n- parent workflow: {}\n- parent revision: {}\n- role: {}\n- tools: {}\n- read paths: {}\n- write paths: {}\n- requested max tokens: {}\n- effective max tokens: {}\n- backend event: {}\n- result artifact: {}\n- evidence: {}\n- failure code: {}",
        record.subagent_id,
        record.status.as_str(),
        record.revision,
        record.parent_workflow_id,
        record.parent_revision,
        record.role.as_str(),
        record.declared_tools.join(", "),
        record.read_paths.join(", "),
        display_list(&record.write_paths),
        record.requested_max_tokens,
        record.effective_max_tokens,
        display_value(&record.backend_event_id),
        display_value(&record.result_artifact_id),
        display_value(&record.evidence_id),
        display_value(&record.failure_code),
    )
}

fn display_list(values: &[String]) -> String {
    if values.is_empty() {
        "없음".to_string()
    } else {
        values.join(", ")
    }
}

fn display_value(value: &str) -> &str {
    if value.is_empty() {
        "없음"
    } else {
        value
    }
}

#[cfg(test)]
#[path = "subagent/tests.rs"]
mod tests;
