use super::*;

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
