use super::*;

pub(crate) fn recover_prepared_approval_bundle(
    bundle: &transition::PreparedSourceBundle,
    journal: &Path,
) -> Result<(), AppError> {
    let expected_event_types = [
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
    if bundle.additional_members.len() != 8
        || bundle.semantic_events.len() != expected_event_types.len()
        || bundle
            .semantic_events
            .iter()
            .zip(expected_event_types)
            .any(|(event, expected)| event.event_type != expected)
    {
        return Err(AppError::blocked(
            "prepared approval recovery exact E0..E9 shape 불일치",
        ));
    }
    let workflow_id = bundle
        .workflow_id
        .as_deref()
        .ok_or_else(|| AppError::blocked("prepared approval recovery workflow 누락"))?;
    let events = &bundle.semantic_events;
    let members = &bundle.additional_members;
    let planned = transition::planned_events(bundle)?;
    let r1 = state::decode_prepared_workflow_revision(
        workflow_id,
        &members[2],
        &members[4],
        &events[1],
    )?;
    let r2 = state::decode_prepared_workflow_revision(
        workflow_id,
        &members[3],
        &members[5],
        &events[9],
    )?;
    let expected_r2_revision = r1
        .record
        .revision
        .checked_add(1)
        .ok_or_else(|| AppError::blocked("prepared approval R+2 revision overflow"))?;
    if r2.record.revision != expected_r2_revision
        || r2.record.previous_hash != r1.record.artifact_hash
        || r2.record.project_id != bundle.project_id
        || r2.record.session_id != bundle.session_id
        || r1.record.project_id != bundle.project_id
        || r1.record.session_id != bundle.session_id
    {
        return Err(AppError::blocked(
            "prepared approval recovery R+1/R+2 chain 불일치",
        ));
    }
    validate_prepared_approval_semantics(bundle, &r1.record)?;
    let transcript =
        transcript::decode_prepared_no_stream_tool_turn(&members[0], &members[1], &events[8])?;
    if transcript.record.causal_id != events[7].event_id
        || transcript.record.workflow_id != workflow_id
    {
        return Err(AppError::blocked(
            "prepared approval recovery transcript E7 binding 불일치",
        ));
    }
    let final_binding = ledger::LedgerBinding {
        event_count: planned[9].ordinal,
        event_id: Some(planned[9].event.event_id.clone()),
        event_hash: planned[9].event_hash.clone(),
    };
    let current_image = state::decode_prepared_current_image(
        &members[6],
        &r2.record,
        &final_binding,
        &r2.snapshot_member_id,
        &events[9].event_id,
    )?;
    state::validate_current_state_recovery_cas(
        bundle.current_revision,
        &bundle.current_artifact_hash,
        Some(&current_image.bytes),
    )?;
    state::validate_prepared_source_parent(bundle)?;

    let workflow_guard = recovery_context(
        "lock-workflow",
        state::WorkflowCheckpointGuard::acquire(workflow_id),
    )?;
    let predecessor_revision = r1
        .record
        .revision
        .checked_sub(1)
        .ok_or_else(|| AppError::blocked("prepared approval predecessor revision underflow"))?;
    let allowed = [
        (predecessor_revision, r1.record.previous_hash.as_str()),
        (r1.record.revision, r1.record.artifact_hash.as_str()),
        (r2.record.revision, r2.record.artifact_hash.as_str()),
    ];
    let installed = recovery_context(
        "load-workflow",
        workflow_guard.load_recovery_current(&allowed),
    )?;
    let valid_predecessor = installed.revision.checked_add(1) == Some(r1.record.revision)
        && installed.artifact_hash == r1.record.previous_hash;
    if installed != r1.record && installed != r2.record && !valid_predecessor {
        return Err(AppError::blocked(
            "prepared approval recovery workflow predecessor conflict",
        ));
    }
    let writer = recovery_context("lock-ledger", ledger::LedgerWriterGuard::acquire())?;
    recovery_context(
        "prepared-approval-transition",
        state::recover_project_current_state_prepared_approval(
            state::PreparedApprovalTransition {
                transition_guard: None,
                workflow_guard: &workflow_guard,
                writer: &writer,
                planned: &planned,
                bundle,
                r1: &r1,
                r2: &r2,
                transcript: &transcript,
                current: &current_image,
                events,
            },
            journal,
        ),
    )
}

pub(crate) fn recover_prepared_verification_bundle(
    bundle: &transition::PreparedSourceBundle,
    journal: &Path,
) -> Result<(), AppError> {
    let expected_event_types = [
        "runtime.intent.accepted",
        "workflow.checkpoint",
        "patch.verification.approved",
    ];
    if bundle.intent_kind != "approve-verification"
        || bundle.source_install.is_some()
        || bundle.additional_members.len() != 3
        || bundle.semantic_events.len() != expected_event_types.len()
        || bundle
            .semantic_events
            .iter()
            .zip(expected_event_types)
            .any(|(event, expected)| event.event_type != expected)
    {
        return Err(AppError::blocked(
            "prepared verification recovery exact shape 불일치",
        ));
    }
    let workflow_id = bundle
        .workflow_id
        .as_deref()
        .ok_or_else(|| AppError::blocked("prepared verification recovery workflow 누락"))?;
    let events = &bundle.semantic_events;
    let members = &bundle.additional_members;
    let planned = transition::planned_events(bundle)?;
    let revision = state::decode_prepared_workflow_revision(
        workflow_id,
        &members[0],
        &members[1],
        &events[1],
    )?;
    if revision.record.project_id != bundle.project_id
        || revision.record.session_id != bundle.session_id
        || revision.record.phase != "verification-started"
        || revision.record.verification_approval_state != "approved"
    {
        return Err(AppError::blocked(
            "prepared verification workflow semantic binding 불일치",
        ));
    }
    let e0_details = format!(
        "intent_id={} intent_kind=approve-verification workflow_id={} proposal_id={}",
        bundle.intent_id, revision.record.workflow_id, revision.record.proposal_id
    );
    let e2_details = format!(
        "intent_id={} workflow_id={} proposal_id={} gate=verification-command revision={} artifact_hash={} command_hash={}",
        bundle.intent_id,
        revision.record.workflow_id,
        revision.record.proposal_id,
        revision.record.revision,
        revision.record.artifact_hash,
        sha256_text(&revision.record.verification_plan),
    );
    if events[0].summary != "interactive runtime intent accepted"
        || events[0].details != e0_details
        || events[2].summary != "verification command approval durably accepted"
        || events[2].details != e2_details
    {
        return Err(AppError::blocked(
            "prepared verification E0/E2 semantic binding 불일치",
        ));
    }
    let runtime = workflow_skill_runtime(&revision.record)?.ok_or_else(|| {
        AppError::blocked("prepared verification active built-in skill manifest 누락")
    })?;
    if runtime.state != skill::SkillState::AwaitingVerification {
        return Err(AppError::blocked(
            "prepared verification skill state binding 불일치",
        ));
    }
    let final_binding = ledger::LedgerBinding {
        event_count: planned[2].ordinal,
        event_id: Some(planned[2].event.event_id.clone()),
        event_hash: planned[2].event_hash.clone(),
    };
    let current_image = state::decode_prepared_current_image(
        &members[2],
        &revision.record,
        &final_binding,
        &revision.snapshot_member_id,
        &events[2].event_id,
    )?;
    state::validate_current_state_recovery_cas(
        bundle.current_revision,
        &bundle.current_artifact_hash,
        Some(&current_image.bytes),
    )?;

    let workflow_guard = recovery_context(
        "verification-lock-workflow",
        state::WorkflowCheckpointGuard::acquire(workflow_id),
    )?;
    let predecessor_revision =
        revision.record.revision.checked_sub(1).ok_or_else(|| {
            AppError::blocked("prepared verification predecessor revision underflow")
        })?;
    let allowed = [
        (predecessor_revision, revision.record.previous_hash.as_str()),
        (
            revision.record.revision,
            revision.record.artifact_hash.as_str(),
        ),
    ];
    let installed = recovery_context(
        "verification-load-workflow",
        workflow_guard.load_recovery_current(&allowed),
    )?;
    let valid_predecessor = installed.revision.checked_add(1) == Some(revision.record.revision)
        && installed.artifact_hash == revision.record.previous_hash;
    if installed != revision.record && !valid_predecessor {
        return Err(AppError::blocked(
            "prepared verification recovery workflow predecessor conflict",
        ));
    }
    let writer = recovery_context(
        "verification-lock-ledger",
        ledger::LedgerWriterGuard::acquire(),
    )?;
    recovery_context(
        "prepared-verification-transition",
        state::recover_project_current_state_prepared_verification(
            state::PreparedVerificationTransition {
                transition_guard: None,
                workflow_guard: &workflow_guard,
                writer: &writer,
                planned: &planned,
                bundle,
                revision: &revision,
                current: &current_image,
                events,
            },
            journal,
        ),
    )
}

fn validate_prepared_approval_semantics(
    bundle: &transition::PreparedSourceBundle,
    approved: &state::WorkflowRecord,
) -> Result<(), AppError> {
    let events = &bundle.semantic_events;
    let source_install = bundle
        .source_install
        .as_ref()
        .ok_or_else(|| AppError::blocked("prepared approval source_install_v1 누락"))?;
    let identity = ledger::RuntimeIdentity {
        project_id: bundle.project_id.clone(),
        session_id: bundle.session_id.clone(),
        project_root: paths::project_root().display().to_string(),
    };
    let e0_details = format!(
        "intent_id={} intent_kind=approve-patch workflow_id={} proposal_id={}",
        bundle.intent_id, approved.workflow_id, approved.proposal_id
    );
    let e2_details = format!(
        "intent_id={} workflow_id={} proposal_id={} path={} original_sha256={} proposed_sha256={}",
        bundle.intent_id,
        approved.workflow_id,
        approved.proposal_id,
        approved.source_path,
        approved.before_hash,
        approved.after_hash
    );
    let e7_details = format!(
        "proposal_id={} path={} original_sha256={} applied_sha256={} verification=not-requested",
        approved.proposal_id, approved.source_path, approved.before_hash, approved.after_hash
    );
    if approved.proposal_id.is_empty()
        || source_install.target.path != approved.source_path
        || source_install.before_sha256 != approved.before_hash
        || source_install.proposed_sha256 != approved.after_hash
        || events[0].summary != "interactive runtime intent accepted"
        || events[0].details != e0_details
        || events[2].summary != "patch apply approval durably accepted"
        || events[2].details != e2_details
        || events[7].summary != "approved patch applied"
        || events[7].details != e7_details
    {
        return Err(AppError::blocked(
            "prepared approval E0/E2/E7 source/workflow semantic binding 불일치",
        ));
    }
    let manifest = skill::find_skill(&approved.active_skill_id).ok_or_else(|| {
        AppError::blocked("prepared approval active built-in skill manifest 누락")
    })?;
    for (index, hook, tool) in [
        (3, "pre_tool_call", Some("apply_patch")),
        (4, "pre_patch_apply", None),
        (5, "post_patch_apply", None),
        (6, "post_tool_result", Some("apply_patch")),
    ] {
        hooks::validate_prepared_native_lifecycle_event(
            hooks::HookInput {
                hook,
                workflow_id: Some(&approved.workflow_id),
                active_skill_id: Some(&approved.active_skill_id),
                mode: manifest.mode,
                payload: "apply_patch",
            },
            tool,
            &identity,
            &events[index],
        )?;
    }
    Ok(())
}

fn recovery_context<T>(stage: &str, result: Result<T, AppError>) -> Result<T, AppError> {
    result.map_err(|error| AppError {
        code: error.code,
        message: format!(
            "prepared approval recovery stage 실패\n- stage: {stage}\n- error: {}",
            error.message
        ),
    })
}
