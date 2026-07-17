use crate::foundation::error::AppError;
use crate::runtime_core::workflow::storage_compat::record::WorkflowRecord;
use crate::surfaces::tui::outcome::{
    exact_tui_outcome, unsupported_source_platform_outcome, validate_tui_id,
    verification_credential_issued, TuiOutcome, TuiOutcomeCode, TuiOutcomeContext,
};
use crate::surfaces::tui::runtime_bridge::{
    OneShotSecret, SelectionLease, SelectionObservation, TuiGateKind, TuiIntent,
};

pub(crate) enum TuiMutationFailure {
    StaleSelection,
    ResumeInconclusiveEffect,
    ResumeCorruptState,
    CancelNoActiveWorkflow,
    CancelTerminal(String),
    RollbackConflict,
    Other(AppError),
}

pub(crate) trait TuiActionPort {
    fn selection_observation(&mut self) -> Result<SelectionObservation, AppError>;
    fn workflow(&mut self, workflow_id: &str) -> Result<WorkflowRecord, AppError>;
    fn approve_patch(
        &mut self,
        proposal_id: &str,
        token: &str,
        intent_id: &str,
        lease: &SelectionLease,
    ) -> Result<Option<OneShotSecret>, TuiMutationFailure>;
    fn approve_verification(
        &mut self,
        proposal_id: &str,
        token: &str,
        intent_id: &str,
        lease: &SelectionLease,
    ) -> Result<(), TuiMutationFailure>;
    fn deny_pending_gate(
        &mut self,
        workflow_id: &str,
        intent_id: &str,
        gate_id: &str,
        gate_kind: TuiGateKind,
        lease: &SelectionLease,
    ) -> Result<TuiOutcome, TuiMutationFailure>;
    fn resume_workflow(
        &mut self,
        workflow_id: &str,
        intent_id: &str,
        lease: &SelectionLease,
    ) -> Result<(), TuiMutationFailure>;
    fn cancel_workflow(
        &mut self,
        workflow_id: &str,
        intent_id: &str,
        lease: &SelectionLease,
    ) -> Result<(), TuiMutationFailure>;
    fn resume_session(
        &mut self,
        session_id: &str,
        intent_id: &str,
        lease: &SelectionLease,
    ) -> Result<Option<String>, AppError>;
}

pub(crate) fn selection_lease(
    port: &mut impl TuiActionPort,
    selected_object_id: &str,
) -> Result<SelectionLease, AppError> {
    validate_tui_id(selected_object_id, "selected object")?;
    Ok(port.selection_observation()?.lease_for(selected_object_id))
}

pub(crate) fn gate_descriptor(
    port: &mut impl TuiActionPort,
    workflow_id: &str,
) -> Result<(String, TuiGateKind), AppError> {
    let workflow = port.workflow(workflow_id)?;
    let kind = match (workflow.phase.as_str(), workflow.failure_reason.as_str()) {
        ("cancelled", "user-denied-patch") => TuiGateKind::PatchApply,
        ("cancelled", "user-denied-verification") => TuiGateKind::VerificationCommand,
        ("pending-approval" | "approved", _) => TuiGateKind::PatchApply,
        (
            "pending-verification-approval"
            | "verification-approved"
            | "verification-started"
            | "verified",
            _,
        ) => TuiGateKind::VerificationCommand,
        _ if matches!(
            workflow.approval_state.as_str(),
            "pending" | "pending-rotated"
        ) =>
        {
            TuiGateKind::PatchApply
        }
        _ => TuiGateKind::VerificationCommand,
    };
    Ok((workflow.proposal_id, kind))
}

