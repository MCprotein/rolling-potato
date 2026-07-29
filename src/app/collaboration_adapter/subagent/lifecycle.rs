use crate::adapters::filesystem::{layout as paths, lease};
use crate::app::inference_adapter::backend;
use crate::app::workflow_adapter::ledger;
use crate::foundation::error::AppError;
use crate::runtime_core::collaboration::subagent::{SubagentRecordV1, SubagentStatus};

use super::persistence::{checkpoint_record, load_record};
use super::reporting::render_status_report;

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

pub(super) fn append_lifecycle_event(
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

fn display_value(value: &str) -> &str {
    if value.is_empty() {
        "없음"
    } else {
        value
    }
}
