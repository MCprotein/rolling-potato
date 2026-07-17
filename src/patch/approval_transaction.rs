use super::*;

pub(super) fn prepared_approval_receipt_exists(
    record: &ProposalRecord,
    workflow: &state::WorkflowRecord,
    intent_id: &str,
) -> Result<bool, AppError> {
    let expected_types = [
        "runtime.intent.accepted",
        "workflow.checkpoint",
        "patch.apply.approved",
        "hook.dispatched",
        "hook.dispatched",
        "hook.dispatched",
        "hook.dispatched",
        "patch.applied",
        "transcript.recorded",
        "workflow.checkpoint",
    ];
    let e0_details = format!(
        "intent_id={intent_id} intent_kind=approve-patch workflow_id={} proposal_id={}",
        workflow.workflow_id, record.proposal_id
    );
    let events = ledger::read_runtime_events()?;
    let Some(start) = events.iter().position(|event| {
        event.event_type == "runtime.intent.accepted"
            && event.project_id == workflow.project_id
            && event.session_id == workflow.session_id
            && event.details == e0_details
    }) else {
        return Ok(false);
    };
    let Some(receipt) = events.get(start..start + expected_types.len()) else {
        return Ok(false);
    };
    if receipt
        .iter()
        .zip(expected_types)
        .any(|(event, expected)| event.event_type != expected)
    {
        return Ok(false);
    }
    let e7 = &receipt[7];
    let e9 = &receipt[9];
    Ok(e7
        .details
        .contains(&format!("proposal_id={}", record.proposal_id))
        && e7
            .details
            .contains(&format!("applied_sha256={}", record.proposed_sha256))
        && e9.details.contains(&format!(
            "workflow_id={} revision={} artifact_hash={}",
            workflow.workflow_id, workflow.revision, workflow.artifact_hash
        )))
}

struct ApprovalSourcePreflight {
    relative_path: String,
    before: String,
    source_install: transition::SourceInstallV1,
}

