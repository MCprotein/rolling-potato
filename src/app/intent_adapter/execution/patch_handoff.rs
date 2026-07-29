use super::super::{dispatch_skill_hook, fail_skill_workflow};
use super::model_turn::ModelTurn;
use super::preparation::PreparedExecution;
use crate::app::extensions_adapter::skill;
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::runtime_core::patch::intent::{
    display_bool, display_list, display_optional_u32, IntentDecision, ParsedModelAction,
};
use crate::runtime_core::patch::proposal::WorkflowProposal;

pub(super) fn complete(
    request: &str,
    decision: &IntentDecision,
    manifest: &skill::ResolvedSkillManifest,
    execution: &mut PreparedExecution,
    model_turn: &ModelTurn,
) -> Result<String, AppError> {
    ensure_safe_patch_action(
        &model_turn.action,
        &mut execution.workflow,
        &mut execution.skill_runtime,
    )?;
    observe_fix_test_baseline(
        manifest,
        &model_turn.action,
        &mut execution.workflow,
        &mut execution.skill_runtime,
    )?;
    let proposal = prepare_proposal(
        manifest,
        &model_turn.action,
        &mut execution.workflow,
        &mut execution.skill_runtime,
    )?;
    record_pending_approval(
        &proposal,
        &mut execution.workflow,
        &mut execution.skill_runtime,
    )?;
    Ok(render_pending_approval(
        request, decision, execution, model_turn, &proposal,
    ))
}

fn ensure_safe_patch_action(
    action: &ParsedModelAction,
    workflow: &mut state::WorkflowRecord,
    skill_runtime: &mut skill::SkillRuntimeState,
) -> Result<(), AppError> {
    let expected_pointer = format!("{}:1", action.target_path);
    let is_safe = action.status == "parsed"
        && action.kind == "patch-proposal"
        && action.requested_side_effects == "none"
        && !action.target_path.is_empty()
        && !action.find_text.is_empty()
        && !action.verification_command.is_empty()
        && action
            .source_pointers
            .split(',')
            .map(str::trim)
            .any(|pointer| pointer == expected_pointer);
    if is_safe {
        return Ok(());
    }
    let error = AppError::blocked(format!(
        "run agent loop 차단\n- workflow id: {}\n- 이유: model action은 non-executable record로 저장했지만 안전한 patch proposal 계약을 충족하지 못했습니다.\n- model side effect 실행: 없음",
        workflow.workflow_id
    ));
    Err(fail_skill_workflow(
        workflow,
        skill_runtime,
        "invalid-or-hostile-model-action",
        error,
    ))
}

fn observe_fix_test_baseline(
    manifest: &skill::ResolvedSkillManifest,
    action: &ParsedModelAction,
    workflow: &mut state::WorkflowRecord,
    skill_runtime: &mut skill::SkillRuntimeState,
) -> Result<(), AppError> {
    if manifest.id() != "fix-test" {
        return Ok(());
    }
    crate::app::patch_adapter::validate_skill_verification(
        manifest.id(),
        &action.verification_command,
    )
    .map_err(|error| {
        fail_skill_workflow(
            workflow,
            skill_runtime,
            "fix-test-verification-invalid",
            error,
        )
    })?;
    dispatch_skill_hook(
        manifest,
        workflow,
        skill_runtime,
        "pre_tool_call",
        "run_command",
        Some("run_command"),
    )?;
    dispatch_skill_hook(
        manifest,
        workflow,
        skill_runtime,
        "pre_command_run",
        "failing-test-before",
        None,
    )?;
    let observed = crate::app::patch_adapter::record_failing_test_before(
        workflow,
        &action.verification_command,
    );
    dispatch_skill_hook(
        manifest,
        workflow,
        skill_runtime,
        "post_command_run",
        "failing-test-before",
        None,
    )?;
    dispatch_skill_hook(
        manifest,
        workflow,
        skill_runtime,
        "post_tool_result",
        "run_command",
        Some("run_command"),
    )?;
    if let Err(error) = observed {
        skill_runtime.transition(skill::SkillState::Failed)?;
        skill_runtime.store_in_workflow(workflow);
        workflow.phase = "failed".to_string();
        workflow.failure_reason = "failing-test-before-not-observed".to_string();
        *workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision)?;
        state::clear_terminal_workflow_pointer(workflow)?;
        return Err(error);
    }
    skill_runtime.record_evidence("failing_test_before");
    Ok(())
}

