use super::*;
use crate::foundation::error::AppError;
use crate::runtime_core::workflow::storage_compat::record::WorkflowRecord;
use crate::surfaces::tui::outcome::{TuiOutcome, TuiOutcomeCode};
use crate::surfaces::tui::runtime_bridge::{
    ObservedWorkflow, OneShotSecret, SelectionLease, SelectionObservation, TuiGateKind,
};

struct StubPort;

impl TuiActionPort for StubPort {
    fn selection_observation(&mut self) -> Result<SelectionObservation, AppError> {
        Ok(observation())
    }

    fn workflow(&mut self, _workflow_id: &str) -> Result<WorkflowRecord, AppError> {
        unreachable!("workflow lookup is not used by these action tests")
    }

    fn approve_patch(
        &mut self,
        _proposal_id: &str,
        _token: &str,
        _intent_id: &str,
        _lease: &SelectionLease,
    ) -> Result<Option<OneShotSecret>, TuiMutationFailure> {
        unreachable!("patch approval is not used by these action tests")
    }

    fn approve_verification(
        &mut self,
        _proposal_id: &str,
        _token: &str,
        _intent_id: &str,
        _lease: &SelectionLease,
    ) -> Result<(), TuiMutationFailure> {
        Err(TuiMutationFailure::StaleSelection)
    }

    fn deny_pending_gate(
        &mut self,
        _workflow_id: &str,
        _intent_id: &str,
        _gate_id: &str,
        _gate_kind: TuiGateKind,
        _lease: &SelectionLease,
    ) -> Result<TuiOutcome, TuiMutationFailure> {
        unreachable!("denial is not used by these action tests")
    }

    fn resume_workflow(
        &mut self,
        _workflow_id: &str,
        _intent_id: &str,
        _lease: &SelectionLease,
    ) -> Result<(), TuiMutationFailure> {
        unreachable!("resume is not used by these action tests")
    }

    fn cancel_workflow(
        &mut self,
        _workflow_id: &str,
        _intent_id: &str,
        _lease: &SelectionLease,
    ) -> Result<(), TuiMutationFailure> {
        Err(TuiMutationFailure::CancelTerminal("complete".to_string()))
    }

    fn resume_session(
        &mut self,
        _session_id: &str,
        _intent_id: &str,
        _lease: &SelectionLease,
    ) -> Result<Option<String>, AppError> {
        unreachable!("session resume is not used by these action tests")
    }
}

fn observation() -> SelectionObservation {
    SelectionObservation {
        project_id: "project-test".to_string(),
        session_id: "session-test".to_string(),
        current_revision: 7,
        current_hash: "sha256:current".to_string(),
        active_workflow: Some(ObservedWorkflow {
            workflow_id: "workflow-test".to_string(),
            revision: 3,
            hash: "sha256:workflow".to_string(),
        }),
    }
}

#[test]
fn selection_lease_is_derived_from_the_observed_boundary() {
    let lease = selection_lease(&mut StubPort, "workflow-test").unwrap();

    assert_eq!(lease, observation().lease_for("workflow-test"));
}

#[test]
fn stale_verification_maps_to_the_exact_refresh_outcome() {
    let lease = observation().lease_for("workflow-test");
    let outcome = dispatch_intent(
        &mut StubPort,
        TuiIntent::ApproveVerification {
            intent_id: "intent-test".to_string(),
            proposal_id: "proposal-test".to_string(),
            lease,
            secret: OneShotSecret::new("secret".to_string()).unwrap(),
        },
    )
    .unwrap();

    assert_eq!(outcome.code, TuiOutcomeCode::ResumeStaleSelection);
}

#[test]
fn terminal_cancel_maps_to_the_exact_blocked_outcome() {
    let lease = observation().lease_for("workflow-test");
    let outcome = dispatch_intent(
        &mut StubPort,
        TuiIntent::CancelWorkflow {
            intent_id: "intent-test".to_string(),
            workflow_id: "workflow-test".to_string(),
            lease,
        },
    )
    .unwrap();

    assert_eq!(outcome.code, TuiOutcomeCode::CancelTerminalBlocked);
    assert!(outcome.safe_message.contains("phase: complete"));
}
