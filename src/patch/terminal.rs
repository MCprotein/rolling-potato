use super::*;

pub fn cancel_workflow_report(workflow_id: &str) -> Result<String, AppError> {
    let intent_id = format!("intent-cancel-{}", workflow_id);
    let workflow = cancel_workflow_transaction(workflow_id, &intent_id, None).map_err(|error| {
        if let Some(reason) = error.message.strip_prefix("internal.rollback-conflict:") {
            AppError::blocked(format!(
                "workflow cancel 차단\n- 이유: 적용된 source를 안전하게 복원하지 못했습니다.\n- rollback: {reason}\n- pointer: 유지"
            ))
        } else if let Some(phase) = error.message.strip_prefix("internal.cancel-terminal:") {
            AppError::blocked(format!(
                "cancel 차단\n- 이유: terminal workflow는 취소할 수 없습니다.\n- phase: {phase}"
            ))
        } else {
            error
        }
    })?;
    Ok(format!(
        "workflow 취소 완료\n- workflow id: {}\n- phase: cancelled\n- source 복원: 검증됨 또는 적용 전\n- backend/verification 재실행: 없음",
        workflow.workflow_id
    ))
}

pub(crate) fn cancel_workflow_for_tui(
    workflow_id: &str,
    intent_id: &str,
    lease: &SelectionLease,
) -> Result<(), AppError> {
    cancel_workflow_transaction(workflow_id, intent_id, Some(lease)).map(|_| ())
}

fn cancel_workflow_transaction(
    workflow_id: &str,
    intent_id: &str,
    expected_lease: Option<&SelectionLease>,
) -> Result<state::WorkflowRecord, AppError> {
    validate_outcome_id(intent_id, "intent")?;
    let (observed, _approval_lock) = load_workflow_under_approval_lock(workflow_id)?;
    if observed.phase == "complete" {
        return Err(AppError::blocked(format!(
            "internal.cancel-terminal:{}",
            observed.phase
        )));
    }
    if matches!(observed.phase.as_str(), "failed" | "cancelled") {
        return Err(AppError::blocked(format!(
            "internal.cancel-terminal:{}",
            observed.phase
        )));
    }
    let identity = ledger::validated_current_identity()?;
    let transition_guard = transition::TransitionGuard::acquire_for(
        &identity.project_id,
        transition::CurrentStateIntent::Cancel,
    )?;
    if let Some(lease) = expected_lease {
        if !state::tui_lease_matches_workflow_under_transition(lease, workflow_id)? {
            return Err(stale_selection_error());
        }
    }
    let workflow_guard = state::WorkflowCheckpointGuard::acquire(workflow_id)?;
    let current = workflow_guard.load_current()?;
    if current != observed {
        return Err(stale_selection_error());
    }
    let source = if workflow_has_applied_source(&current) {
        let record = load_bound_proposal(&current)?;
        prepare_terminal_rollback_source(&record, intent_id, false)?
    } else {
        None
    };
    let mut terminal = current.clone();
    terminal.phase = "cancelled".to_string();
    terminal.failure_reason = "user-cancelled".to_string();
    terminal.approval_state = "cancelled".to_string();
    terminal.verification_approval_state = "cancelled".to_string();
    if let Some(mut runtime) = workflow_skill_runtime(&terminal)? {
        runtime.transition(skill::SkillState::Cancelled)?;
        runtime.store_in_workflow(&mut terminal);
    }
    state::transition_project_current_state_prepared_terminal_action(
        &transition_guard,
        &workflow_guard,
        state::TerminalActionRequest {
            intent_id,
            intent_kind: "cancel-workflow",
            identity: &identity,
            before: &current,
            terminal,
            audit_event_type: "workflow.user-cancelled",
            audit_summary: "workflow cancelled by user",
            audit_details: "reason=user-cancelled",
            source,
        },
    )
}

#[cfg(test)]
pub fn deny_pending_gate(workflow_id: &str, intent_id: &str) -> Result<TuiOutcome, AppError> {
    deny_pending_gate_transaction(workflow_id, intent_id, None)
}

