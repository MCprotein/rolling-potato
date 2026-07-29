use super::super::{
    dispatch_skill_hook, fail_skill_workflow, is_non_mutating_action, model_transcript_content,
};
use crate::app::context_adapter::ContextPack;
use crate::app::extensions_adapter::skill;
use crate::app::inference_adapter::backend;
use crate::app::workflow_adapter::{state, transcript};
use crate::foundation::error::AppError;
use crate::runtime_core::inference::backend::BackendChatRun;
use crate::runtime_core::patch::intent::{parse_model_action, ActionCandidate, ParsedModelAction};

const RUN_MAX_TOKENS: u32 = 256;

pub(super) struct ModelTurn {
    pub(super) run: BackendChatRun,
    pub(super) action: ParsedModelAction,
    pub(super) transcript: String,
    pub(super) action_event_id: String,
}

pub(super) fn request_and_record(
    prompt: &str,
    manifest: &skill::ResolvedSkillManifest,
    action_candidate: &ActionCandidate,
    context_pack: &ContextPack,
    workflow: &mut state::WorkflowRecord,
    skill_runtime: &mut skill::SkillRuntimeState,
) -> Result<ModelTurn, AppError> {
    dispatch_skill_hook(
        manifest,
        workflow,
        skill_runtime,
        "pre_model_request",
        "chat_once",
        None,
    )?;
    skill_runtime.transition(skill::SkillState::ModelRequested)?;
    checkpoint_runtime(workflow, skill_runtime)?;

    let run = backend::chat_once(prompt, Some(RUN_MAX_TOKENS)).map_err(|error| {
        fail_skill_workflow(workflow, skill_runtime, "backend-call-failed", error)
    })?;
    dispatch_model_response_hooks(manifest, workflow, skill_runtime)?;
    let action = parse_model_action(&run.response, action_candidate, context_pack);
    dispatch_skill_hook(
        manifest,
        workflow,
        skill_runtime,
        "post_action_parse",
        action.status,
        None,
    )?;
    let visible_transcript = model_transcript_content(&run.response, &action).or_else(|error| {
        if is_non_mutating_action(&action.kind) {
            crate::app::inference_adapter::answer::repair_existing(&run.response)
        } else {
            Err(error)
        }
    });
    let visible_transcript = match visible_transcript {
        Ok(content) => content,
        Err(error) => {
            fail_answer_guard(workflow, skill_runtime)?;
            return Err(error);
        }
    };
    transcript::record_workflow_turn(
        workflow,
        "model",
        &run.ledger_event,
        &visible_transcript,
        &[],
    )?;
    let action_event_id = record_action_event(&action)?;
    record_action(workflow, skill_runtime, manifest, &action)?;

    Ok(ModelTurn {
        run,
        action,
        transcript: visible_transcript,
        action_event_id,
    })
}

fn checkpoint_runtime(
    workflow: &mut state::WorkflowRecord,
    skill_runtime: &skill::SkillRuntimeState,
) -> Result<(), AppError> {
    skill_runtime.store_in_workflow(workflow);
    *workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision)?;
    Ok(())
}

fn dispatch_model_response_hooks(
    manifest: &skill::ResolvedSkillManifest,
    workflow: &state::WorkflowRecord,
    skill_runtime: &mut skill::SkillRuntimeState,
) -> Result<(), AppError> {
    for (hook, payload) in [
        ("post_model_response", "response-recorded"),
        ("pre_action_parse", "model-action"),
    ] {
        dispatch_skill_hook(manifest, workflow, skill_runtime, hook, payload, None)?;
    }
    Ok(())
}

fn fail_answer_guard(
    workflow: &mut state::WorkflowRecord,
    skill_runtime: &mut skill::SkillRuntimeState,
) -> Result<(), AppError> {
    skill_runtime.transition(skill::SkillState::Failed)?;
    skill_runtime.store_in_workflow(workflow);
    workflow.phase = "failed".to_string();
    workflow.failure_reason = "model-answer-guard-failed".to_string();
    *workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision)?;
    state::clear_terminal_workflow_pointer(workflow)
}

fn record_action_event(action: &ParsedModelAction) -> Result<String, AppError> {
    state::record_event(
        "model.action.parsed",
        "model response action parsing",
        &format!(
            "status={} kind={} source_pointers={} next_gate={} requested_side_effects={} executable_now={}",
            action.status,
            action.kind,
            action.source_pointers,
            action.next_gate,
            action.requested_side_effects,
            action.executable_now
        ),
    )
}

fn record_action(
    workflow: &mut state::WorkflowRecord,
    skill_runtime: &mut skill::SkillRuntimeState,
    manifest: &skill::ResolvedSkillManifest,
    action: &ParsedModelAction,
) -> Result<(), AppError> {
    workflow.action_kind = action.kind.clone();
    workflow.action_status = action.status.to_string();
    if manifest.imported().is_none() {
        workflow.source_path = action.target_path.clone();
    }
    workflow.find_text = action.find_text.clone();
    workflow.replace_text = action.replace_text.clone();
    workflow.verification_plan = action.verification_command.clone();
    workflow.phase = "action-recorded".to_string();
    skill_runtime.transition(skill::SkillState::ActionRecorded)?;
    checkpoint_runtime(workflow, skill_runtime)
}
