use super::*;

pub fn verify_report(proposal_id: &str, token: &str) -> Result<String, AppError> {
    let intent_id = format!("intent-verify-{proposal_id}");
    verify_report_for_intent(proposal_id, token, &intent_id, None)
}

pub(crate) fn verify_for_tui(
    proposal_id: &str,
    token: &str,
    intent_id: &str,
    lease: &SelectionLease,
) -> Result<String, AppError> {
    verify_report_for_intent(proposal_id, token, intent_id, Some(lease))
}

fn verify_report_for_intent(
    proposal_id: &str,
    token: &str,
    intent_id: &str,
    expected_lease: Option<&SelectionLease>,
) -> Result<String, AppError> {
    validate_proposal_id(proposal_id)?;
    validate_outcome_id(intent_id, "intent")?;
    transition::recover_pending_source_bundles()?;
    let _approval_lock = ApprovalLock::acquire(proposal_id)?;
    let active = state::active_workflow_id()?;
    let proposal_path = paths::project_patch_proposals_dir().join(format!("{proposal_id}.txt"));
    let record = load_proposal_record(proposal_id, &proposal_path)?;
    if record.workflow_id.is_empty() {
        return Err(AppError::blocked(
            "patch verify 차단\n- 이유: workflow proposal만 verification을 실행할 수 있습니다.",
        ));
    }
    let mut workflow = state::load_workflow(&record.workflow_id)?;
    if !workflow.is_terminal() && active.as_deref() != Some(record.workflow_id.as_str()) {
        return Err(AppError::blocked(
            "patch verify 차단\n- 이유: active workflow/current pointer가 일치하지 않습니다.",
        ));
    }
    validate_workflow_binding(&workflow, &record)?;
    validate_token_hash(&workflow.verification_credential_hash, token, &record)?;
    if workflow.phase == "complete" {
        validate_completed_workflow(&workflow)?;
        state::clear_terminal_workflow_pointer(&workflow)?;
        return Ok(success_report(&workflow));
    }
    if workflow.phase == "failed" {
        return Err(AppError::blocked(failure_report(&workflow)));
    }
    if workflow.phase != "pending-verification-approval" {
        return Err(AppError::blocked(format!(
            "patch verify 차단\n- 이유: verification approval을 받을 수 없는 phase입니다.\n- phase: {}",
            workflow.phase
        )));
    }
    let plan = build_verification_plan(&record.verification_command)?;
    workflow =
        approve_prepared_verification_transaction(&record, workflow, intent_id, expected_lease)?;
    verification_approval_transaction_fault("after-commit")?;
    continue_approved_workflow(record, Some(workflow), Some(plan))
}

