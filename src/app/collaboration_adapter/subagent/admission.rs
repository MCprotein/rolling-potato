use crate::adapters::filesystem::{layout as paths, lease};
use crate::app::workflow_adapter::{ledger, state};
use crate::foundation::error::AppError;
use crate::runtime_core::collaboration::subagent::{
    is_sha256, validate_launch, SubagentRecordV1, SubagentStatus, ValidatedLaunch,
};

use super::execution::{recover_completed_parent_merges, terminalize_locked};
use super::lifecycle::append_lifecycle_event;
use super::persistence::{checkpoint_record, create_record, load_record, records_for_parent};

#[derive(Debug)]
pub(crate) struct AdmittedLaunch {
    pub(super) record: SubagentRecordV1,
    pub(super) context: crate::app::context_adapter::ContextPack,
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
    pub(super) task: String,
    pub(super) admitted: AdmittedLaunch,
}

impl AdmittedTeamMember {
    pub fn subagent_id(&self) -> &str {
        &self.admitted.record.subagent_id
    }
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
        let result = admit_team_member(&identity, &parent, member, launch, context);
        match result {
            Ok(admitted) => admitted_members.push(admitted),
            Err(error) => {
                rollback_team_admission(&admitted_members)?;
                return Err(error);
            }
        }
    }
    Ok(admitted_members)
}

fn admit_team_member(
    identity: &ledger::RuntimeIdentity,
    parent: &state::WorkflowRecord,
    member: TeamMemberLaunch,
    launch: ValidatedLaunch,
    context: crate::app::context_adapter::ContextPack,
) -> Result<AdmittedTeamMember, AppError> {
    let requested = create_record(SubagentRecordV1::new(
        &parent.project_id,
        &parent.session_id,
        &parent.workflow_id,
        parent.revision,
        &parent.artifact_hash,
        launch,
    )?)?;
    append_lifecycle_event(
        identity,
        &requested,
        "team.subagent.requested",
        "team member requested",
    )?;
    let mut admitted = requested.clone();
    admitted.transition_to(SubagentStatus::Admitted, None)?;
    let admitted = checkpoint_record(admitted, requested.revision)?;
    append_lifecycle_event(
        identity,
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
}

fn rollback_team_admission(admitted_members: &[AdmittedTeamMember]) -> Result<(), AppError> {
    for admitted in admitted_members {
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
    Ok(())
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

pub(super) fn admit_launch(launch: ValidatedLaunch) -> Result<AdmittedLaunch, AppError> {
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
    validate_active_parent(&identity, &parent)?;
    recover_or_block_existing_child(&identity, &parent)?;

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

fn validate_active_parent(
    identity: &ledger::RuntimeIdentity,
    parent: &state::WorkflowRecord,
) -> Result<(), AppError> {
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
    Ok(())
}

fn recover_or_block_existing_child(
    identity: &ledger::RuntimeIdentity,
    parent: &state::WorkflowRecord,
) -> Result<(), AppError> {
    let Some(existing) = records_for_parent(&parent.workflow_id)?
        .into_iter()
        .find(|record| !record.status.is_terminal())
    else {
        return Ok(());
    };
    if existing.status == SubagentStatus::Running {
        match lease::RecoverableLease::acquire(
            paths::project_subagent_execution_lock(&existing.subagent_id),
            "subagent execution",
        ) {
            Ok(_recovery_lease) => {
                return terminalize_locked(
                    &existing,
                    SubagentStatus::Failed,
                    "interrupted-no-replay",
                    "team.subagent.failed",
                )
                .map(|_| ());
            }
            Err(error) if error.message.contains("subagent execution lock 차단") => {}
            Err(error) => return Err(error),
        }
    }
    append_lifecycle_event(
        identity,
        &existing,
        "team.subagent.blocked",
        "subagent admission blocked",
    )?;
    Err(AppError::blocked(format!(
        "subagent admission 차단\n- 이유: parent당 non-terminal child는 하나만 허용합니다.\n- existing child: {}\n- existing status: {}",
        existing.subagent_id,
        existing.status.as_str()
    )))
}