fn prepare_proposal(
    manifest: &skill::ResolvedSkillManifest,
    action: &ParsedModelAction,
    workflow: &mut state::WorkflowRecord,
    skill_runtime: &mut skill::SkillRuntimeState,
) -> Result<WorkflowProposal, AppError> {
    dispatch_skill_hook(
        manifest,
        workflow,
        skill_runtime,
        "pre_tool_call",
        "render_diff",
        Some("render_diff"),
    )?;
    let proposal = crate::app::patch_adapter::prepare_workflow_proposal(
        &workflow.workflow_id,
        &workflow.action_id,
        &action.target_path,
        &action.find_text,
        &action.replace_text,
        &action.verification_command,
    )
    .map_err(|error| {
        fail_skill_workflow(
            workflow,
            skill_runtime,
            "proposal-preparation-failed",
            error,
        )
    })?;
    dispatch_skill_hook(
        manifest,
        workflow,
        skill_runtime,
        "post_tool_result",
        "render_diff",
        Some("render_diff"),
    )?;
    if manifest.evidence_requirements().contains(&"diff_review") {
        skill_runtime.record_evidence("diff_review");
    }
    Ok(proposal)
}

fn record_pending_approval(
    proposal: &WorkflowProposal,
    workflow: &mut state::WorkflowRecord,
    skill_runtime: &mut skill::SkillRuntimeState,
) -> Result<(), AppError> {
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
    skill_runtime.store_in_workflow(workflow);
    *workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision)?;
    Ok(())
}

fn render_pending_approval(
    request: &str,
    decision: &IntentDecision,
    execution: &PreparedExecution,
    model_turn: &ModelTurn,
    proposal: &WorkflowProposal,
) -> String {
    let action = &model_turn.action;
    let run = &model_turn.run;
    format!(
        "run agent loop\n- status: pending-approval\n- request: {}\n- invocation: {}\n- selected skill: {}\n- mode: {}\n- signals: {}\n- constraints: {}\n- classifier: {}\n- workflow ownership: {}\n- resumed context: {}\n- context origin: {}\n- ontology records selected: {}\n- ontology stale rejected: {}\n- context files read: {}\n- context chars: {}\n- source pointers: {}\n- action candidate: {}\n- approval required before side effect: {}\n- next gate: {}\n- allowed side effects now: {}\n- model action parse: {}\n- model action kind: {}\n- model action source pointers: {}\n- model action next gate: {}\n- model action requested side effects: {}\n- model action executable now: {}\n- backend: {}\n- model id: {}\n- model path: {}\n- ctx size: {}\n- prompt chars: {}\n- response chars: {}\n- requested max tokens: {}\n- effective max tokens: {}\n- resource governor admission: {}\n- resource governor token action: {}\n- resource governor reason: {}\n- finish reason: {}\n- guard: {}\n- prompt tokens: {}\n- completion tokens: {}\n- total tokens: {}\n- elapsed ms: {}\n- intent ledger event: {}\n- context ledger event: {}\n- action ledger event: {}\n- model action ledger event: {}\n- model ledger event: {}\n- workflow id: {}\n- workflow revision: {}\n- proposal id: {}\n- verification plan: {}\n- approval command: rpotato patch approve {} --token {}\n- model response visibility: action record만 저장하고 raw response는 표시하지 않음\n- boundary: model output은 실행되지 않았고 ontology source pointer에서 원본 source를 다시 읽어 diff만 만들었습니다.\n- diff:\n{}",
        request,
        decision.invocation,
        decision.skill_id,
        decision.mode,
        display_list(&decision.signals),
        display_list(&decision.constraints),
        decision.classifier,
        state::workflow_ownership_summary(),
        execution.resume_context.summary(),
        execution.context_pack.origin,
        execution.context_pack.ontology_records_selected,
        execution.context_pack.ontology_stale_rejected,
        execution.context_pack.files_read,
        execution.context_pack.chars_read,
        execution.context_pack.pointer_summary(),
        execution.action_candidate.kind,
        display_bool(execution.action_candidate.approval_required),
        execution.action_candidate.next_gate,
        execution.action_candidate.allowed_side_effects,
        action.status,
        action.kind,
        action.source_pointers,
        action.next_gate,
        action.requested_side_effects,
        display_bool(action.executable_now),
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
        execution.intent_event_id,
        execution.context_event_id,
        execution.action_event_id,
        model_turn.action_event_id,
        run.ledger_event,
        execution.workflow.workflow_id,
        execution.workflow.revision,
        proposal.proposal_id,
        crate::app::workflow_adapter::ledger::redact_text(&proposal.verification_command),
        proposal.proposal_id,
        proposal.approval_token,
        proposal.diff
    )
}
