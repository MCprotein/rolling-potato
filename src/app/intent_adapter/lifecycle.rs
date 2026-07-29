use crate::app::extensions_adapter::{hooks, skill};
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;

fn checkpoint_failure_or_original(workflow: state::WorkflowRecord, original: AppError) -> AppError {
    match state::checkpoint_workflow(workflow.clone(), workflow.revision) {
        Ok(_) => original,
        Err(persistence) => {
            let _ = state::record_validation_gap(
                "workflow-failure-checkpoint",
                &format!("{}:{}", workflow.workflow_id, workflow.failure_reason),
            );
            AppError {
                code: original.code,
                message: format!(
                    "{}\n- failure checkpoint: 저장 실패\n- persistence error: {}",
                    original.message, persistence.message
                ),
            }
        }
    }
}

pub(super) fn fail_skill_workflow(
    workflow: &mut state::WorkflowRecord,
    runtime: &mut skill::SkillRuntimeState,
    reason: &str,
    original: AppError,
) -> AppError {
    let _ = runtime.transition(skill::SkillState::Failed);
    runtime.store_in_workflow(workflow);
    workflow.phase = "failed".to_string();
    workflow.failure_reason = reason.to_string();
    match state::checkpoint_workflow(workflow.clone(), workflow.revision) {
        Ok(checkpointed) => {
            *workflow = checkpointed;
            if let Err(clear_error) = state::clear_terminal_workflow_pointer(workflow) {
                return AppError {
                    code: original.code,
                    message: format!(
                        "{}\n- terminal pointer 정리 실패: {}",
                        original.message, clear_error.message
                    ),
                };
            }
            original
        }
        Err(_) => checkpoint_failure_or_original(workflow.clone(), original),
    }
}

pub(super) fn dispatch_skill_hook(
    manifest: &skill::ResolvedSkillManifest,
    workflow: &state::WorkflowRecord,
    runtime: &mut skill::SkillRuntimeState,
    hook: &str,
    payload: &str,
    tool: Option<&str>,
) -> Result<(), AppError> {
    hooks::dispatch_native_lifecycle_for_skill(
        hooks::HookInput {
            hook,
            workflow_id: Some(&workflow.workflow_id),
            active_skill_id: Some(&runtime.active_skill_id),
            mode: manifest.mode(),
            payload,
        },
        tool,
        manifest,
    )?;
    runtime.record_hook(hook)
}

#[cfg(debug_assertions)]
pub(super) fn plugin_completion_fault(point: &str) -> Result<(), AppError> {
    if std::env::var("RPOTATO_TEST_PLUGIN_COMPLETION_FAULT").as_deref() == Ok(point) {
        return Err(AppError::runtime(format!(
            "injected plugin completion fault: {point}"
        )));
    }
    Ok(())
}

#[cfg(not(debug_assertions))]
pub(super) fn plugin_completion_fault(_point: &str) -> Result<(), AppError> {
    Ok(())
}
