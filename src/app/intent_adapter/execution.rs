use super::{
    agent_loop_prompt, available_context_labels, dispatch_skill_hook, fail_skill_workflow,
    is_non_mutating_action, model_transcript_content, plugin_completion_fault,
    record_non_mutating_outcomes, render_non_mutating_report,
};
use crate::app::context_adapter as context;
use crate::app::extensions_adapter::{plugin, skill};
use crate::app::inference_adapter::backend;
use crate::app::workflow_adapter::{state, transcript};
use crate::foundation::error::AppError;
use crate::runtime_core::patch::intent::{
    display_bool, display_list, display_optional_u32, parse_model_action, plan_action_candidate,
    IntentDecision,
};

const RUN_MAX_TOKENS: u32 = 256;

pub(super) fn run_with_decision(
    request: &str,
    decision: IntentDecision,
    manifest: skill::ResolvedSkillManifest,
) -> Result<String, AppError> {
    if let Some(workflow_id) = state::active_workflow_id()? {
        return crate::app::patch_adapter::resume_workflow_report(&workflow_id);
    }
    backend::preflight_chat_ready()?;
    let identity = crate::app::workflow_adapter::ledger::validated_current_identity()?;
    // Compaction is derived-state maintenance. Any failure falls back to the
    // existing bounded recent-turn resume path and must not block the user run.
    let _auto_compaction = context::compact_automatically().ok();
    let mut resume_context = context::rebuild_resume_context(&identity.session_id, None)?;
    let mut workflow = state::create_workflow(request)?;
    let invocation = if decision.invocation == "explicit-skill" {
        "explicit"
    } else {
        "natural-language"
    };
    let mut skill_runtime = skill::SkillRuntimeState::new_resolved(&manifest, invocation)?;
    if let Some(imported) = manifest.imported() {
        workflow.workflow_kind = "plugin-capability".to_string();
        workflow.source_path = imported.source_path.clone();
        workflow.source_hash = imported.source_sha256.clone();
        let admission_event = state::record_event(
            "plugin.capability.admitted",
            "instruction-only imported plugin skill 실행 경계 승인",
            &format!(
                "workflow_id={} plugin_id={} skill_id={} source_path={} source_sha256={} permission=none mode=read-only",
                workflow.workflow_id,
                imported.plugin_id,
                imported.id,
                imported.source_path,
                imported.source_sha256
            ),
        )?;
        skill_runtime.record_evidence("plugin_capability_admission");
        transcript::record_workflow_turn(
            &workflow,
            "tool",
            &admission_event,
            "instruction-only plugin capability admitted under read-only runtime policy",
            &[],
        )?;
    }
    skill_runtime.store_in_workflow(&mut workflow);
    workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision)?;
    dispatch_skill_hook(
        &manifest,
        &workflow,
        &mut skill_runtime,
        "session_start",
        "session",
        None,
    )?;
    dispatch_skill_hook(
        &manifest,
        &workflow,
        &mut skill_runtime,
        "user_request_received",
        "request",
        None,
    )?;
    dispatch_skill_hook(
        &manifest,
        &workflow,
        &mut skill_runtime,
        "pre_context_pack",
        "context",
        None,
    )?;
    let mut context_pack = context::build_context_pack(request)?;
    context::enforce_shared_source_budget(&mut resume_context, &mut context_pack);
    dispatch_skill_hook(
        &manifest,
        &workflow,
        &mut skill_runtime,
        "post_context_pack",
        &context_pack.pointer_summary(),
        None,
    )?;
    let available_context = available_context_labels(&manifest, request, &context_pack);
    if let Err(error) = skill::enforce_resolved_context(&manifest, &available_context) {
        let _ = skill_runtime.transition(skill::SkillState::Failed);
        skill_runtime.store_in_workflow(&mut workflow);
        workflow.phase = "failed".to_string();
        workflow.failure_reason = "skill-context-requirements-missing".to_string();
        workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision)?;
        state::clear_terminal_workflow_pointer(&workflow)?;
        return Err(error);
    }
    skill_runtime.transition(skill::SkillState::ContextReady)?;
    skill_runtime.store_in_workflow(&mut workflow);
    workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision)?;
    transcript::record_workflow_turn(&workflow, "user", "request", request, &[])?;
    let intent_event_id = state::record_event(
        "intent.classified",
        "사용자 요청 intent 정규화",
        &format!(
            "skill_id={} mode={} invocation={} signals={:?}",
            decision.skill_id, decision.mode, decision.invocation, decision.signals
        ),
    )?;
    let action_candidate = plan_action_candidate(&decision, &context_pack);
    let context_event_id = state::record_event(
        "context.pack.prepared",
        "bounded repository context 준비",
        &format!(
            "origin={} ontology_selected={} stale_rejected={} files_read={} chars_read={} source_pointers={}",
            context_pack.origin,
            context_pack.ontology_records_selected,
            context_pack.ontology_stale_rejected,
            context_pack.files_read,
            context_pack.chars_read,
            context_pack.pointer_summary()
        ),
    )?;
    transcript::record_workflow_turn(
        &workflow,
        "tool",
        &context_event_id,
        &format!(
            "context pack prepared: origin={} files={} chars={} pointers={}",
            context_pack.origin,
            context_pack.files_read,
            context_pack.chars_read,
            context_pack.pointer_summary()
        ),
        &context_pack.source_pointers,
    )?;
    let action_event_id = state::record_event(
        "action.candidate.prepared",
        "run action candidate 준비",
        &format!(
            "kind={} approval_required={} next_gate={} source_pointers={}",
            action_candidate.kind,
            action_candidate.approval_required,
            action_candidate.next_gate,
            context_pack.pointer_summary()
        ),
    )?;
    let agent_prompt = agent_loop_prompt(
        request,
        &decision,
        &resume_context,
        &context_pack,
        &action_candidate,
        &manifest,
    )?;
    dispatch_skill_hook(
        &manifest,
        &workflow,
        &mut skill_runtime,
        "pre_model_request",
        "chat_once",
        None,
    )?;
    skill_runtime.transition(skill::SkillState::ModelRequested)?;
    skill_runtime.store_in_workflow(&mut workflow);
    workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision)?;
    let run = match backend::chat_once(&agent_prompt, Some(RUN_MAX_TOKENS)) {
        Ok(run) => run,
        Err(err) => {
            return Err(fail_skill_workflow(
                &mut workflow,
                &mut skill_runtime,
                "backend-call-failed",
                err,
            ));
        }
    };
    dispatch_skill_hook(
        &manifest,
        &workflow,
        &mut skill_runtime,
        "post_model_response",
        "response-recorded",
        None,
    )?;
    dispatch_skill_hook(
        &manifest,
        &workflow,
        &mut skill_runtime,
        "pre_action_parse",
        "model-action",
        None,
    )?;
    let model_action = parse_model_action(&run.response, &action_candidate, &context_pack);
    dispatch_skill_hook(
        &manifest,
        &workflow,
        &mut skill_runtime,
        "post_action_parse",
        model_action.status,
        None,
    )?;
    let model_transcript =
        match model_transcript_content(&run.response, &model_action).or_else(|error| {
            if is_non_mutating_action(&model_action.kind) {
                crate::app::inference_adapter::answer::repair_existing(&run.response)
            } else {
                Err(error)
            }
        }) {
            Ok(content) => content,
            Err(error) => {
                skill_runtime.transition(skill::SkillState::Failed)?;
                skill_runtime.store_in_workflow(&mut workflow);
                workflow.phase = "failed".to_string();
                workflow.failure_reason = "model-answer-guard-failed".to_string();
                workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision)?;
                state::clear_terminal_workflow_pointer(&workflow)?;
                return Err(error);
            }
        };
    transcript::record_workflow_turn(
        &workflow,
        "model",
        &run.ledger_event,
        &model_transcript,
        &[],
    )?;
    let model_action_event_id = state::record_event(
        "model.action.parsed",
        "model response action parsing",
        &format!(
            "status={} kind={} source_pointers={} next_gate={} requested_side_effects={} executable_now={}",
            model_action.status,
            model_action.kind,
            model_action.source_pointers,
            model_action.next_gate,
            model_action.requested_side_effects,
            model_action.executable_now
        ),
    )?;

    workflow.action_kind = model_action.kind.clone();
    workflow.action_status = model_action.status.to_string();
    if manifest.imported().is_none() {
        workflow.source_path = model_action.target_path.clone();
    }
    workflow.find_text = model_action.find_text.clone();
    workflow.replace_text = model_action.replace_text.clone();
    workflow.verification_plan = model_action.verification_command.clone();
    workflow.phase = "action-recorded".to_string();
    skill_runtime.transition(skill::SkillState::ActionRecorded)?;
    skill_runtime.store_in_workflow(&mut workflow);
    workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision)?;

    if is_non_mutating_action(&model_action.kind) {
        let pointers_are_valid = model_action.kind != "inspect-sources"
            || (!matches!(model_action.source_pointers.as_str(), "none" | "unverified"));
        let action_status_is_safe = model_action.status == "parsed"
            || (model_action.kind == "answer-only"
                && model_action.status == "runtime-owned-answer");
        let action_is_safe = action_status_is_safe
            && model_action.requested_side_effects == "none"
            && pointers_are_valid;
        if !action_is_safe {
            let _ = skill_runtime.transition(skill::SkillState::Failed);
            skill_runtime.store_in_workflow(&mut workflow);
            workflow.phase = "failed".to_string();
            workflow.failure_reason = "invalid-or-hostile-model-action".to_string();
            workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision)?;
            state::clear_terminal_workflow_pointer(&workflow)?;
            return Err(AppError::blocked(format!(
                "run agent loop 차단\n- workflow id: {}\n- 이유: 읽기 전용 model action이 runtime 계약을 충족하지 못했습니다.\n- model side effect 실행: 없음",
                workflow.workflow_id
            )));
        }

        let answer = model_transcript;
        record_non_mutating_outcomes(
            &manifest,
            &context_pack,
            &model_action,
            &answer,
            &mut skill_runtime,
        );
        dispatch_skill_hook(
            &manifest,
            &workflow,
            &mut skill_runtime,
            "pre_final_report",
            "non-mutating-report",
            None,
        )?;
        dispatch_skill_hook(
            &manifest,
            &workflow,
            &mut skill_runtime,
            "stop_gate",
            "non-mutating-stop",
            None,
        )?;
        dispatch_skill_hook(
            &manifest,
            &workflow,
            &mut skill_runtime,
            "session_end",
            "complete",
            None,
        )?;
        let completed_imported = manifest
            .imported()
            .map(|imported| {
                plugin::revalidate_completed_imported_skill(
                    &imported.id,
                    &imported.source_path,
                    &imported.source_sha256,
                )
            })
            .transpose()?;
        if completed_imported.is_some() {
            skill_runtime.record_stop_criterion("plugin_capability_completed");
        }
        if let Err(error) = skill_runtime.validate_stop_against(&manifest) {
            return Err(fail_skill_workflow(
                &mut workflow,
                &mut skill_runtime,
                "skill-stop-gate-failed",
                error,
            ));
        }
        skill_runtime.transition(skill::SkillState::StopPassed)?;
        skill_runtime.transition(skill::SkillState::Complete)?;
        skill_runtime.store_in_workflow(&mut workflow);
        workflow.phase = "complete".to_string();
        workflow.action_status = "complete".to_string();
        workflow.approval_state = "not-required".to_string();
        workflow.result_summary = "non-mutating action completed".to_string();
        workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision)?;
        if let Some(imported) = completed_imported.as_ref() {
            plugin_completion_fault("before-event")?;
            state::record_event(
                "plugin.capability.completed",
                "instruction-only imported plugin skill 실행 완료",
                &format!(
                    "workflow_id={} plugin_id={} skill_id={} source_path={} source_sha256={} side_effects=none",
                    workflow.workflow_id,
                    imported.plugin_id,
                    imported.id,
                    imported.source_path,
                    imported.source_sha256
                ),
            )?;
            plugin_completion_fault("before-pointer-clear")?;
        }
        state::clear_terminal_workflow_pointer(&workflow)?;
        let mut report = render_non_mutating_report(
            request,
            &decision,
            &context_pack,
            &resume_context,
            &model_action,
            &answer,
            &workflow,
        );
        if let Some(imported) = manifest.imported() {
            report.push_str(&format!(
                "\n- plugin boundary: instruction-only/read-only\n- plugin source: {}@{}",
                imported.source_path, imported.source_sha256
            ));
        }
        return Ok(report);
    }

    let expected_pointer = format!("{}:1", model_action.target_path);
    let action_is_safe = model_action.status == "parsed"
        && model_action.kind == "patch-proposal"
        && model_action.requested_side_effects == "none"
        && !model_action.target_path.is_empty()
        && !model_action.find_text.is_empty()
        && !model_action.verification_command.is_empty()
        && model_action
            .source_pointers
            .split(',')
            .map(str::trim)
            .any(|pointer| pointer == expected_pointer);
    if !action_is_safe {
        let error = AppError::blocked(format!(
            "run agent loop 차단\n- workflow id: {}\n- 이유: model action은 non-executable record로 저장했지만 안전한 patch proposal 계약을 충족하지 못했습니다.\n- model side effect 실행: 없음",
            workflow.workflow_id
        ));
        return Err(fail_skill_workflow(
            &mut workflow,
            &mut skill_runtime,
            "invalid-or-hostile-model-action",
            error,
        ));
    }

    if manifest.id() == "fix-test" {
        if let Err(error) = crate::app::patch_adapter::validate_skill_verification(
            manifest.id(),
            &model_action.verification_command,
        ) {
            return Err(fail_skill_workflow(
                &mut workflow,
                &mut skill_runtime,
                "fix-test-verification-invalid",
                error,
            ));
        }
        dispatch_skill_hook(
            &manifest,
            &workflow,
            &mut skill_runtime,
            "pre_tool_call",
            "run_command",
            Some("run_command"),
        )?;
        dispatch_skill_hook(
            &manifest,
            &workflow,
            &mut skill_runtime,
            "pre_command_run",
            "failing-test-before",
            None,
        )?;
        let observed = crate::app::patch_adapter::record_failing_test_before(
            &workflow,
            &model_action.verification_command,
        );
        dispatch_skill_hook(
            &manifest,
            &workflow,
            &mut skill_runtime,
            "post_command_run",
            "failing-test-before",
            None,
        )?;
        dispatch_skill_hook(
            &manifest,
            &workflow,
            &mut skill_runtime,
            "post_tool_result",
            "run_command",
            Some("run_command"),
        )?;
        if let Err(error) = observed {
            skill_runtime.transition(skill::SkillState::Failed)?;
            skill_runtime.store_in_workflow(&mut workflow);
            workflow.phase = "failed".to_string();
            workflow.failure_reason = "failing-test-before-not-observed".to_string();
            workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision)?;
            state::clear_terminal_workflow_pointer(&workflow)?;
            return Err(error);
        }
        skill_runtime.record_evidence("failing_test_before");
    }

    dispatch_skill_hook(
        &manifest,
        &workflow,
        &mut skill_runtime,
        "pre_tool_call",
        "render_diff",
        Some("render_diff"),
    )?;
    let proposal = match crate::app::patch_adapter::prepare_workflow_proposal(
        &workflow.workflow_id,
        &workflow.action_id,
        &model_action.target_path,
        &model_action.find_text,
        &model_action.replace_text,
        &model_action.verification_command,
    ) {
        Ok(proposal) => proposal,
        Err(err) => {
            return Err(fail_skill_workflow(
                &mut workflow,
                &mut skill_runtime,
                "proposal-preparation-failed",
                err,
            ));
        }
    };
    dispatch_skill_hook(
        &manifest,
        &workflow,
        &mut skill_runtime,
        "post_tool_result",
        "render_diff",
        Some("render_diff"),
    )?;
    if manifest.evidence_requirements().contains(&"diff_review") {
        skill_runtime.record_evidence("diff_review");
    }
    workflow.source_path = proposal.relative_path.clone();
    workflow.source_hash = proposal.original_sha256.clone();
    workflow.proposal_id = proposal.proposal_id.clone();
    workflow.proposal_hash = proposal.proposal_hash.clone();
    workflow.approval_credential_hash = proposal.approval_credential_hash.clone();
    workflow.before_hash = proposal.original_sha256.clone();
    workflow.after_hash = proposal.proposed_sha256.clone();
    workflow.verification_plan = proposal.verification_command.clone();
    workflow.approval_state = "pending".to_string();
    workflow.result_summary = "patch proposal awaiting apply approval".to_string();
    workflow.phase = "pending-approval".to_string();
    skill_runtime.transition(skill::SkillState::AwaitingApproval)?;
    skill_runtime.store_in_workflow(&mut workflow);
    workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision)?;

    Ok(format!(
        "run agent loop\n- status: pending-approval\n- request: {}\n- invocation: {}\n- selected skill: {}\n- mode: {}\n- signals: {}\n- constraints: {}\n- classifier: {}\n- workflow ownership: {}\n- resumed context: {}\n- context origin: {}\n- ontology records selected: {}\n- ontology stale rejected: {}\n- context files read: {}\n- context chars: {}\n- source pointers: {}\n- action candidate: {}\n- approval required before side effect: {}\n- next gate: {}\n- allowed side effects now: {}\n- model action parse: {}\n- model action kind: {}\n- model action source pointers: {}\n- model action next gate: {}\n- model action requested side effects: {}\n- model action executable now: {}\n- backend: {}\n- model id: {}\n- model path: {}\n- ctx size: {}\n- prompt chars: {}\n- response chars: {}\n- requested max tokens: {}\n- effective max tokens: {}\n- resource governor admission: {}\n- resource governor token action: {}\n- resource governor reason: {}\n- finish reason: {}\n- guard: {}\n- prompt tokens: {}\n- completion tokens: {}\n- total tokens: {}\n- elapsed ms: {}\n- intent ledger event: {}\n- context ledger event: {}\n- action ledger event: {}\n- model action ledger event: {}\n- model ledger event: {}\n- workflow id: {}\n- workflow revision: {}\n- proposal id: {}\n- verification plan: {}\n- approval command: rpotato patch approve {} --token {}\n- model response visibility: action record만 저장하고 raw response는 표시하지 않음\n- boundary: model output은 실행되지 않았고 ontology source pointer에서 원본 source를 다시 읽어 diff만 만들었습니다.\n- diff:\n{}",
        request,
        decision.invocation,
        decision.skill_id,
        decision.mode,
        display_list(&decision.signals),
        display_list(&decision.constraints),
        decision.classifier,
        state::workflow_ownership_summary(),
        resume_context.summary(),
        context_pack.origin,
        context_pack.ontology_records_selected,
        context_pack.ontology_stale_rejected,
        context_pack.files_read,
        context_pack.chars_read,
        context_pack.pointer_summary(),
        action_candidate.kind,
        display_bool(action_candidate.approval_required),
        action_candidate.next_gate,
        action_candidate.allowed_side_effects,
        model_action.status,
        model_action.kind,
        model_action.source_pointers,
        model_action.next_gate,
        model_action.requested_side_effects,
        display_bool(model_action.executable_now),
        run.backend_id,
        run.model_id,
        run.model_path.display(),
        display_optional_u32(run.ctx_size),
        run.prompt_chars,
        run.response_chars,
        run.requested_max_tokens,
        run.effective_max_tokens,
        run.resource_governor_admission,
        run.resource_governor_token_action,
        run.resource_governor_reason,
        run.finish_reason,
        run.guard_status,
        display_optional_u32(run.prompt_tokens),
        display_optional_u32(run.completion_tokens),
        display_optional_u32(run.total_tokens),
        run.elapsed_ms,
        intent_event_id,
        context_event_id,
        action_event_id,
        model_action_event_id,
        run.ledger_event,
        workflow.workflow_id,
        workflow.revision,
        proposal.proposal_id,
        crate::app::workflow_adapter::ledger::redact_text(&proposal.verification_command),
        proposal.proposal_id,
        proposal.approval_token,
        proposal.diff
    ))
}
