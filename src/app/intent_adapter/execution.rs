//! Intent execution coordinator.

use super::is_non_mutating_action;
use crate::app::extensions_adapter::skill;
use crate::app::inference_adapter::backend;
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::runtime_core::patch::intent::IntentDecision;

mod model_turn;
mod non_mutating;
mod patch_handoff;
mod preparation;

pub(super) fn run_with_decision(
    request: &str,
    decision: IntentDecision,
    manifest: skill::ResolvedSkillManifest,
) -> Result<String, AppError> {
    if let Some(workflow_id) = state::active_workflow_id()? {
        return crate::app::patch_adapter::resume_workflow_report(&workflow_id);
    }
    backend::preflight_chat_ready()?;

    let mut execution = preparation::prepare(request, &decision, &manifest)?;
    let model_turn = model_turn::request_and_record(
        &execution.agent_prompt,
        &manifest,
        &execution.action_candidate,
        &execution.context_pack,
        &mut execution.workflow,
        &mut execution.skill_runtime,
    )?;

    if is_non_mutating_action(&model_turn.action.kind) {
        return non_mutating::complete(request, &decision, &manifest, &mut execution, &model_turn);
    }

    patch_handoff::complete(request, &decision, &manifest, &mut execution, &model_turn)
}