pub(crate) fn dispatch_intent(
    port: &mut impl TuiActionPort,
    intent: TuiIntent,
) -> Result<TuiOutcome, AppError> {
    match intent {
        TuiIntent::Refresh { .. } | TuiIntent::Inspect { .. } => Err(AppError::usage(
            "TUI read intent는 read_tui_page 경계를 사용해야 합니다.",
        )),
        TuiIntent::ApprovePatch {
            intent_id,
            proposal_id,
            lease,
            secret,
        } => {
            validate_tui_id(&intent_id, "intent")?;
            if !cfg!(unix) {
                return unsupported_source_platform_outcome(std::env::consts::OS);
            }
            let verification_token = match secret
                .expose(|token| port.approve_patch(&proposal_id, token, &intent_id, &lease))
            {
                Ok(token) => token,
                Err(TuiMutationFailure::StaleSelection) => {
                    return stale_selection_outcome(&lease.selected_object_id)
                }
                Err(failure) => return Err(unexpected_or_other("approve patch", failure)),
            };
            match verification_token {
                Some(credential) => verification_credential_issued(&intent_id, credential),
                None => Ok(secret_refresh_only(&intent_id)),
            }
        }
        TuiIntent::ApproveVerification {
            intent_id,
            proposal_id,
            lease,
            secret,
        } => {
            validate_tui_id(&intent_id, "intent")?;
            match secret
                .expose(|token| port.approve_verification(&proposal_id, token, &intent_id, &lease))
            {
                Ok(()) => {}
                Err(TuiMutationFailure::StaleSelection) => {
                    return stale_selection_outcome(&lease.selected_object_id)
                }
                Err(failure) => return Err(unexpected_or_other("approve verification", failure)),
            }
            Ok(secret_refresh_only(&intent_id))
        }
        TuiIntent::DenyPendingGate {
            intent_id,
            workflow_id,
            gate_id,
            gate_kind,
            lease,
        } => match port.deny_pending_gate(&workflow_id, &intent_id, &gate_id, gate_kind, &lease) {
            Err(TuiMutationFailure::StaleSelection) => stale_selection_outcome(&workflow_id),
            Err(failure) => Err(unexpected_or_other("deny pending gate", failure)),
            Ok(outcome) => Ok(outcome),
        },
        TuiIntent::ResumeWorkflow {
            intent_id,
            workflow_id,
            lease,
        } => {
            validate_tui_id(&intent_id, "intent")?;
            match port.resume_workflow(&workflow_id, &intent_id, &lease) {
                Ok(()) => {}
                Err(TuiMutationFailure::StaleSelection) => {
                    return stale_selection_outcome(&workflow_id)
                }
                Err(TuiMutationFailure::ResumeInconclusiveEffect) => {
                    return exact_tui_outcome(
                        TuiOutcomeCode::ResumeInconclusiveEffect,
                        TuiOutcomeContext {
                            workflow_id: Some(&workflow_id),
                            phase: Some("verification-started"),
                            ..TuiOutcomeContext::default()
                        },
                    )
                }
                Err(TuiMutationFailure::ResumeCorruptState) => {
                    return exact_tui_outcome(
                        TuiOutcomeCode::ResumeCorruptState,
                        TuiOutcomeContext {
                            workflow_id: Some(&workflow_id),
                            ..TuiOutcomeContext::default()
                        },
                    )
                }
                Err(failure) => return Err(unexpected_or_other("resume workflow", failure)),
            }
            exact_tui_outcome(
                TuiOutcomeCode::ResumeAccepted,
                TuiOutcomeContext {
                    intent_id: Some(&intent_id),
                    workflow_id: Some(&workflow_id),
                    ..TuiOutcomeContext::default()
                },
            )
        }
        TuiIntent::CancelWorkflow {
            intent_id,
            workflow_id,
            lease,
        } => {
            validate_tui_id(&intent_id, "intent")?;
            match port.cancel_workflow(&workflow_id, &intent_id, &lease) {
                Ok(()) => {}
                Err(TuiMutationFailure::StaleSelection) => {
                    return stale_selection_outcome(&workflow_id)
                }
                Err(TuiMutationFailure::CancelNoActiveWorkflow) => {
                    return exact_tui_outcome(
                        TuiOutcomeCode::CancelNoActiveWorkflow,
                        TuiOutcomeContext::default(),
                    )
                }
                Err(TuiMutationFailure::CancelTerminal(phase)) => {
                    return exact_tui_outcome(
                        TuiOutcomeCode::CancelTerminalBlocked,
                        TuiOutcomeContext {
                            workflow_id: Some(&workflow_id),
                            phase: Some(&phase),
                            ..TuiOutcomeContext::default()
                        },
                    )
                }
                Err(TuiMutationFailure::RollbackConflict) => {
                    return exact_tui_outcome(
                        TuiOutcomeCode::RollbackConflict,
                        TuiOutcomeContext {
                            intent_id: Some(&intent_id),
                            workflow_id: Some(&workflow_id),
                            ..TuiOutcomeContext::default()
                        },
                    )
                }
                Err(failure) => return Err(unexpected_or_other("cancel workflow", failure)),
            }
            exact_tui_outcome(
                TuiOutcomeCode::CancelAccepted,
                TuiOutcomeContext {
                    intent_id: Some(&intent_id),
                    workflow_id: Some(&workflow_id),
                    ..TuiOutcomeContext::default()
                },
            )
        }
        TuiIntent::SelectSession {
            intent_id,
            session_id,
            lease,
        }
        | TuiIntent::ResumeSession {
            intent_id,
            session_id,
            lease,
        } => {
            validate_tui_id(&intent_id, "intent")?;
            if port
                .resume_session(&session_id, &intent_id, &lease)?
                .is_none()
            {
                return stale_selection_outcome(&session_id);
            }
            exact_tui_outcome(
                TuiOutcomeCode::ResumeAccepted,
                TuiOutcomeContext {
                    intent_id: Some(&intent_id),
                    workflow_id: Some(&session_id),
                    ..TuiOutcomeContext::default()
                },
            )
        }
    }
}

fn stale_selection_outcome(workflow_id: &str) -> Result<TuiOutcome, AppError> {
    exact_tui_outcome(
        TuiOutcomeCode::ResumeStaleSelection,
        TuiOutcomeContext {
            workflow_id: Some(workflow_id),
            ..TuiOutcomeContext::default()
        },
    )
}

fn secret_refresh_only(intent_id: &str) -> TuiOutcome {
    exact_tui_outcome(
        TuiOutcomeCode::SecretRefreshOnly,
        TuiOutcomeContext {
            intent_id: Some(intent_id),
            ..TuiOutcomeContext::default()
        },
    )
    .expect("validated TUI intent IDs always produce the refresh-only outcome")
}

fn unexpected_or_other(operation: &str, failure: TuiMutationFailure) -> AppError {
    match failure {
        TuiMutationFailure::Other(error) => error,
        _ => AppError::runtime(format!("TUI mutation adapter contract 불일치: {operation}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surfaces::tui::runtime_bridge::ObservedWorkflow;

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
}