pub(super) fn approve_prepared_skill_transaction(
    record: ProposalRecord,
    observed_workflow: state::WorkflowRecord,
    intent_id: &str,
    expected_lease: Option<&SelectionLease>,
) -> Result<ApprovalDispatch, AppError> {
    let identity = ledger::validated_current_identity()?;
    let current_lease = state::current_state_lease_view()?;
    let observed_ledger = ledger::validated_ledger_binding()?;
    let source = prepare_approval_source(&record, intent_id)?;

    let transition_guard = transition::TransitionGuard::acquire_for(
        &identity.project_id,
        transition::CurrentStateIntent::ApprovePatch,
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
            "prepared approval workflow가 lock 획득 전에 변경되었습니다.",
        ));
    }
    validate_workflow_binding(&current, &record)?;
    let mut runtime = workflow_skill_runtime(&current)?.ok_or_else(|| {
        AppError::blocked("prepared approval은 registered built-in skill workflow가 필요합니다.")
    })?;
    validate_skill_verification(&runtime.active_skill_id, &record.verification_command)?;
    validate_failing_test_before(&current, &runtime)?;
    if runtime.state != skill::SkillState::AwaitingApproval {
        return Err(AppError::blocked(format!(
            "skill side effect 차단\n- workflow phase: {}\n- skill state: {}\n- expected skill state: awaiting-approval",
            current.phase,
            runtime.state.label()
        )));
    }
    if state::current_state_lease_view_under_transition()? != current_lease {
        return Err(AppError::blocked(
            "prepared approval current-state lease가 lock 획득 전에 변경되었습니다.",
        ));
    }

    let writer = ledger::LedgerWriterGuard::acquire()?;
    let ledger_binding = writer.binding()?;
    if ledger_binding != observed_ledger {
        return Err(AppError::blocked(
            "prepared approval ledger head가 lock 획득 전에 변경되었습니다.",
        ));
    }

    let mut approved = current.clone();
    approved.phase = "approved".to_string();
    approved.approval_state = "approved".to_string();
    let r1 = workflow_guard.prepare_revision(&current, approved)?;

    let e0 = ledger::new_event_for(
        &identity,
        "runtime.intent.accepted",
        "interactive runtime intent accepted",
        &format!(
            "intent_id={intent_id} intent_kind=approve-patch workflow_id={} proposal_id={}",
            current.workflow_id, record.proposal_id
        ),
    );
    let e2 = ledger::new_event_for(
        &identity,
        "patch.apply.approved",
        "patch apply approval durably accepted",
        &format!(
            "intent_id={intent_id} workflow_id={} proposal_id={} path={} original_sha256={} proposed_sha256={}",
            current.workflow_id,
            record.proposal_id,
            record.relative_path,
            record.original_sha256,
            record.proposed_sha256
        ),
    );
    let e3 = prepare_transaction_hook_event(
        &r1.record,
        &mut runtime,
        "pre_tool_call",
        "apply_patch",
        &identity,
    )?;
    let e4 = prepare_transaction_hook_event(
        &r1.record,
        &mut runtime,
        "pre_patch_apply",
        "apply_patch",
        &identity,
    )?;
    let e5 = prepare_transaction_hook_event(
        &r1.record,
        &mut runtime,
        "post_patch_apply",
        "apply_patch",
        &identity,
    )?;
    let e6 = prepare_transaction_hook_event(
        &r1.record,
        &mut runtime,
        "post_tool_result",
        "apply_patch",
        &identity,
    )?;
    runtime.record_stop_criterion("patch_applied");
    runtime.transition(skill::SkillState::AwaitingVerification)?;
    let e7 = ledger::new_event_for(
        &identity,
        "patch.applied",
        "approved patch applied",
        &format!(
            "proposal_id={} path={} original_sha256={} applied_sha256={} verification=not-requested",
            record.proposal_id,
            record.relative_path,
            record.original_sha256,
            record.proposed_sha256
        ),
    );
    let source_pointer = crate::context::SourcePointer {
        path: source.relative_path.clone(),
        stable_ref: format!("{}:1", source.relative_path),
        chars: 0,
        fingerprint: record.proposed_sha256.clone(),
        snippet: String::new(),
    };
    let transcript = transcript::prepare_no_stream_tool_turn(
        &r1.record,
        &e7.event_id,
        &format!(
            "patch applied: proposal_id={} path={} original_sha256={} applied_sha256={}",
            record.proposal_id,
            record.relative_path,
            record.original_sha256,
            record.proposed_sha256
        ),
        &[source_pointer],
    )?;
    let verification_plaintext = issue_approval_token()?;
    let mut pending = r1.record.clone();
    pending.phase = "pending-verification-approval".to_string();
    pending.approval_state = "applied".to_string();
    pending.verification_credential_hash = sha256_text(&verification_plaintext);
    let verification_token = OneShotSecret::new(verification_plaintext)?;
    pending.verification_approval_state = "pending".to_string();
    pending.result_summary = "patch applied; verification approval pending".to_string();
    runtime.store_in_workflow(&mut pending);
    let r2 = workflow_guard.prepare_revision(&r1.record, pending)?;

    let semantic_events = vec![
        e0,
        r1.event.clone(),
        e2,
        e3,
        e4,
        e5,
        e6,
        e7.clone(),
        transcript.event.clone(),
        r2.event.clone(),
    ];
    let planned = writer.plan_events(&semantic_events)?;
    let final_binding = ledger::LedgerBinding {
        event_count: planned[9].ordinal,
        event_id: Some(planned[9].event.event_id.clone()),
        event_hash: planned[9].event_hash.clone(),
    };
    let current_image = state::prepare_current_image(&r2.record, &final_binding)?;
    let mut bundle = transition::prepare_source_bundle_with_context(
        intent_id,
        Some(&current.workflow_id),
        source.source_install,
        source.before.as_bytes(),
        record.proposed_content.as_bytes(),
        transition::PreparedBundleContext {
            identity: &identity,
            lease: &current_lease,
            ledger_binding,
        },
    )?;
    transition::bind_planned_events(&mut bundle, &planned)?;
    let lag = transition::prepare_projection_lag_member(intent_id, &planned)?;
    let members =
        prepared_approval_members(&r1, &r2, &transcript, &current_image, lag, &semantic_events);
    transition::bind_additional_members(&mut bundle, members)?;
    state::transition_project_current_state_prepared_approval(state::PreparedApprovalTransition {
        transition_guard: Some(&transition_guard),
        workflow_guard: &workflow_guard,
        writer: &writer,
        planned: &planned,
        bundle: &bundle,
        r1: &r1,
        r2: &r2,
        transcript: &transcript,
        current: &current_image,
        events: &semantic_events,
    })?;
    let rollback_path = transition::resolve_prepared_project_path(
        &bundle
            .source_install
            .as_ref()
            .ok_or_else(|| AppError::blocked("prepared approval source_install_v1 누락"))?
            .rollback_final,
    )?;
    Ok(ApprovalDispatch {
        report: format!(
        "patch approve\n- status: applied-awaiting-verification\n- proposal id: {}\n- path: {}\n- approval token: accepted\n- applied sha256: {}\n- rollback record: {}\n- verification command: {}\n- verification approval: required\n- ledger event: {}\n- intent: {}\n- boundary: exact prepared journal과 E0..E9를 수렴한 뒤 patch만 적용했으며 verification command는 아직 실행하지 않았습니다.",
        record.proposal_id,
        source.relative_path,
        record.proposed_sha256,
        rollback_path.display(),
        ledger::redact_text(&record.verification_command),
        e7.event_id,
        intent_id,
        ),
        verification_credential: Some(verification_token),
    })
}

fn prepare_transaction_hook_event(
    workflow: &state::WorkflowRecord,
    runtime: &mut skill::SkillRuntimeState,
    hook: &str,
    tool: &str,
    identity: &ledger::RuntimeIdentity,
) -> Result<ledger::LedgerEvent, AppError> {
    let mode = skill::find_skill(&runtime.active_skill_id)
        .map(|manifest| manifest.mode)
        .unwrap_or("unknown");
    let (_, event) = hooks::prepare_native_lifecycle_event(
        hooks::HookInput {
            hook,
            workflow_id: Some(&workflow.workflow_id),
            active_skill_id: Some(&runtime.active_skill_id),
            mode,
            payload: tool,
        },
        matches!(hook, "pre_tool_call" | "post_tool_result").then_some(tool),
        identity,
    )?;
    runtime.record_hook(hook)?;
    Ok(event)
}

