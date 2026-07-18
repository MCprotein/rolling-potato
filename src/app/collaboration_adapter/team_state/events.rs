use super::*;

pub(super) fn append_planned_event_if_missing(
    identity: &ledger::RuntimeIdentity,
    record: &TeamStateV1,
) -> Result<(), AppError> {
    if ledger::event_details_match(
        "team.stage.planned",
        &[("team_id", record.team_id.as_str()), ("revision", "1")],
    )? {
        return Ok(());
    }
    let event = ledger::new_event_for(
        identity,
        "team.stage.planned",
        "team plan recorded",
        &format!(
            "team_id={} revision={} stage={} parent_workflow_id={} member_count={} manifest_hash={}",
            record.team_id,
            record.revision,
            record.stage.as_str(),
            record.parent_workflow_id,
            record.member_count,
            record.manifest_hash,
        ),
    );
    let appended = ledger::append_event(&event)?;
    observability::project_event_with_ordinal(&event, appended.ordinal)
}

pub(super) fn append_stage_event_if_missing(
    identity: &ledger::RuntimeIdentity,
    record: &TeamStateV1,
) -> Result<(), AppError> {
    let event_type = match record.stage {
        TeamStage::Plan => "team.stage.planned",
        TeamStage::Dispatch => "team.stage.dispatched",
        TeamStage::Execute => "team.stage.executing",
        TeamStage::Review => "team.stage.reviewing",
        TeamStage::Verify => "team.stage.verifying",
        TeamStage::Merge => "team.stage.merging",
        TeamStage::Report => "team.stage.reporting",
        TeamStage::Complete => "team.stage.completed",
        TeamStage::Failed => "team.stage.failed",
        TeamStage::Cancelled => "team.stage.cancelled",
    };
    if ledger::event_details_match(
        event_type,
        &[
            ("team_id", record.team_id.as_str()),
            ("stage", record.stage.as_str()),
        ],
    )? {
        return Ok(());
    }
    let event = ledger::new_event_for(
        identity,
        event_type,
        "team stage advanced",
        &format!(
            "team_id={} revision={} stage={} status={} requested_lanes={} admitted_lanes={} execution_mode={}",
            record.team_id,
            record.revision,
            record.stage.as_str(),
            record.status,
            record.requested_lanes,
            record.admitted_lanes,
            record.execution_mode,
        ),
    );
    let appended = ledger::append_event(&event)?;
    observability::project_event_with_ordinal(&event, appended.ordinal)
}
