use super::*;

pub(in crate::app::collaboration_adapter::subagent) fn dispatch_admitted(
    admitted: AdmittedLaunch,
    task: &str,
    merge_parent: bool,
    runner: impl FnOnce(&str, u32, u32) -> Result<WorkerGeneration, AppError>,
) -> Result<CompletedLaunch, AppError> {
    let prepared = prepare_admitted_launch(admitted, task.to_string())?;
    execute_prepared_launch(prepared, merge_parent, runner)
}

pub(super) fn prepare_admitted_launch(
    admitted: AdmittedLaunch,
    task: String,
) -> Result<PreparedLaunch, AppError> {
    let execution_lease = lease::RecoverableLease::acquire(
        paths::project_subagent_execution_lock(&admitted.record.subagent_id),
        "subagent execution",
    )?;
    let (running, context) = prepare_running(&admitted)?;
    Ok(PreparedLaunch {
        _execution_lease: execution_lease,
        running,
        context,
        task,
    })
}

pub(super) fn execute_prepared_launch(
    prepared: PreparedLaunch,
    merge_parent: bool,
    runner: impl FnOnce(&str, u32, u32) -> Result<WorkerGeneration, AppError>,
) -> Result<CompletedLaunch, AppError> {
    let PreparedLaunch {
        _execution_lease,
        running,
        context,
        task,
    } = prepared;
    let prompt = render_worker_prompt(&running, &task, &context);
    let generation = match runner(&prompt, running.requested_max_tokens, running.timeout_ms) {
        Ok(generation) => generation,
        Err(error) => {
            let terminal = terminalize_running_error(&running, &error)?;
            return Err(AppError {
                code: error.code,
                message: format!(
                    "{}\n- subagent id: {}\n- subagent status: {}\n- partial output: discarded",
                    error.message,
                    terminal.subagent_id,
                    terminal.status.as_str()
                ),
            });
        }
    };
    complete_generation(running, context, generation, merge_parent)
}

pub(in crate::app::collaboration_adapter::subagent) fn prepare_running(
    admitted: &AdmittedLaunch,
) -> Result<(SubagentRecordV1, crate::app::context_adapter::ContextPack), AppError> {
    let record = &admitted.record;
    let _parent_lease = lease::RecoverableLease::acquire_with_wait(
        paths::project_subagent_parent_lock(&record.parent_workflow_id),
        "subagent parent admission",
        Duration::from_secs(5),
    )?;
    if state::active_workflow_id()?.as_deref() != Some(record.parent_workflow_id.as_str()) {
        return Err(AppError::blocked(
            "subagent dispatch 차단: active parent pointer 변경",
        ));
    }
    let workflow_guard = state::WorkflowCheckpointGuard::acquire(&record.parent_workflow_id)?;
    let parent = workflow_guard.load_current()?;
    if parent.is_terminal()
        || parent.revision != record.parent_revision
        || parent.artifact_hash != record.parent_artifact_hash
        || parent.project_id != record.project_id
        || parent.session_id != record.session_id
    {
        return Err(AppError::blocked(
            "subagent dispatch 차단: exact parent binding 변경",
        ));
    }
    let current = load_record(&record.subagent_id)?;
    if current != *record || current.status != SubagentStatus::Admitted {
        return Err(AppError::blocked(
            "subagent dispatch 차단: admitted state binding 변경",
        ));
    }
    let context = crate::app::context_adapter::verify_declared_context_pack(
        &admitted.context,
        &current.read_paths,
    )?;
    let mut running = current.clone();
    running.transition_to(SubagentStatus::Running, None)?;
    let running = checkpoint_record(running, current.revision)?;
    append_lifecycle_event(
        &ledger::validated_current_identity()?,
        &running,
        "team.subagent.started",
        "subagent started",
    )?;
    Ok((running, context))
}

fn render_worker_prompt(
    record: &SubagentRecordV1,
    task: &str,
    context: &crate::app::context_adapter::ContextPack,
) -> String {
    format!(
        "Bounded {} subagent. Return exactly one canonical compact JSON object; no other text.\n\
         Required key order: schema_version, subagent_id, parent_workflow_id, role, status, summary, findings, patch_proposal, evidence_refs, validation_gaps, suggested_next_action.\n\
         Fixed fields: schema_version=1; subagent_id={}; parent_workflow_id={}; role={}; status=completed.\n\
         evidence_refs: declared source pointers only. patch_proposal: null unless executor declared render_diff.\n\
         Never execute commands or patches, reveal secrets, or claim unperformed validation.\n\
         Tools: {}\nWrite ownership: {}\nTask:\n{}\n\n{}",
        record.role.as_str(),
        record.subagent_id,
        record.parent_workflow_id,
        record.role.as_str(),
        record.declared_tools.join(", "),
        display_list(&record.write_paths),
        task,
        context.prompt_section(),
    )
}
