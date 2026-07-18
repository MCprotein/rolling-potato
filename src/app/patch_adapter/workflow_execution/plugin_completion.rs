use super::super::*;

pub(in super::super) fn validate_completed_plugin_workflow(
    workflow: &state::WorkflowRecord,
) -> Result<skill::ImportedSkillManifest, AppError> {
    if workflow.phase != "complete" || workflow.workflow_kind != "plugin-capability" {
        return Err(AppError::blocked(
            "plugin workflow complete 검증 차단\n- 이유: complete plugin-capability workflow가 아닙니다.",
        ));
    }
    if !matches!(
        workflow.action_kind.as_str(),
        "answer-only" | "inspect-sources" | "generated-artifact-plan"
    ) || workflow.action_status != "complete"
        || workflow.approval_state != "not-required"
        || !workflow.proposal_id.is_empty()
        || !workflow.verification_plan.is_empty()
    {
        return Err(AppError::blocked(format!(
            "plugin workflow complete 검증 차단\n- workflow: {}\n- 이유: read-only completion shape가 아닙니다.",
            workflow.workflow_id
        )));
    }
    let imported = plugin::revalidate_completed_imported_skill(
        &workflow.active_skill_id,
        &workflow.source_path,
        &workflow.source_hash,
    )?;
    let resolved = skill::ResolvedSkillManifest::Imported(imported.clone());
    let runtime = skill::SkillRuntimeState::from_workflow_against(workflow, &resolved)?;
    if runtime.state != skill::SkillState::Complete {
        return Err(AppError::blocked(format!(
            "plugin workflow complete 검증 차단\n- skill: {}\n- skill state: {}",
            runtime.active_skill_id,
            runtime.state.label()
        )));
    }
    runtime.validate_stop_against(&resolved)?;
    if !ledger::event_details_match(
        "plugin.capability.admitted",
        &[
            ("workflow_id", &workflow.workflow_id),
            ("plugin_id", &imported.plugin_id),
            ("skill_id", &imported.id),
            ("source_path", &imported.source_path),
            ("source_sha256", &imported.source_sha256),
            ("permission", "none"),
            ("mode", "read-only"),
        ],
    )? {
        return Err(AppError::blocked(
            "plugin workflow complete 검증 차단\n- 이유: admission ledger binding이 없습니다.",
        ));
    }
    Ok(imported)
}

fn plugin_completion_event_exists(
    workflow: &state::WorkflowRecord,
    imported: &skill::ImportedSkillManifest,
) -> Result<bool, AppError> {
    ledger::event_details_match(
        "plugin.capability.completed",
        &[
            ("workflow_id", &workflow.workflow_id),
            ("plugin_id", &imported.plugin_id),
            ("skill_id", &imported.id),
            ("source_path", &imported.source_path),
            ("source_sha256", &imported.source_sha256),
            ("side_effects", "none"),
        ],
    )
}

fn plugin_completion_event_details(
    workflow: &state::WorkflowRecord,
    imported: &skill::ImportedSkillManifest,
) -> String {
    format!(
        "workflow_id={} plugin_id={} skill_id={} source_path={} source_sha256={} side_effects=none",
        workflow.workflow_id,
        imported.plugin_id,
        imported.id,
        imported.source_path,
        imported.source_sha256
    )
}

pub(in super::super) fn ensure_plugin_completion_event(
    workflow: &state::WorkflowRecord,
    imported: &skill::ImportedSkillManifest,
) -> Result<(), AppError> {
    if plugin_completion_event_exists(workflow, imported)? {
        return Ok(());
    }
    if ledger::event_detail_exists(
        "plugin.capability.completed",
        "workflow_id",
        &workflow.workflow_id,
    )? {
        return Err(AppError::blocked("plugin completion ledger binding 충돌"));
    }
    state::record_event(
        "plugin.capability.completed",
        "instruction-only imported plugin skill 실행 완료",
        &plugin_completion_event_details(workflow, imported),
    )?;
    Ok(())
}

pub(in super::super) fn ensure_plugin_completion_event_under_transition(
    transition_guard: &transition::TransitionGuard,
    workflow: &state::WorkflowRecord,
    imported: &skill::ImportedSkillManifest,
) -> Result<(), AppError> {
    if plugin_completion_event_exists(workflow, imported)? {
        return Ok(());
    }
    if ledger::event_detail_exists(
        "plugin.capability.completed",
        "workflow_id",
        &workflow.workflow_id,
    )? {
        return Err(AppError::blocked("plugin completion ledger binding 충돌"));
    }
    state::record_workflow_event_under_transition(
        transition_guard,
        workflow,
        "plugin.capability.completed",
        "instruction-only imported plugin skill 실행 완료",
        &plugin_completion_event_details(workflow, imported),
    )?;
    Ok(())
}

pub(in super::super) fn plugin_completion_recovery_report(
    workflow: &state::WorkflowRecord,
) -> String {
    format!(
        "plugin capability 복구 완료\n- 결과: 성공\n- workflow id: {}\n- skill id: {}\n- source: {}@{}\n- side effect: 없음\n- completion event: 확인됨\n- active pointer: 정리됨",
        workflow.workflow_id,
        workflow.active_skill_id,
        workflow.source_path,
        workflow.source_hash
    )
}
