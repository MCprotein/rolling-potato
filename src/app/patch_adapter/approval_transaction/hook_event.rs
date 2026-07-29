use super::super::*;

pub(super) fn prepare_transaction_hook_event(
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