pub(crate) fn deny_pending_gate_for_tui(
    workflow_id: &str,
    intent_id: &str,
    gate_id: &str,
    gate_kind: TuiGateKind,
    lease: &SelectionLease,
) -> Result<TuiOutcome, AppError> {
    deny_pending_gate_transaction(workflow_id, intent_id, Some((gate_id, gate_kind, lease)))
}

fn deny_pending_gate_transaction(
    workflow_id: &str,
    intent_id: &str,
    expected: Option<(&str, TuiGateKind, &SelectionLease)>,
) -> Result<TuiOutcome, AppError> {
    validate_outcome_id(intent_id, "intent")?;
    let (observed, _approval_lock) = load_workflow_under_approval_lock(workflow_id)?;
    validate_outcome_id(&observed.workflow_id, "workflow")?;
    if observed.phase == "cancelled"
        && observed.failure_reason == "user-denied-patch"
        && terminal_action_receipt_exists(intent_id, workflow_id, "patch.apply.denied")?
    {
        validate_stored_terminal_gate(&observed, expected, TuiGateKind::PatchApply)?;
        return deny_patch_accepted(intent_id, &observed.workflow_id);
    }
    if observed.phase == "cancelled"
        && observed.failure_reason == "user-denied-verification"
        && terminal_action_receipt_exists(intent_id, workflow_id, "patch.verification.denied")?
    {
        validate_stored_terminal_gate(&observed, expected, TuiGateKind::VerificationCommand)?;
        return deny_verification_accepted(intent_id, &observed.workflow_id);
    }
    let identity = ledger::validated_current_identity()?;
    let transition_guard = transition::TransitionGuard::acquire_for(
        &identity.project_id,
        transition::CurrentStateIntent::Cancel,
    )?;
    let workflow_guard = state::WorkflowCheckpointGuard::acquire(workflow_id)?;
    let workflow = workflow_guard.load_current()?;
    if workflow != observed {
        return Err(stale_selection_error());
    }
    if workflow.is_terminal() {
        if let Some((gate_id, gate_kind, lease)) = expected {
            if !state::tui_lease_matches_terminal_selection_under_transition(lease, workflow_id)? {
                return Err(stale_selection_error());
            }
            validate_terminal_gate(&workflow, gate_id, gate_kind)?;
        }
        return exact_tui_outcome(
            TuiOutcomeCode::DenyBlockedTerminalState,
            TuiOutcomeContext {
                intent_id: Some(intent_id),
                workflow_id: Some(&workflow.workflow_id),
                phase: Some(&workflow.phase),
                ..TuiOutcomeContext::default()
            },
        );
    }
    if let Some((_, _, lease)) = expected {
        if !state::tui_lease_matches_workflow_under_transition(lease, workflow_id)? {
            return Err(stale_selection_error());
        }
    }
    if let Some((gate_id, gate_kind, _)) = expected {
        validate_terminal_gate(&workflow, gate_id, gate_kind)?;
    }
    match denial_phase_outcome_code(&workflow.phase) {
        Some(TuiOutcomeCode::DenyPatchAccepted) => {
            let mut terminal = workflow.clone();
            terminal.phase = "cancelled".to_string();
            terminal.failure_reason = "user-denied-patch".to_string();
            terminal.approval_state = "denied".to_string();
            terminal.verification_approval_state = "not-issued".to_string();
            if let Some(mut skill_runtime) = workflow_skill_runtime(&terminal)? {
                skill_runtime.transition(skill::SkillState::Cancelled)?;
                skill_runtime.store_in_workflow(&mut terminal);
            }
            let committed = state::transition_project_current_state_prepared_terminal_action(
                &transition_guard,
                &workflow_guard,
                state::TerminalActionRequest {
                    intent_id,
                    intent_kind: "deny-patch",
                    identity: &identity,
                    before: &workflow,
                    terminal,
                    audit_event_type: "patch.apply.denied",
                    audit_summary: "patch apply approval denied",
                    audit_details: "gate=patch-apply effect=none",
                    source: None,
                },
            )?;
            deny_patch_accepted(intent_id, &committed.workflow_id)
        }
        Some(TuiOutcomeCode::DenyVerificationRolledBack) => {
            let record = load_bound_proposal(&workflow)?;
            let source = match prepare_terminal_rollback_source(&record, intent_id, true) {
                Ok(Some(source)) => source,
                Ok(None) => {
                    return Err(AppError::blocked(
                        "prepared verification denial rollback receipt 누락",
                    ))
                }
                Err(error) if error.message.starts_with("internal.rollback-conflict:") => {
                    return exact_tui_outcome(
                        TuiOutcomeCode::RollbackConflict,
                        TuiOutcomeContext {
                            intent_id: Some(intent_id),
                            workflow_id: Some(&workflow.workflow_id),
                            ..TuiOutcomeContext::default()
                        },
                    )
                }
                Err(error) => return Err(error),
            };
            let mut terminal = workflow.clone();
            terminal.phase = "cancelled".to_string();
            terminal.failure_reason = "user-denied-verification".to_string();
            terminal.approval_state = "applied-then-rolled-back".to_string();
            terminal.verification_approval_state = "denied".to_string();
            if let Some(mut skill_runtime) = workflow_skill_runtime(&terminal)? {
                skill_runtime.transition(skill::SkillState::Cancelled)?;
                skill_runtime.store_in_workflow(&mut terminal);
            }
            let committed = state::transition_project_current_state_prepared_terminal_action(
                &transition_guard,
                &workflow_guard,
                state::TerminalActionRequest {
                    intent_id,
                    intent_kind: "deny-verification",
                    identity: &identity,
                    before: &workflow,
                    terminal,
                    audit_event_type: "patch.verification.denied",
                    audit_summary: "verification approval denied and source rolled back",
                    audit_details: "gate=verification-command rollback=restored",
                    source: Some(source),
                },
            )?;
            deny_verification_accepted(intent_id, &committed.workflow_id)
        }
        Some(TuiOutcomeCode::DenyBlockedNotPending) => {
            exact_tui_outcome(
                TuiOutcomeCode::DenyBlockedNotPending,
                TuiOutcomeContext {
                    intent_id: Some(intent_id),
                    workflow_id: Some(&workflow.workflow_id),
                    phase: Some(&workflow.phase),
                    ..TuiOutcomeContext::default()
                },
            )
        }
        Some(TuiOutcomeCode::DenyBlockedTerminalState) => exact_tui_outcome(
            TuiOutcomeCode::DenyBlockedTerminalState,
            TuiOutcomeContext {
                intent_id: Some(intent_id),
                workflow_id: Some(&workflow.workflow_id),
                phase: Some(&workflow.phase),
                ..TuiOutcomeContext::default()
            },
        ),
        Some(other) => Err(AppError::blocked(format!(
            "승인 거부 차단\n- code: deny.corrupt-state\n- mapped outcome: {}\n- 동작: 허용되지 않은 denial outcome을 실행하지 않았습니다.",
            other.as_str()
        ))),
        None => Err(AppError::blocked(
            "승인 거부 차단\n- code: deny.corrupt-state\n- 동작: 알 수 없는 workflow phase를 출력하거나 변경하지 않았습니다.",
        )),
    }
}

