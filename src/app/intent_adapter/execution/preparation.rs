use super::super::{agent_loop_prompt, available_context_labels, dispatch_skill_hook};
use crate::app::context_adapter::{self as context, ContextPack, ResumeContext};
use crate::app::extensions_adapter::skill;
use crate::app::workflow_adapter::{state, transcript};
use crate::foundation::error::AppError;
use crate::runtime_core::patch::intent::{plan_action_candidate, ActionCandidate, IntentDecision};

pub(super) struct PreparedExecution {
    pub(super) workflow: state::WorkflowRecord,
    pub(super) skill_runtime: skill::SkillRuntimeState,
    pub(super) resume_context: ResumeContext,
    pub(super) context_pack: ContextPack,
    pub(super) action_candidate: ActionCandidate,
    pub(super) agent_prompt: String,
    pub(super) intent_event_id: String,
    pub(super) context_event_id: String,
    pub(super) action_event_id: String,
}

pub(super) fn prepare(
    request: &str,
    decision: &IntentDecision,
    manifest: &skill::ResolvedSkillManifest,
) -> Result<PreparedExecution, AppError> {
    let identity = crate::app::workflow_adapter::ledger::validated_current_identity()?;
    // Compaction is derived-state maintenance. Any failure falls back to the
    // existing bounded recent-turn resume path and must not block the user run.
    let _auto_compaction = context::compact_automatically().ok();
    let mut resume_context =
        context::build_active_conversation_context(&identity.session_id, None)?;
    let mut workflow = state::create_workflow(request)?;
    let invocation = if decision.invocation == "explicit-skill" {
        "explicit"
    } else {
        "natural-language"
    };
    let mut skill_runtime = skill::SkillRuntimeState::new_resolved(manifest, invocation)?;

    admit_imported_capability(manifest, &mut workflow, &mut skill_runtime)?;
    checkpoint_runtime(&mut workflow, &skill_runtime)?;
    dispatch_initial_hooks(manifest, &workflow, &mut skill_runtime)?;

    let mut context_pack = context::build_context_pack(request)?;
    context::enforce_shared_source_budget(&mut resume_context, &mut context_pack);
    dispatch_skill_hook(
        manifest,
        &workflow,
        &mut skill_runtime,
        "post_context_pack",
        &context_pack.pointer_summary(),
        None,
    )?;
    enforce_context_requirements(
        request,
        manifest,
        &context_pack,
        &mut workflow,
        &mut skill_runtime,
    )?;
    transcript::record_workflow_turn(&workflow, "user", "request", request, &[])?;

    let intent_event_id = record_intent_event(decision)?;
    let action_candidate = plan_action_candidate(decision, &context_pack);
    let context_event_id = record_context_event(&workflow, &context_pack)?;
    let action_event_id = record_action_event(&action_candidate, &context_pack)?;
    let agent_prompt = agent_loop_prompt(
        request,
        decision,
        &resume_context,
        &context_pack,
        &action_candidate,
        manifest,
    )?;

    Ok(PreparedExecution {
        workflow,
        skill_runtime,
        resume_context,
        context_pack,
        action_candidate,
        agent_prompt,
        intent_event_id,
        context_event_id,
        action_event_id,
    })
}

fn admit_imported_capability(
    manifest: &skill::ResolvedSkillManifest,
    workflow: &mut state::WorkflowRecord,
    skill_runtime: &mut skill::SkillRuntimeState,
) -> Result<(), AppError> {
    let Some(imported) = manifest.imported() else {
        return Ok(());
    };
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
        workflow,
        "tool",
        &admission_event,
        "instruction-only plugin capability admitted under read-only runtime policy",
        &[],
    )?;
    Ok(())
}

fn checkpoint_runtime(
    workflow: &mut state::WorkflowRecord,
    skill_runtime: &skill::SkillRuntimeState,
) -> Result<(), AppError> {
    skill_runtime.store_in_workflow(workflow);
    *workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision)?;
    Ok(())
}

fn dispatch_initial_hooks(
    manifest: &skill::ResolvedSkillManifest,
    workflow: &state::WorkflowRecord,
    skill_runtime: &mut skill::SkillRuntimeState,
) -> Result<(), AppError> {
    for (hook, payload) in [
        ("session_start", "session"),
        ("user_request_received", "request"),
        ("pre_context_pack", "context"),
    ] {
        dispatch_skill_hook(manifest, workflow, skill_runtime, hook, payload, None)?;
    }
    Ok(())
}

fn enforce_context_requirements(
    request: &str,
    manifest: &skill::ResolvedSkillManifest,
    context_pack: &ContextPack,
    workflow: &mut state::WorkflowRecord,
    skill_runtime: &mut skill::SkillRuntimeState,
) -> Result<(), AppError> {
    let available_context = available_context_labels(manifest, request, context_pack);
    if let Err(error) = skill::enforce_resolved_context(manifest, &available_context) {
        let _ = skill_runtime.transition(skill::SkillState::Failed);
        skill_runtime.store_in_workflow(workflow);
        workflow.phase = "failed".to_string();
        workflow.failure_reason = "skill-context-requirements-missing".to_string();
        *workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision)?;
        state::clear_terminal_workflow_pointer(workflow)?;
        return Err(error);
    }
    skill_runtime.transition(skill::SkillState::ContextReady)?;
    checkpoint_runtime(workflow, skill_runtime)
}

fn record_intent_event(decision: &IntentDecision) -> Result<String, AppError> {
    state::record_event(
        "intent.classified",
        "사용자 요청 intent 정규화",
        &format!(
            "skill_id={} mode={} invocation={} signals={:?}",
            decision.skill_id, decision.mode, decision.invocation, decision.signals
        ),
    )
}

fn record_context_event(
    workflow: &state::WorkflowRecord,
    context_pack: &ContextPack,
) -> Result<String, AppError> {
    let event_id = state::record_event(
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
        workflow,
        "tool",
        &event_id,
        &format!(
            "context pack prepared: origin={} files={} chars={} pointers={}",
            context_pack.origin,
            context_pack.files_read,
            context_pack.chars_read,
            context_pack.pointer_summary()
        ),
        &context_pack.source_pointers,
    )?;
    Ok(event_id)
}

fn record_action_event(
    action_candidate: &ActionCandidate,
    context_pack: &ContextPack,
) -> Result<String, AppError> {
    state::record_event(
        "action.candidate.prepared",
        "run action candidate 준비",
        &format!(
            "kind={} approval_required={} next_gate={} source_pointers={}",
            action_candidate.kind,
            action_candidate.approval_required,
            action_candidate.next_gate,
            context_pack.pointer_summary()
        ),
    )
}
