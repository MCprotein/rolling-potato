use crate::foundation::error::AppError;
use crate::surfaces::tui::outcome::{
    exact_tui_outcome, unsupported_source_platform_outcome, validate_tui_id,
    verification_credential_issued, TuiOutcome, TuiOutcomeCode, TuiOutcomeContext,
};
use crate::surfaces::tui::runtime_bridge::{OneShotSecret, SelectionLease, TuiGateKind};

use super::outcome::{secret_refresh_only, stale_selection, unexpected_or_other};
use super::port::{TuiActionPort, TuiMutationFailure};

pub(super) fn approve_patch(
    port: &mut impl TuiActionPort,
    intent_id: String,
    proposal_id: String,
    lease: SelectionLease,
    secret: OneShotSecret,
) -> Result<TuiOutcome, AppError> {
    validate_tui_id(&intent_id, "intent")?;
    if !cfg!(unix) {
        return unsupported_source_platform_outcome(std::env::consts::OS);
    }
    let verification_token =
        match secret.expose(|token| port.approve_patch(&proposal_id, token, &intent_id, &lease)) {
            Ok(token) => token,
            Err(TuiMutationFailure::StaleSelection) => {
                return stale_selection(&lease.selected_object_id)
            }
            Err(failure) => return Err(unexpected_or_other("approve patch", failure)),
        };
    match verification_token {
        Some(credential) => verification_credential_issued(&intent_id, credential),
        None => Ok(secret_refresh_only(&intent_id)),
    }
}

pub(super) fn approve_verification(
    port: &mut impl TuiActionPort,
    intent_id: String,
    proposal_id: String,
    lease: SelectionLease,
    secret: OneShotSecret,
) -> Result<TuiOutcome, AppError> {
    validate_tui_id(&intent_id, "intent")?;
    match secret.expose(|token| port.approve_verification(&proposal_id, token, &intent_id, &lease))
    {
        Ok(()) => {}
        Err(TuiMutationFailure::StaleSelection) => {
            return stale_selection(&lease.selected_object_id)
        }
        Err(failure) => return Err(unexpected_or_other("approve verification", failure)),
    }
    Ok(secret_refresh_only(&intent_id))
}

pub(super) fn deny_pending_gate(
    port: &mut impl TuiActionPort,
    intent_id: String,
    workflow_id: String,
    gate_id: String,
    gate_kind: TuiGateKind,
    lease: SelectionLease,
) -> Result<TuiOutcome, AppError> {
    match port.deny_pending_gate(&workflow_id, &intent_id, &gate_id, gate_kind, &lease) {
        Err(TuiMutationFailure::StaleSelection) => stale_selection(&workflow_id),
        Err(failure) => Err(unexpected_or_other("deny pending gate", failure)),
        Ok(outcome) => Ok(outcome),
    }
}

pub(super) fn resume(
    port: &mut impl TuiActionPort,
    intent_id: String,
    workflow_id: String,
    lease: SelectionLease,
) -> Result<TuiOutcome, AppError> {
    validate_tui_id(&intent_id, "intent")?;
    match port.resume_workflow(&workflow_id, &intent_id, &lease) {
        Ok(()) => {}
        Err(TuiMutationFailure::StaleSelection) => return stale_selection(&workflow_id),
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

pub(super) fn cancel(
    port: &mut impl TuiActionPort,
    intent_id: String,
    workflow_id: String,
    lease: SelectionLease,
) -> Result<TuiOutcome, AppError> {
    validate_tui_id(&intent_id, "intent")?;
    match port.cancel_workflow(&workflow_id, &intent_id, &lease) {
        Ok(()) => {}
        Err(TuiMutationFailure::StaleSelection) => return stale_selection(&workflow_id),
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
