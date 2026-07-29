//! Facade for validated workflow, session, lease, and read-only runtime views.

mod lease;
mod session;
mod tui_read;
mod types;

pub(crate) use lease::{validate_current_lease, validate_snapshot_identity};
pub(crate) use session::validate_session_resume_target;
pub(crate) use tui_read::{
    validate_ledger_ancestor, validate_read_only_pointer, validate_read_only_workflow,
    validate_selection_ledger_suffix, validated_tui_identity,
};
pub(crate) use types::{
    CurrentStateLeaseView, CurrentStateSnapshot, CurrentWorkflowBinding, TuiStateSnapshot,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_core::workflow::storage_compat::ledger::{
        LedgerBinding, ParsedLedgerEvent, RuntimeIdentity,
    };
    use crate::runtime_core::workflow::storage_compat::record::WorkflowRecord;

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
    fn selection_lease_accepts_only_transcript_events_after_the_current_state() {
        let transcript = ParsedLedgerEvent {
            event_id: "event-2".to_string(),
            ts_ms: 2,
            event_type: "transcript.recorded".to_string(),
            project_id: "project-1".to_string(),
            session_id: "session-1".to_string(),
            summary: "model transcript record persisted".to_string(),
            details: "record_id=record-1".to_string(),
            previous_event_hash: Some("ledger-hash-1".to_string()),
            event_hash: Some("ledger-hash-2".to_string()),
        };
        let head = ledger_binding(2, Some("event-2"), "ledger-hash-2");
        assert!(validate_selection_ledger_suffix(
            &snapshot(None).ledger_binding,
            &head,
            std::slice::from_ref(&transcript)
        )
        .is_err());
        let ancestor = ParsedLedgerEvent {
            event_id: "event-1".to_string(),
            ts_ms: 1,
            event_type: "session.new".to_string(),
            project_id: "project-1".to_string(),
            session_id: "session-1".to_string(),
            summary: "session".to_string(),
            details: String::new(),
            previous_event_hash: Some("root".to_string()),
            event_hash: Some("ledger-hash-1".to_string()),
        };
        assert!(validate_selection_ledger_suffix(
            &snapshot(None).ledger_binding,
            &head,
            &[ancestor, transcript.clone()]
        )
        .is_ok());

        let mut state_change = transcript;
        state_change.event_type = "session.resume.selected".to_string();
        assert!(validate_selection_ledger_suffix(
            &snapshot(None).ledger_binding,
            &head,
            &[
                ParsedLedgerEvent {
                    event_id: "event-1".to_string(),
                    ts_ms: 1,
                    event_type: "session.new".to_string(),
                    project_id: "project-1".to_string(),
                    session_id: "session-1".to_string(),
                    summary: "session".to_string(),
                    details: String::new(),
                    previous_event_hash: Some("root".to_string()),
                    event_hash: Some("ledger-hash-1".to_string()),
                },
                state_change,
            ]
        )
        .is_err());
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
