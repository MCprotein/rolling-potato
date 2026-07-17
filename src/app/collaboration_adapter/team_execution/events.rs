use super::*;

pub(super) fn append_execution_blocked(
    identity: &ledger::RuntimeIdentity,
    team: &team_state::TeamStateV1,
    reason: &str,
) -> Result<(), AppError> {
    let event = ledger::new_event_for(
        identity,
        "team.execution.blocked",
        "team execution resource blocked",
        &format!(
            "team_id={} stage={} requested_lanes={} reason={}",
            team.team_id,
            team.stage.as_str(),
            team.requested_lanes,
            ledger::redact_text(reason),
        ),
    );
    ledger::append_event(&event)?;
    observability::project_event(&event)
}

pub(super) fn append_action_event(
    identity: &ledger::RuntimeIdentity,
    team_id: &str,
    member: &subagent::CompletedTeamMember,
    action: Option<&OwnedAction>,
) -> Result<(), AppError> {
    let details = format!(
        "team_id={} lane={} member_id={} subagent_id={} action={} target_path={} source_hash={}",
        team_id,
        member.lane,
        member.member_id,
        member.record.subagent_id,
        if action.is_some() { "patch" } else { "none" },
        action
            .map(|action| action.target_path.as_str())
            .unwrap_or("none"),
        action
            .map(|action| action.source_hash.as_str())
            .unwrap_or("none"),
    );
    if has_exact_event(identity, "team.worker.action-owned", &details)? {
        return Ok(());
    }
    let event = ledger::new_event_for(
        identity,
        "team.worker.action-owned",
        "team worker action ownership enforced",
        &details,
    );
    ledger::append_event(&event)?;
    observability::project_event(&event)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn append_worker_event(
    identity: &ledger::RuntimeIdentity,
    event_type: &str,
    summary: &str,
    team_id: &str,
    lane: u32,
    member_id: &str,
    subagent_id: &str,
    status: &str,
    result_artifact_id: &str,
    evidence_id: &str,
) -> Result<(), AppError> {
    let details = format!(
        "team_id={} lane={} member_id={} subagent_id={} status={} result_artifact_id={} evidence_id={}",
        team_id, lane, member_id, subagent_id, status, result_artifact_id, evidence_id,
    );
    if has_exact_event(identity, event_type, &details)? {
        return Ok(());
    }
    let event = ledger::new_event_for(identity, event_type, summary, &details);
    ledger::append_event(&event)?;
    observability::project_event(&event)
}

fn has_exact_event(
    identity: &ledger::RuntimeIdentity,
    event_type: &str,
    details: &str,
) -> Result<bool, AppError> {
    Ok(ledger::read_runtime_events()?.iter().any(|event| {
        event.project_id == identity.project_id
            && event.session_id == identity.session_id
            && event.event_type == event_type
            && event.details == details
    }))
}
