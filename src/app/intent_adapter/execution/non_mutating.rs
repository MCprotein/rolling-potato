use super::super::{
    dispatch_skill_hook, fail_skill_workflow, plugin_completion_fault,
    record_non_mutating_outcomes, render_non_mutating_report,
};
use super::model_turn::ModelTurn;
use super::preparation::PreparedExecution;
use crate::app::extensions_adapter::{plugin, skill};
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::runtime_core::patch::intent::IntentDecision;

pub(super) fn complete(
    request: &str,
    decision: &IntentDecision,
    manifest: &skill::ResolvedSkillManifest,
    execution: &mut PreparedExecution,
    model_turn: &ModelTurn,
) -> Result<String, AppError> {
    let model_action = &model_turn.action;
    let workflow = &mut execution.workflow;
    let skill_runtime = &mut execution.skill_runtime;
    let pointers_are_valid = model_action.kind != "inspect-sources"
        || (!matches!(model_action.source_pointers.as_str(), "none" | "unverified"));
    let action_status_is_safe = model_action.status == "parsed"
        || (model_action.kind == "answer-only" && model_action.status == "runtime-owned-answer");
    let action_is_safe = action_status_is_safe
        && model_action.requested_side_effects == "none"
        && pointers_are_valid;
    if !action_is_safe {
        let _ = skill_runtime.transition(skill::SkillState::Failed);
        skill_runtime.store_in_workflow(workflow);
        workflow.phase = "failed".to_string();
        workflow.failure_reason = "invalid-or-hostile-model-action".to_string();
        *workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision)?;
        state::clear_terminal_workflow_pointer(workflow)?;
        return Err(AppError::blocked(format!(
            "run agent loop 차단\n- workflow id: {}\n- 이유: 읽기 전용 model action이 runtime 계약을 충족하지 못했습니다.\n- model side effect 실행: 없음",
            workflow.workflow_id
        )));
    }

    record_non_mutating_outcomes(
        manifest,
        &execution.context_pack,
        model_action,
        &model_turn.transcript,
        skill_runtime,
    );
    dispatch_skill_hook(
        manifest,
        workflow,
        skill_runtime,
        "pre_final_report",
        "non-mutating-report",
        None,
    )?;
    dispatch_skill_hook(
        manifest,
        workflow,
        skill_runtime,
        "stop_gate",
        "non-mutating-stop",
        None,
    )?;
    dispatch_skill_hook(
        manifest,
        workflow,
        skill_runtime,
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
    if let Err(error) = skill_runtime.validate_stop_against(manifest) {
        return Err(fail_skill_workflow(
            workflow,
            skill_runtime,
            "skill-stop-gate-failed",
            error,
        ));
    }
    skill_runtime.transition(skill::SkillState::StopPassed)?;
    skill_runtime.transition(skill::SkillState::Complete)?;
    skill_runtime.store_in_workflow(workflow);
    workflow.phase = "complete".to_string();
    workflow.action_status = "complete".to_string();
    workflow.approval_state = "not-required".to_string();
    workflow.result_summary = "non-mutating action completed".to_string();
    *workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision)?;
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
    state::clear_terminal_workflow_pointer(workflow)?;
    let mut report = render_non_mutating_report(
        request,
        decision,
        &execution.context_pack,
        &execution.resume_context,
        model_action,
        &model_turn.transcript,
        workflow,
    );
    if let Some(imported) = manifest.imported() {
        report.push_str(&format!(
            "\n- plugin boundary: instruction-only/read-only\n- plugin source: {}@{}",
            imported.source_path, imported.source_sha256
        ));
    }
    Ok(report)
}
