//! Validated workflow, current-state, session, and read-only runtime views.

use crate::foundation::error::AppError;
use crate::runtime_core::workflow::storage_compat::ledger::{
    LedgerBinding, ParsedLedgerEvent, RuntimeIdentity,
};
use crate::runtime_core::workflow::storage_compat::record::{WorkflowPointer, WorkflowRecord};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CurrentWorkflowBinding {
    pub(crate) workflow_id: String,
    pub(crate) revision: u64,
    pub(crate) artifact_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CurrentStateSnapshot {
    pub(crate) schema_version: u64,
    pub(crate) revision: u64,
    pub(crate) previous_artifact_hash: String,
    pub(crate) project_id: String,
    pub(crate) project_root: String,
    pub(crate) session_id: String,
    pub(crate) active_workflow: Option<CurrentWorkflowBinding>,
    pub(crate) parent_session_id: Option<String>,
    pub(crate) branch_from_event_id: Option<String>,
    pub(crate) compaction_boundary: Option<String>,
    pub(crate) resume_source: Option<String>,
    pub(crate) ledger_binding: LedgerBinding,
    pub(crate) artifact_hash: String,
    pub(crate) legacy_canonical_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CurrentStateLeaseView {
    pub revision: u64,
    pub artifact_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TuiStateSnapshot {
    pub identity: RuntimeIdentity,
    pub current_revision: u64,
    pub current_hash: String,
    pub ledger_binding: LedgerBinding,
    pub ledger_events: Vec<ParsedLedgerEvent>,
    pub active_workflow: Option<WorkflowRecord>,
    pub ledger_tail_truncated: bool,
    pub current_ledger_binding_stale: bool,
}

pub(crate) fn validate_session_resume_target(
    session_id: &str,
    canonical_session: bool,
    projected_session: bool,
    active_workflow: Option<&WorkflowRecord>,
) -> Result<Option<String>, AppError> {
    if !canonical_session {
        return Err(AppError::blocked(format!(
            "session resume 차단\n- session id: {}\n- 이유: canonical runtime ledger에서 현재 project의 session을 찾지 못했습니다.\n- 확인: `rpotato session list`",
            session_id
        )));
    }
    if !projected_session {
        return Err(AppError::blocked(format!(
            "session resume 차단\n- session id: {}\n- 이유: canonical ledger에는 존재하지만 SQLite projection 재생성 후 session을 찾지 못했습니다.\n- 확인: `rpotato state status`",
            session_id
        )));
    }
    if let Some(workflow) = active_workflow {
        if workflow.session_id != session_id {
            return Err(AppError::blocked(format!(
                "session resume 차단\n- session id: {}\n- 이유: 다른 session이 소유한 non-terminal workflow가 있습니다.\n- active workflow: {}\n- owner session: {}\n- 동작: current-state를 변경하지 않았습니다.",
                session_id, workflow.workflow_id, workflow.session_id
            )));
        }
    }
    Ok(active_workflow.map(|workflow| workflow.workflow_id.clone()))
}

pub(crate) fn validate_snapshot_identity(
    snapshot: &CurrentStateSnapshot,
    identity: &RuntimeIdentity,
) -> Result<(), AppError> {
    if snapshot.project_id == identity.project_id && snapshot.session_id == identity.session_id {
        Ok(())
    } else {
        Err(AppError::blocked(
            "selection current-state identity binding 불일치",
        ))
    }
}

pub(crate) fn validate_current_lease(
    snapshot: &CurrentStateSnapshot,
    current_ledger: &LedgerBinding,
    active_workflow: Option<&WorkflowRecord>,
) -> Result<CurrentStateLeaseView, AppError> {
    if &snapshot.ledger_binding != current_ledger {
        return Err(AppError::blocked(
            "current-state lease 차단\n- code: selection.stale-ledger-binding\n- 동작: ledger와 current-state가 수렴하기 전 선택 권한을 만들지 않았습니다.",
        ));
    }
    match (snapshot.active_workflow.as_ref(), active_workflow) {
        (Some(binding), Some(workflow))
            if binding.workflow_id == workflow.workflow_id
                && binding.revision == workflow.revision
                && binding.artifact_hash == workflow.artifact_hash => {}
        (Some(_), _) => {
            return Err(AppError::blocked(
                "current-state lease 차단\n- code: selection.stale-workflow-binding\n- 동작: workflow pointer와 current-state가 일치하지 않습니다.",
            ));
        }
        (None, None) => {}
        (None, Some(_)) => {
            return Err(AppError::blocked(
                "current-state lease 차단\n- code: selection.stale-workflow-binding\n- 동작: workflow pointer와 current-state가 일치하지 않습니다.",
            ));
        }
    }
    Ok(CurrentStateLeaseView {
        revision: snapshot.revision,
        artifact_hash: snapshot.artifact_hash.clone(),
    })
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(session_id: &str) -> RuntimeIdentity {
        RuntimeIdentity {
            project_id: "project-1".to_string(),
            session_id: session_id.to_string(),
            project_root: "/project".to_string(),
        }
    }

    fn ledger_binding(event_count: u64, event_id: Option<&str>, event_hash: &str) -> LedgerBinding {
        LedgerBinding {
            event_count,
            event_id: event_id.map(str::to_string),
            event_hash: event_hash.to_string(),
        }
    }

    fn snapshot(active_workflow: Option<CurrentWorkflowBinding>) -> CurrentStateSnapshot {
        CurrentStateSnapshot {
            schema_version: 2,
            revision: 4,
            previous_artifact_hash: "previous-state-hash".to_string(),
            project_id: "project-1".to_string(),
            project_root: "/project".to_string(),
            session_id: "session-1".to_string(),
            active_workflow,
            parent_session_id: None,
            branch_from_event_id: None,
            compaction_boundary: None,
            resume_source: None,
            ledger_binding: ledger_binding(1, Some("event-1"), "ledger-hash-1"),
            artifact_hash: "state-hash-4".to_string(),
            legacy_canonical_hash: None,
        }
    }

    fn active_workflow(session_id: &str) -> WorkflowRecord {
        let mut workflow = WorkflowRecord::new(&identity(session_id), "test request");
        workflow.workflow_id = "workflow-1".to_string();
        workflow.revision = 2;
        workflow.previous_hash = "workflow-hash-1".to_string();
        workflow.artifact_hash = "workflow-hash-2".to_string();
        workflow
    }

    #[test]
    fn session_resume_requires_both_authorities_and_matching_workflow_owner() {
        let canonical_error =
            validate_session_resume_target("session-1", false, false, None).unwrap_err();
        assert!(canonical_error.message.contains("canonical runtime ledger"));

        let projection_error =
            validate_session_resume_target("session-1", true, false, None).unwrap_err();
        assert!(projection_error.message.contains("SQLite projection"));

        let other_owner = active_workflow("session-2");
        let owner_error =
            validate_session_resume_target("session-1", true, true, Some(&other_owner))
                .unwrap_err();
        assert!(owner_error.message.contains("다른 session"));

        let same_owner = active_workflow("session-1");
        assert_eq!(
            validate_session_resume_target("session-1", true, true, Some(&same_owner)).unwrap(),
            Some("workflow-1".to_string())
        );
    }

    #[test]
    fn lease_rejects_stale_ledger_and_workflow_bindings() {
        let stale_ledger = ledger_binding(2, Some("event-2"), "ledger-hash-2");
        let ledger_error =
            validate_current_lease(&snapshot(None), &stale_ledger, None).unwrap_err();
        assert!(ledger_error.message.contains("stale-ledger-binding"));

        let binding = CurrentWorkflowBinding {
            workflow_id: "workflow-1".to_string(),
            revision: 2,
            artifact_hash: "workflow-hash-2".to_string(),
        };
        let mut workflow = active_workflow("session-1");
        workflow.revision = 3;
        let current = snapshot(Some(binding));
        let workflow_error =
            validate_current_lease(&current, &current.ledger_binding, Some(&workflow)).unwrap_err();
        assert!(workflow_error.message.contains("stale-workflow-binding"));
    }

    #[test]
    fn read_only_workflow_requires_matching_checkpoint() {
        let identity = identity("session-1");
        let workflow = active_workflow("session-1");
        let binding = CurrentWorkflowBinding {
            workflow_id: workflow.workflow_id.clone(),
            revision: workflow.revision,
            artifact_hash: workflow.artifact_hash.clone(),
        };

        let missing = validate_read_only_workflow(&binding, &identity, &workflow, &[]).unwrap_err();
        assert!(missing.message.contains("bounded canonical ledger tail"));

        let checkpoint = ParsedLedgerEvent {
            event_id: "event-1".to_string(),
            ts_ms: 1,
            event_type: "workflow.checkpoint".to_string(),
            project_id: identity.project_id.clone(),
            session_id: identity.session_id.clone(),
            summary: "checkpoint".to_string(),
            details: format!(
                "workflow_id={} revision={} artifact_hash={} previous_hash={}",
                workflow.workflow_id,
                workflow.revision,
                workflow.artifact_hash,
                workflow.previous_hash
            ),
            previous_event_hash: Some("root".to_string()),
            event_hash: Some("ledger-hash-1".to_string()),
        };
        validate_read_only_workflow(&binding, &identity, &workflow, &[checkpoint]).unwrap();
    }
}