pub(crate) fn denial_phase_outcome_code(phase: &str) -> Option<TuiOutcomeCode> {
    match phase {
        "pending-approval" => Some(TuiOutcomeCode::DenyPatchAccepted),
        "pending-verification-approval" => Some(TuiOutcomeCode::DenyVerificationRolledBack),
        "approved" | "verification-approved" | "verification-started" | "verified" => {
            Some(TuiOutcomeCode::DenyBlockedNotPending)
        }
        "complete" | "failed" | "cancelled" => Some(TuiOutcomeCode::DenyBlockedTerminalState),
        _ => None,
    }
}

fn deny_patch_accepted(intent_id: &str, workflow_id: &str) -> Result<TuiOutcome, AppError> {
    exact_tui_outcome(
        TuiOutcomeCode::DenyPatchAccepted,
        TuiOutcomeContext {
            intent_id: Some(intent_id),
            workflow_id: Some(workflow_id),
            ..TuiOutcomeContext::default()
        },
    )
}

fn deny_verification_accepted(intent_id: &str, workflow_id: &str) -> Result<TuiOutcome, AppError> {
    exact_tui_outcome(
        TuiOutcomeCode::DenyVerificationRolledBack,
        TuiOutcomeContext {
            intent_id: Some(intent_id),
            workflow_id: Some(workflow_id),
            ..TuiOutcomeContext::default()
        },
    )
}