fn approve_prepared_verification_transaction(
    record: &ProposalRecord,
    observed_workflow: state::WorkflowRecord,
    intent_id: &str,
    expected_lease: Option<&SelectionLease>,
) -> Result<state::WorkflowRecord, AppError> {
    let identity = ledger::validated_current_identity()?;
    let current_lease = state::current_state_lease_view()?;
    let observed_ledger = ledger::validated_ledger_binding()?;
    validate_applied_proposal(record)?;

    let transition_guard = transition::TransitionGuard::acquire_for(
        &identity.project_id,
        transition::CurrentStateIntent::ApproveVerification,
    )?;
    if let Some(lease) = expected_lease {
        if !state::tui_lease_matches_workflow_under_transition(
            lease,
            &observed_workflow.workflow_id,
        )? {
            return Err(stale_selection_error());
        }
    }
    let workflow_guard = state::WorkflowCheckpointGuard::acquire(&observed_workflow.workflow_id)?;
    let current = workflow_guard.load_current()?;
    if current != observed_workflow {
        return Err(AppError::blocked(
            "prepared verification workflow가 lock 획득 전에 변경되었습니다.",
        ));
    }
    validate_workflow_binding(&current, record)?;
    if current.phase != "pending-verification-approval"
        || !matches!(
            current.verification_approval_state.as_str(),
            "pending" | "pending-rotated"
        )
    {
        return Err(AppError::blocked(
            "prepared verification approval gate 상태 불일치",
        ));
    }
    let runtime = workflow_skill_runtime(&current)?.ok_or_else(|| {
        AppError::blocked(
            "prepared verification은 registered built-in skill workflow가 필요합니다.",
        )
    })?;
    if runtime.state != skill::SkillState::AwaitingVerification {
        return Err(AppError::blocked(format!(
            "verification side effect 차단\n- skill state: {}\n- expected skill state: awaiting-verification",
            runtime.state.label()
        )));
    }
    validate_skill_verification(&runtime.active_skill_id, &record.verification_command)?;
    validate_failing_test_before(&current, &runtime)?;
    if state::current_state_lease_view_under_transition()? != current_lease {
        return Err(AppError::blocked(
            "prepared verification current-state lease가 lock 획득 전에 변경되었습니다.",
        ));
    }

    let writer = ledger::LedgerWriterGuard::acquire()?;
    let ledger_binding = writer.binding()?;
    if ledger_binding != observed_ledger {
        return Err(AppError::blocked(
            "prepared verification ledger head가 lock 획득 전에 변경되었습니다.",
        ));
    }

    let mut started = current.clone();
    started.phase = "verification-started".to_string();
    started.verification_approval_state = "approved".to_string();
    runtime.store_in_workflow(&mut started);
    let r1 = workflow_guard.prepare_revision(&current, started)?;
    let e0 = ledger::new_event_for(
        &identity,
        "runtime.intent.accepted",
        "interactive runtime intent accepted",
        &format!(
            "intent_id={intent_id} intent_kind=approve-verification workflow_id={} proposal_id={}",
            current.workflow_id, record.proposal_id
        ),
    );
    let e2 = ledger::new_event_for(
        &identity,
        "patch.verification.approved",
        "verification command approval durably accepted",
        &format!(
            "intent_id={intent_id} workflow_id={} proposal_id={} gate=verification-command revision={} artifact_hash={} command_hash={}",
            r1.record.workflow_id,
            record.proposal_id,
            r1.record.revision,
            r1.record.artifact_hash,
            sha256_text(&record.verification_command),
        ),
    );
    let semantic_events = vec![e0, r1.event.clone(), e2];
    let planned = writer.plan_events(&semantic_events)?;
    let final_binding = ledger::LedgerBinding {
        event_count: planned[2].ordinal,
        event_id: Some(planned[2].event.event_id.clone()),
        event_hash: planned[2].event_hash.clone(),
    };
    let current_image =
        state::prepare_current_image_after(&r1.record, current.revision, &final_binding)?;
    let mut bundle = transition::prepare_workflow_bundle_with_context(
        intent_id,
        "approve-verification",
        &current.workflow_id,
        transition::PreparedBundleContext {
            identity: &identity,
            lease: &current_lease,
            ledger_binding,
        },
    )?;
    transition::bind_planned_events(&mut bundle, &planned)?;
    transition::bind_additional_members(
        &mut bundle,
        prepared_verification_members(&r1, &current_image, &semantic_events),
    )?;
    state::transition_project_current_state_prepared_verification(
        state::PreparedVerificationTransition {
            transition_guard: Some(&transition_guard),
            workflow_guard: &workflow_guard,
            writer: &writer,
            planned: &planned,
            bundle: &bundle,
            revision: &r1,
            current: &current_image,
            events: &semantic_events,
        },
    )?;
    Ok(r1.record)
}

fn prepared_verification_members(
    revision: &state::PreparedWorkflowRevision,
    current: &state::PreparedCurrentImage,
    events: &[ledger::LedgerEvent],
) -> Vec<transition::PreparedMember> {
    use transition::{PreparedMember, PreparedMemberBinding, PreparedMemberKind};
    let member = |kind,
                  path: String,
                  schema_version,
                  artifact_id: String,
                  causal_id: Option<String>,
                  event_id: String,
                  bytes_utf8: String,
                  expected_type: &str| PreparedMember {
        kind,
        path,
        schema_version,
        binding: PreparedMemberBinding {
            artifact_id: Some(artifact_id),
            causal_id,
            source_key: None,
            event_id: Some(event_id),
        },
        bytes_utf8,
        expected_type: expected_type.to_string(),
        expected_identity: None,
        readonly: false,
        mode: 0o600,
        ownership: None,
        semantic_role_rank: 0,
    };
    vec![
        member(
            PreparedMemberKind::WorkflowSnapshot,
            revision.snapshot_stored_path.clone(),
            4,
            revision.snapshot_member_id.clone(),
            None,
            events[1].event_id.clone(),
            revision.snapshot_bytes.clone(),
            "absent",
        ),
        member(
            PreparedMemberKind::WorkflowPointer,
            revision.pointer_stored_path.clone(),
            4,
            revision.pointer_member_id.clone(),
            Some(revision.snapshot_member_id.clone()),
            events[1].event_id.clone(),
            revision.pointer_bytes.clone(),
            "file",
        ),
        member(
            PreparedMemberKind::CurrentImage,
            current.stored_path.clone(),
            2,
            current.artifact_id.clone(),
            Some(revision.snapshot_member_id.clone()),
            events[2].event_id.clone(),
            current.bytes.clone(),
            "file",
        ),
    ]
}
