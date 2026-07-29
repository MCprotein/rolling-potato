use super::super::*;

pub(crate) fn prepared_approval_receipt_exists(
    record: &ProposalRecord,
    workflow: &state::WorkflowRecord,
    intent_id: &str,
) -> Result<bool, AppError> {
    let expected_types = [
        "runtime.intent.accepted",
        "workflow.checkpoint",
        "patch.apply.approved",
        "hook.dispatched",
        "hook.dispatched",
        "hook.dispatched",
        "hook.dispatched",
        "patch.applied",
        "transcript.recorded",
        "workflow.checkpoint",
    ];
    let e0_details = format!(
        "intent_id={intent_id} intent_kind=approve-patch workflow_id={} proposal_id={}",
        workflow.workflow_id, record.proposal_id
    );
    let events = ledger::read_runtime_events()?;
    let Some(start) = events.iter().position(|event| {
        event.event_type == "runtime.intent.accepted"
            && event.project_id == workflow.project_id
            && event.session_id == workflow.session_id
            && event.details == e0_details
    }) else {
        return Ok(false);
    };
    let Some(receipt) = events.get(start..start + expected_types.len()) else {
        return Ok(false);
    };
    if receipt
        .iter()
        .zip(expected_types)
        .any(|(event, expected)| event.event_type != expected)
    {
        return Ok(false);
    }
    let e7 = &receipt[7];
    let e9 = &receipt[9];
    Ok(e7
        .details
        .contains(&format!("proposal_id={}", record.proposal_id))
        && e7
            .details
            .contains(&format!("applied_sha256={}", record.proposed_sha256))
        && e9.details.contains(&format!(
            "workflow_id={} revision={} artifact_hash={}",
            workflow.workflow_id, workflow.revision, workflow.artifact_hash
        )))
}