fn prepared_approval_members(
    r1: &state::PreparedWorkflowRevision,
    r2: &state::PreparedWorkflowRevision,
    transcript: &transcript::PreparedTranscriptTurn,
    current: &state::PreparedCurrentImage,
    lag: transition::PreparedMember,
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
                  expected_type: &str,
                  expected_identity: Option<String>,
                  role| PreparedMember {
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
        expected_identity,
        readonly: false,
        mode: 0o600,
        ownership: None,
        semantic_role_rank: role,
    };
    vec![
        member(
            PreparedMemberKind::ToolOutput,
            transcript.tool_stored_path.clone(),
            1,
            transcript.tool_artifact_id.clone(),
            Some(events[7].event_id.clone()),
            events[7].event_id.clone(),
            transcript.tool_bytes.clone(),
            "absent",
            None,
            0,
        ),
        member(
            PreparedMemberKind::TranscriptV2,
            transcript.transcript_stored_path.clone(),
            2,
            transcript.record.record_id.clone(),
            Some(transcript.tool_artifact_id.clone()),
            events[8].event_id.clone(),
            transcript.transcript_bytes.clone(),
            "absent",
            None,
            0,
        ),
        member(
            PreparedMemberKind::WorkflowSnapshot,
            r1.snapshot_stored_path.clone(),
            4,
            r1.snapshot_member_id.clone(),
            None,
            events[1].event_id.clone(),
            r1.snapshot_bytes.clone(),
            "absent",
            None,
            0,
        ),
        member(
            PreparedMemberKind::WorkflowSnapshot,
            r2.snapshot_stored_path.clone(),
            4,
            r2.snapshot_member_id.clone(),
            None,
            events[9].event_id.clone(),
            r2.snapshot_bytes.clone(),
            "absent",
            None,
            1,
        ),
        member(
            PreparedMemberKind::WorkflowPointer,
            r1.pointer_stored_path.clone(),
            4,
            r1.pointer_member_id.clone(),
            Some(r1.snapshot_member_id.clone()),
            events[1].event_id.clone(),
            r1.pointer_bytes.clone(),
            "file",
            None,
            0,
        ),
        member(
            PreparedMemberKind::WorkflowPointer,
            r2.pointer_stored_path.clone(),
            4,
            r2.pointer_member_id.clone(),
            Some(r2.snapshot_member_id.clone()),
            events[9].event_id.clone(),
            r2.pointer_bytes.clone(),
            "file",
            None,
            1,
        ),
        member(
            PreparedMemberKind::CurrentImage,
            current.stored_path.clone(),
            2,
            current.artifact_id.clone(),
            Some(r2.snapshot_member_id.clone()),
            events[9].event_id.clone(),
            current.bytes.clone(),
            "file",
            None,
            0,
        ),
        lag,
    ]
}

mod recovery;

pub(crate) use recovery::{recover_prepared_approval_bundle, recover_prepared_verification_bundle};

fn prepare_approval_source(
    record: &ProposalRecord,
    intent_id: &str,
) -> Result<ApprovalSourcePreflight, AppError> {
    let target = resolve_target_for("patch approve", &record.relative_path)?;
    let read_decision = policy::classify_path(PathMode::Read, &target.relative_path)?;
    let write_decision = policy::classify_path(PathMode::Write, &target.relative_path)?;
    if read_decision.decision != Decision::Allow || write_decision.decision == Decision::Deny {
        return Err(AppError::blocked(
            "prepared patch approve source policy가 allow가 아닙니다.",
        ));
    }
    let metadata = fs::metadata(&target.absolute_path)
        .map_err(|err| AppError::blocked(format!("prepared patch target metadata 실패: {err}")))?;
    if !metadata.is_file() || metadata.len() > MAX_PATCH_FILE_BYTES {
        return Err(AppError::blocked(
            "prepared patch target type/size boundary 불일치",
        ));
    }
    let before = fs::read_to_string(&target.absolute_path)
        .map_err(|err| AppError::blocked(format!("prepared patch target read 실패: {err}")))?;
    let before_hash = sha256_text(&before);
    if before_hash != record.original_sha256
        || sha256_text(&record.proposed_content) != record.proposed_sha256
    {
        return Err(AppError::blocked(
            "prepared patch source/proposal hash binding 불일치",
        ));
    }
    let source_install = transition::prepare_source_install_v1(
        intent_id,
        &record.proposal_id,
        &target.absolute_path,
        before.as_bytes(),
        record.proposed_content.as_bytes(),
    )?;
    Ok(ApprovalSourcePreflight {
        relative_path: target.relative_path,
        before,
        source_install,
    })
}