fn workflow_has_applied_source(workflow: &state::WorkflowRecord) -> bool {
    matches!(
        workflow.phase.as_str(),
        "approved"
            | "pending-verification-approval"
            | "verification-approved"
            | "verification-started"
            | "verified"
    ) || matches!(
        workflow.approval_state.as_str(),
        "applied" | "approved" | "applied-then-rolled-back"
    )
}

fn load_bound_proposal(workflow: &state::WorkflowRecord) -> Result<ProposalRecord, AppError> {
    let proposal_path =
        paths::project_patch_proposals_dir().join(format!("{}.txt", workflow.proposal_id));
    let record = load_proposal_record(&workflow.proposal_id, &proposal_path)?;
    validate_workflow_binding(workflow, &record)?;
    Ok(record)
}

fn prepare_terminal_rollback_source(
    record: &ProposalRecord,
    intent_id: &str,
    require_receipt: bool,
) -> Result<Option<state::PreparedTerminalSource>, AppError> {
    let target = resolve_target_for("terminal rollback", &record.relative_path)?;
    let current = fs::read(&target.absolute_path)
        .map_err(|err| AppError::blocked(format!("terminal rollback target read 실패: {err}")))?;
    let current_hash = sha256_bytes(&current);
    if current_hash != record.proposed_sha256 && current_hash != record.original_sha256 {
        return Err(AppError::blocked(format!(
            "internal.rollback-conflict:target-sha256={current_hash}"
        )));
    }
    if current_hash == record.original_sha256 && !require_receipt {
        return Ok(None);
    }
    let rollback_path = rollback_path_for_record(record)?;
    let original = fs::read(&rollback_path)
        .map_err(|err| AppError::blocked(format!("terminal rollback record read 실패: {err}")))?;
    if sha256_bytes(&original) != record.original_sha256 {
        return Err(AppError::blocked(
            "internal.rollback-conflict:rollback-record-hash",
        ));
    }
    let plan = transition::prepare_source_install_v1(
        intent_id,
        &record.proposal_id,
        &target.absolute_path,
        &current,
        &original,
    )?;
    Ok(Some(state::PreparedTerminalSource {
        plan,
        before: current,
        proposed: original,
    }))
}

fn validate_terminal_gate(
    workflow: &state::WorkflowRecord,
    gate_id: &str,
    gate_kind: TuiGateKind,
) -> Result<(), AppError> {
    validate_outcome_id(gate_id, "gate")?;
    let expected_kind = match (workflow.phase.as_str(), workflow.failure_reason.as_str()) {
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
    if gate_id != workflow.proposal_id || gate_kind != expected_kind {
        return Err(stale_selection_error());
    }
    Ok(())
}

fn validate_stored_terminal_gate(
    workflow: &state::WorkflowRecord,
    expected: Option<(&str, TuiGateKind, &SelectionLease)>,
    expected_kind: TuiGateKind,
) -> Result<(), AppError> {
    if let Some((gate_id, gate_kind, lease)) = expected {
        if gate_id != workflow.proposal_id
            || gate_kind != expected_kind
            || lease.selected_object_id != workflow.workflow_id
        {
            return Err(stale_selection_error());
        }
    }
    Ok(())
}

fn terminal_action_receipt_exists(
    intent_id: &str,
    workflow_id: &str,
    event_type: &str,
) -> Result<bool, AppError> {
    ledger::event_details_match(
        event_type,
        &[("intent_id", intent_id), ("workflow_id", workflow_id)],
    )
}
