use super::super::*;

pub(in super::super) fn workflow_skill_runtime(
    workflow: &state::WorkflowRecord,
) -> Result<Option<skill::SkillRuntimeState>, AppError> {
    if workflow.active_skill_id.is_empty() {
        return Ok(None);
    }
    skill::SkillRuntimeState::from_workflow(workflow).map(Some)
}

pub(super) fn validate_skill_phase_for_side_effect(
    workflow: &state::WorkflowRecord,
    runtime: &skill::SkillRuntimeState,
) -> Result<(), AppError> {
    let expected = match workflow.phase.as_str() {
        "approved" => skill::SkillState::AwaitingApproval,
        "verification-started" => skill::SkillState::AwaitingVerification,
        _ => {
            return Err(AppError::blocked(format!(
                "skill side effect 차단\n- workflow phase: {}\n- 이유: side effect를 허용하는 phase가 아닙니다.",
                workflow.phase
            )))
        }
    };
    if runtime.state != expected {
        return Err(AppError::blocked(format!(
            "skill side effect 차단\n- workflow phase: {}\n- skill state: {}\n- expected skill state: {}",
            workflow.phase,
            runtime.state.label(),
            expected.label()
        )));
    }
    Ok(())
}

pub(in super::super) fn validate_failing_test_before(
    workflow: &state::WorkflowRecord,
    runtime: &skill::SkillRuntimeState,
) -> Result<(), AppError> {
    if runtime.active_skill_id != "fix-test" {
        return Ok(());
    }
    let command_hash =
        state::sha256_text(&build_verification_plan(&workflow.verification_plan)?.command);
    if !runtime
        .evidence
        .iter()
        .any(|evidence| evidence == "failing_test_before")
        || !ledger::event_details_match(
            "skill.test_failure.observed",
            &[
                ("workflow_id", &workflow.workflow_id),
                ("command_hash", &command_hash),
            ],
        )?
    {
        return Err(AppError::blocked(
            "fix-test evidence 차단\n- 이유: patch 전 실제 failing test event와 workflow evidence binding이 없습니다.",
        ));
    }
    Ok(())
}

pub(in super::super) fn validate_completed_workflow(
    workflow: &state::WorkflowRecord,
) -> Result<(), AppError> {
    if workflow.phase != "complete" {
        return Err(AppError::blocked(
            "workflow complete 검증 차단\n- 이유: complete phase가 아닙니다.",
        ));
    }
    if let Some(runtime) = workflow_skill_runtime(workflow)? {
        if runtime.state != skill::SkillState::Complete {
            return Err(AppError::blocked(format!(
                "workflow complete 검증 차단\n- skill: {}\n- skill state: {}",
                runtime.active_skill_id,
                runtime.state.label()
            )));
        }
        validate_skill_verification(&runtime.active_skill_id, &workflow.verification_plan)?;
        validate_failing_test_before(workflow, &runtime)?;
        runtime.validate_stop()?;
    }
    crate::app::evidence_adapter::validate_patch_stop_gate(workflow)
}

pub(super) fn dispatch_workflow_skill_hook(
    workflow: &state::WorkflowRecord,
    runtime: &mut skill::SkillRuntimeState,
    hook: &str,
    tool: &str,
) -> Result<(), AppError> {
    hooks::dispatch_native_lifecycle(
        hooks::HookInput {
            hook,
            workflow_id: Some(&workflow.workflow_id),
            active_skill_id: Some(&runtime.active_skill_id),
            mode: skill::find_skill(&runtime.active_skill_id)
                .map(|manifest| manifest.mode)
                .unwrap_or("unknown"),
            payload: tool,
        },
        matches!(hook, "pre_tool_call" | "post_tool_result").then_some(tool),
    )?;
    runtime.record_hook(hook)
}

pub(in super::super) fn finalize_verified_skill(
    workflow: &mut state::WorkflowRecord,
    runtime: Option<&mut skill::SkillRuntimeState>,
) -> Result<(), AppError> {
    let Some(runtime) = runtime else {
        return Ok(());
    };
    dispatch_workflow_skill_hook(
        workflow,
        runtime,
        "pre_final_report",
        "patch-success-report",
    )?;
    runtime.record_stop_criterion("korean_report_passed");
    dispatch_workflow_skill_hook(workflow, runtime, "stop_gate", "patch-stop")?;
    dispatch_workflow_skill_hook(workflow, runtime, "session_end", "complete")?;
    runtime.validate_stop()?;
    runtime.transition(skill::SkillState::StopPassed)?;
    runtime.transition(skill::SkillState::Complete)?;
    runtime.store_in_workflow(workflow);
    Ok(())
}
