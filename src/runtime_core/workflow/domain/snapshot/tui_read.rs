//! Read-only TUI identity, ledger, pointer, and workflow validation.

use crate::foundation::error::AppError;
use crate::runtime_core::workflow::storage_compat::ledger::{
    LedgerBinding, ParsedLedgerEvent, RuntimeIdentity,
};
use crate::runtime_core::workflow::storage_compat::record::{WorkflowPointer, WorkflowRecord};

use super::types::{CurrentStateSnapshot, CurrentWorkflowBinding};

pub(crate) fn validated_tui_identity(
    snapshot: &CurrentStateSnapshot,
    fresh: &RuntimeIdentity,
) -> Result<RuntimeIdentity, AppError> {
    if snapshot.project_id != fresh.project_id || snapshot.project_root != fresh.project_root {
        return Err(AppError::blocked(
            "TUI current-state project binding 불일치",
        ));
    }
    Ok(RuntimeIdentity {
        project_id: snapshot.project_id.clone(),
        session_id: snapshot.session_id.clone(),
        project_root: snapshot.project_root.clone(),
    })
}

pub(crate) fn validate_ledger_ancestor(
    current: &LedgerBinding,
    tail_binding: &LedgerBinding,
    tail_events: &[ParsedLedgerEvent],
) -> Result<(), AppError> {
    if current == tail_binding || current.event_count == 0 && current.event_hash == "root" {
        return Ok(());
    }
    if current.event_count > tail_binding.event_count || current.event_id.is_none() {
        return Err(AppError::blocked(
            "TUI current-state ledger binding은 canonical head의 ancestor가 아닙니다.",
        ));
    }
    let first_ordinal = tail_binding
        .event_count
        .saturating_sub(tail_events.len() as u64)
        .saturating_add(1);
    let index = current
        .event_count
        .checked_sub(first_ordinal)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| {
            AppError::blocked(
                "TUI current-state ledger ancestor가 bounded canonical tail 밖에 있습니다.",
            )
        })?;
    let event = tail_events.get(index).ok_or_else(|| {
        AppError::blocked("TUI current-state ledger ancestor ordinal이 canonical tail과 다릅니다.")
    })?;
    if current.event_id.as_deref() != Some(event.event_id.as_str())
        || event.event_hash.as_deref() != Some(current.event_hash.as_str())
    {
        return Err(AppError::blocked(
            "TUI current-state ledger ancestor id/hash binding 불일치",
        ));
    }
    Ok(())
}

pub(crate) fn validate_selection_ledger_suffix(
    current: &LedgerBinding,
    tail_binding: &LedgerBinding,
    events: &[ParsedLedgerEvent],
) -> Result<(), AppError> {
    validate_ledger_ancestor(current, tail_binding, events)?;
    let suffix_start = usize::try_from(current.event_count)
        .map_err(|_| AppError::blocked("current-state ledger ordinal 범위 초과"))?;
    if events
        .get(suffix_start..)
        .unwrap_or_default()
        .iter()
        .all(|event| event.event_type == "transcript.recorded")
    {
        Ok(())
    } else {
        Err(AppError::blocked(
            "current-state lease 차단\n- code: selection.stale-ledger-binding\n- 동작: 대화 기록 외 상태 변경이 current-state 이후 발견되어 선택 권한을 만들지 않았습니다.",
        ))
    }
}

pub(crate) fn validate_read_only_pointer(
    binding: &CurrentWorkflowBinding,
    pointer: &WorkflowPointer,
) -> Result<(), AppError> {
    if pointer.workflow_id != binding.workflow_id
        || pointer.committed_revision != binding.revision
        || pointer.artifact_hash != binding.artifact_hash
        || pointer.committed_revision == 0
    {
        Err(AppError::blocked(
            "TUI workflow pointer/current-state binding 불일치",
        ))
    } else {
        Ok(())
    }
}

pub(crate) fn validate_read_only_workflow(
    binding: &CurrentWorkflowBinding,
    identity: &RuntimeIdentity,
    workflow: &WorkflowRecord,
    ledger_events: &[ParsedLedgerEvent],
) -> Result<(), AppError> {
    if workflow.workflow_id != binding.workflow_id
        || workflow.revision != binding.revision
        || workflow.artifact_hash != binding.artifact_hash
        || workflow.project_id != identity.project_id
        || workflow.session_id != identity.session_id
    {
        return Err(AppError::blocked(
            "TUI workflow snapshot owner/hash binding 불일치",
        ));
    }
    let revision = binding.revision.to_string();
    let checkpoint = ledger_events.iter().rev().find(|event| {
        event.event_type == "workflow.checkpoint"
            && event.project_id == identity.project_id
            && detail_value(&event.details, "workflow_id") == Some(binding.workflow_id.as_str())
            && detail_value(&event.details, "revision") == Some(revision.as_str())
            && detail_value(&event.details, "artifact_hash") == Some(binding.artifact_hash.as_str())
            && detail_value(&event.details, "previous_hash")
                == Some(workflow.previous_hash.as_str())
    });
    if checkpoint.is_none() {
        return Err(AppError::blocked(
            "TUI workflow checkpoint가 bounded canonical ledger tail에 없습니다.",
        ));
    }
    Ok(())
}

fn detail_value<'a>(details: &'a str, key: &str) -> Option<&'a str> {
    details.split_ascii_whitespace().find_map(|part| {
        let (candidate, value) = part.split_once('=')?;
        (candidate == key).then_some(value)
    })
}
