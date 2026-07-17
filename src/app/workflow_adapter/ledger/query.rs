use crate::foundation::error::AppError;
use crate::runtime_core::workflow::storage_compat::ledger::WorkflowCheckpoint;

use super::read_runtime_events;
use super::storage::is_sha256;

pub fn event_detail_exists(event_type: &str, field: &str, value: &str) -> Result<bool, AppError> {
    Ok(read_runtime_events()?.iter().any(|event| {
        event.event_type == event_type && detail_value(&event.details, field) == Some(value)
    }))
}

pub fn event_details_match(event_type: &str, fields: &[(&str, &str)]) -> Result<bool, AppError> {
    Ok(read_runtime_events()?.iter().any(|event| {
        event.event_type == event_type
            && fields
                .iter()
                .all(|(field, value)| detail_value(&event.details, field) == Some(*value))
    }))
}

pub fn workflow_checkpoint_exists(
    workflow_id: &str,
    revision: u64,
    artifact_hash: &str,
) -> Result<bool, AppError> {
    Ok(workflow_checkpoints(workflow_id)?.iter().any(|checkpoint| {
        checkpoint.revision == revision && checkpoint.artifact_hash == artifact_hash
    }))
}

pub fn workflow_checkpoints(workflow_id: &str) -> Result<Vec<WorkflowCheckpoint>, AppError> {
    let mut checkpoints = Vec::new();
    for event in read_runtime_events()? {
        if event.event_type != "workflow.checkpoint"
            || detail_value(&event.details, "workflow_id") != Some(workflow_id)
        {
            continue;
        }
        let revision = detail_value(&event.details, "revision")
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or_else(|| malformed_checkpoint(&event.event_id))?;
        let artifact_hash = detail_value(&event.details, "artifact_hash")
            .filter(|value| is_sha256(value))
            .ok_or_else(|| malformed_checkpoint(&event.event_id))?
            .to_string();
        let previous_hash = detail_value(&event.details, "previous_hash")
            .filter(|value| *value == "none" || is_sha256(value))
            .ok_or_else(|| malformed_checkpoint(&event.event_id))?
            .to_string();
        checkpoints.push(WorkflowCheckpoint {
            revision,
            artifact_hash,
            previous_hash,
        });
    }
    checkpoints.sort_by_key(|checkpoint| checkpoint.revision);
    for (index, checkpoint) in checkpoints.iter().enumerate() {
        let expected_revision = index as u64 + 1;
        let expected_previous = if index == 0 {
            "none"
        } else {
            checkpoints[index - 1].artifact_hash.as_str()
        };
        if checkpoint.revision != expected_revision || checkpoint.previous_hash != expected_previous
        {
            return Err(AppError::blocked(format!(
                "workflow ledger chain 검증 차단\n- workflow id: {workflow_id}\n- revision: {}\n- 이유: latest checkpoint 또는 previous_hash chain 불일치",
                checkpoint.revision
            )));
        }
    }
    Ok(checkpoints)
}

fn detail_value<'a>(details: &'a str, key: &str) -> Option<&'a str> {
    details.split_whitespace().find_map(|field| {
        let (candidate, value) = field.split_once('=')?;
        (candidate == key).then_some(value)
    })
}

fn malformed_checkpoint(event_id: &str) -> AppError {
    AppError::blocked(format!(
        "workflow ledger checkpoint 검증 차단\n- event id: {event_id}\n- 이유: required checkpoint field가 malformed입니다."
    ))
}
