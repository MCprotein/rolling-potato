use super::*;

#[cfg(test)]
pub fn proposal_summaries(limit: usize) -> Result<Vec<PatchProposalSummary>, AppError> {
    proposal_summaries_bounded(limit, usize::MAX)
}

#[cfg(test)]
pub fn proposal_summaries_bounded(
    limit: usize,
    scan_limit: usize,
) -> Result<Vec<PatchProposalSummary>, AppError> {
    let dir = paths::project_patch_proposals_dir();
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => {
            return Err(AppError::runtime(format!(
                "patch proposal directory를 읽지 못했습니다: {} ({err})",
                dir.display()
            )));
        }
    };

    let mut rows = Vec::new();
    for (index, entry) in entries.enumerate() {
        if index >= scan_limit {
            return Err(AppError::blocked(
                "patch proposal view directory scan budget 초과",
            ));
        }
        let entry = entry.map_err(|err| {
            AppError::runtime(format!("patch proposal entry를 읽지 못했습니다: {err}"))
        })?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("txt") {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        rows.push((modified, summary_from_path(&path)?));
    }

    rows.sort_by_key(|row| std::cmp::Reverse(row.0));
    Ok(rows
        .into_iter()
        .take(limit)
        .map(|(_, summary)| summary)
        .collect())
}

pub fn preflight_resume_workflow(workflow_id: &str) -> Result<(), AppError> {
    let workflow = state::load_workflow(workflow_id)?;
    match workflow.phase.as_str() {
        "model-pending" | "action-recorded" => Ok(()),
        "pending-approval" => {
            let proposal_path = paths::project_patch_proposals_dir()
                .join(format!("{}.txt", workflow.proposal_id));
            let record = load_proposal_record(&workflow.proposal_id, &proposal_path)?;
            validate_workflow_binding(&workflow, &record)?;
            let source_hash = current_source_hash(&workflow.source_path)?;
            if source_hash != workflow.before_hash {
                return Err(AppError::blocked(format!(
                    "workflow resume preflight 차단\n- 이유: pending approval target hash가 stale합니다.\n- expected: {}\n- current: {}",
                    workflow.before_hash, source_hash
                )));
            }
            Ok(())
        }
        "approved" | "verification-approved" => Err(AppError::blocked(format!(
            "workflow resume preflight 차단\n- 이유: prepared journal 없이 중간 승인 phase를 직접 재개할 수 없습니다.\n- phase: {}",
            workflow.phase
        ))),
        "pending-verification-approval" => {
            let proposal_path = paths::project_patch_proposals_dir()
                .join(format!("{}.txt", workflow.proposal_id));
            let record = load_proposal_record(&workflow.proposal_id, &proposal_path)?;
            validate_workflow_binding(&workflow, &record)?;
            let source_hash = current_source_hash(&workflow.source_path)?;
            if source_hash != workflow.after_hash {
                return Err(AppError::blocked(format!(
                    "workflow resume preflight 차단\n- 이유: verification 승인 대기 중 source hash가 변경되었습니다.\n- expected: {}\n- current: {}",
                    workflow.after_hash, source_hash
                )));
            }
            Ok(())
        }
        "verified" => {
            let proposal_path = paths::project_patch_proposals_dir()
                .join(format!("{}.txt", workflow.proposal_id));
            let record = load_proposal_record(&workflow.proposal_id, &proposal_path)?;
            validate_workflow_binding(&workflow, &record)?;
            crate::app::evidence_adapter::validate_patch_stop_gate(&workflow)
        }
        "complete" => {
            if workflow.workflow_kind == "plugin-capability" {
                validate_completed_plugin_workflow(&workflow).map(|_| ())
            } else {
                let proposal_path = paths::project_patch_proposals_dir()
                    .join(format!("{}.txt", workflow.proposal_id));
                let record = load_proposal_record(&workflow.proposal_id, &proposal_path)?;
                validate_workflow_binding(&workflow, &record)?;
                validate_completed_workflow(&workflow)
            }
        }
        phase
            if verification_domain::recovery_admission(phase)
                == RecoveryAdmission::InconclusiveNeverRerun =>
        {
            Err(AppError::blocked(
            "workflow resume preflight 차단\n- 이유: verification 결과가 확정되지 않아 session을 선택할 수 없습니다.",
            ))
        }
        "failed" | "cancelled" => Err(AppError::blocked(failure_report(&workflow))),
        other => Err(AppError::blocked(format!(
            "workflow resume preflight 차단\n- 이유: 안전하게 재개할 수 없는 phase입니다.\n- phase: {other}"
        ))),
    }
}

pub fn resume_workflow_report(workflow_id: &str) -> Result<String, AppError> {
    let (mut workflow, _approval_lock) = load_workflow_under_approval_lock(workflow_id)?;
    match workflow.phase.as_str() {
        "model-pending" | "action-recorded" => {
            workflow.failure_reason = format!("resume-incomplete-{}", workflow.phase);
            workflow.phase = "failed".to_string();
            if let Some(mut runtime) = workflow_skill_runtime(&workflow)? {
                let _ = runtime.transition(skill::SkillState::Failed);
                runtime.store_in_workflow(&mut workflow);
            }
            workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision)?;
            Err(AppError::blocked(format!("workflow 재개 실패\n- workflow id: {}\n- 이유: 중간 phase는 backend 또는 command를 자동 재실행하지 않습니다.\n- terminal phase: failed\n- validation gap: {}", workflow.workflow_id, workflow.failure_reason)))
        }
        "pending-approval" => {
            let detail = proposal_detail_for_workflow_bounded(
                &workflow,
                &workflow.proposal_id,
                MAX_PROPOSAL_RECORD_BYTES,
            )?;
            let source = fs::read_to_string(paths::project_root().join(&workflow.source_path))
                .map_err(|err| AppError::blocked(format!("workflow resume source reread 실패: {err}")))?;
            let source_hash = sha256_text(&source);
            if source_hash != workflow.before_hash {
                return Err(AppError::blocked(format!(
                    "workflow resume 차단\n- 이유: pending approval target hash가 stale합니다.\n- expected: {}\n- current: {}",
                    workflow.before_hash, source_hash
                )));
            }
            Ok(format!(
                "workflow 재개\n- 상태: 승인 대기\n- workflow id: {}\n- action id: {}\n- proposal id: {}\n- source hash: {}\n- verification plan: {}\n- 승인 명령: rpotato patch approve {} --token <최초-발급-token>\n- token 재표시: 불가 (hash만 저장됨)\n- backend 호출: 없음\n- diff:\n{}",
                workflow.workflow_id,
                workflow.action_id,
                workflow.proposal_id,
                workflow.before_hash,
                ledger::redact_text(&workflow.verification_plan),
                workflow.proposal_id,
                detail.diff
            ))
        }
        "approved" => Err(AppError::blocked(
            "workflow 재개 차단\n- 이유: exact E0..E9 prepared journal 없이 approved phase를 직접 재개할 수 없습니다.\n- 동작: journal recovery만 missing suffix를 적용할 수 있습니다.",
        )),
        "pending-verification-approval" => {
            let source_hash = current_source_hash(&workflow.source_path)?;
            if source_hash != workflow.after_hash {
                return Err(AppError::blocked(format!(
                    "workflow resume 차단\n- 이유: verification 승인 대기 중 source hash가 변경되었습니다.\n- expected: {}\n- current: {}",
                    workflow.after_hash, source_hash
                )));
            }
            Ok(format!(
                "workflow 재개\n- 상태: verification 승인 대기\n- workflow id: {}\n- proposal id: {}\n- 적용 SHA-256: {}\n- verification command: {}\n- 승인 명령: rpotato patch verify {} --token <최초-발급-token>\n- token 재표시: 불가 (hash만 저장됨)\n- verification 실행: 없음",
                workflow.workflow_id,
                workflow.proposal_id,
                workflow.after_hash,
                ledger::redact_text(&workflow.verification_plan),
                workflow.proposal_id
            ))
        }
        "verification-approved" => Err(AppError::blocked(
            "workflow 재개 차단\n- 이유: prepared verification journal 없이 verification-approved phase를 직접 재개할 수 없습니다.\n- 동작: 명령을 자동 실행하지 않습니다.",
        )),
        phase
            if verification_domain::recovery_admission(phase)
                == RecoveryAdmission::InconclusiveNeverRerun =>
        {
            Err(AppError::blocked({
                state::record_validation_gap(
                    "verification-inconclusive",
                    &format!("{}:verification-started", workflow.workflow_id),
                )?;
                "workflow 재개 차단\n- 이유: verification 시작 checkpoint 뒤 결과가 확정되지 않았습니다.\n- validation gap: verification-inconclusive\n- 동작: command를 자동 재실행하지 않습니다. `rpotato cancel`로 명시적으로 정리하세요."
            }))
        }
        "verified" => {
            crate::app::evidence_adapter::evaluate_patch_stop_gate(&workflow)?;
            let mut runtime = workflow_skill_runtime(&workflow)?;
            finalize_verified_skill(&mut workflow, runtime.as_mut())?;
            workflow.phase = "complete".to_string();
            workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision)?;
            state::clear_terminal_workflow_pointer(&workflow)?;
            Ok(success_report(&workflow))
        }
        "complete" => {
            if workflow.workflow_kind == "plugin-capability" {
                let imported = validate_completed_plugin_workflow(&workflow)?;
                ensure_plugin_completion_event(&workflow, &imported)?;
                state::clear_terminal_workflow_pointer(&workflow)?;
                return Ok(plugin_completion_recovery_report(&workflow));
            }
            let proposal_path = paths::project_patch_proposals_dir()
                .join(format!("{}.txt", workflow.proposal_id));
            let record = load_proposal_record(&workflow.proposal_id, &proposal_path)?;
            validate_workflow_binding(&workflow, &record)?;
            validate_completed_workflow(&workflow)?;
            state::clear_terminal_workflow_pointer(&workflow)?;
            Ok(success_report(&workflow))
        }
        "failed" | "cancelled" => Err(AppError::blocked(failure_report(&workflow))),
        other => Err(AppError::blocked(format!(
            "workflow resume 차단\n- 이유: 안전하게 재개할 수 없는 phase입니다.\n- phase: {other}\n- backend 호출: 없음"
        ))),
    }
}

pub(crate) fn resume_workflow_for_tui(
    workflow_id: &str,
    intent_id: &str,
    lease: &SelectionLease,
) -> Result<(), AppError> {
    validate_outcome_id(workflow_id, "workflow")?;
    validate_outcome_id(intent_id, "intent")?;
    let (observed, _approval_lock) = load_workflow_under_approval_lock(workflow_id)?;
    let transition_guard = transition::TransitionGuard::acquire_for(
        &lease.project_id,
        transition::CurrentStateIntent::Resume,
    )?;
    if ledger::event_details_match(
        "workflow.resume.accepted",
        &[("intent_id", intent_id), ("workflow_id", workflow_id)],
    )? {
        return Ok(());
    }
    if ledger::event_detail_exists("workflow.resume.accepted", "intent_id", intent_id)? {
        return Err(AppError::blocked(
            "workflow resume intent receipt binding 충돌",
        ));
    }
    if !state::tui_lease_matches_workflow_under_transition(lease, workflow_id)? {
        return Err(stale_selection_error());
    }
    let workflow_guard = state::WorkflowCheckpointGuard::acquire(workflow_id)?;
    let mut workflow = workflow_guard.load_current()?;
    if workflow != observed {
        return Err(stale_selection_error());
    }
    match workflow.phase.as_str() {
        "pending-approval" => {
            let detail = proposal_detail_for_workflow_bounded(
                &workflow,
                &workflow.proposal_id,
                MAX_PROPOSAL_RECORD_BYTES,
            )?;
            let source = fs::read_to_string(paths::project_root().join(&workflow.source_path))
                .map_err(|err| {
                    AppError::blocked(format!("workflow resume source reread 실패: {err}"))
                })?;
            if sha256_text(&source) != workflow.before_hash || detail.diff.is_empty() {
                return Err(AppError::blocked("internal.resume-corrupt-state"));
            }
            state::record_tui_workflow_resume_receipt_under_transition(
                &transition_guard,
                &workflow,
                intent_id,
                Some(&workflow),
            )?;
        }
        "pending-verification-approval" => {
            let detail = proposal_detail_for_workflow_bounded(
                &workflow,
                &workflow.proposal_id,
                MAX_PROPOSAL_RECORD_BYTES,
            )?;
            if detail.diff.is_empty() {
                return Err(AppError::blocked("internal.resume-corrupt-state"));
            }
            if current_source_hash(&workflow.source_path)? != workflow.after_hash {
                return Err(AppError::blocked("internal.resume-corrupt-state"));
            }
            state::record_tui_workflow_resume_receipt_under_transition(
                &transition_guard,
                &workflow,
                intent_id,
                Some(&workflow),
            )?;
        }
        "verified" => {
            crate::app::evidence_adapter::evaluate_patch_stop_gate(&workflow)?;
            let mut runtime = workflow_skill_runtime(&workflow)?;
            finalize_verified_skill(&mut workflow, runtime.as_mut())?;
            workflow.phase = "complete".to_string();
            workflow = state::checkpoint_workflow_under_transition(
                &transition_guard,
                workflow.clone(),
                workflow.revision,
            )?;
            state::clear_terminal_workflow_pointer_under_transition(&transition_guard, &workflow)?;
            state::record_tui_workflow_resume_receipt_under_transition(
                &transition_guard,
                &workflow,
                intent_id,
                None,
            )?;
        }
        "complete" => {
            if workflow.workflow_kind == "plugin-capability" {
                let imported = validate_completed_plugin_workflow(&workflow)?;
                ensure_plugin_completion_event_under_transition(
                    &transition_guard,
                    &workflow,
                    &imported,
                )?;
                state::clear_terminal_workflow_pointer_under_transition(
                    &transition_guard,
                    &workflow,
                )?;
                state::record_tui_workflow_resume_receipt_under_transition(
                    &transition_guard,
                    &workflow,
                    intent_id,
                    None,
                )?;
                return Ok(());
            }
            let proposal_path =
                paths::project_patch_proposals_dir().join(format!("{}.txt", workflow.proposal_id));
            let record = load_proposal_record(&workflow.proposal_id, &proposal_path)?;
            validate_workflow_binding(&workflow, &record)?;
            validate_completed_workflow(&workflow)?;
            state::clear_terminal_workflow_pointer_under_transition(&transition_guard, &workflow)?;
            state::record_tui_workflow_resume_receipt_under_transition(
                &transition_guard,
                &workflow,
                intent_id,
                None,
            )?;
        }
        "verification-started" => {
            return Err(AppError::blocked("internal.resume-inconclusive-effect"))
        }
        _ => return Err(AppError::blocked("internal.resume-corrupt-state")),
    }
    Ok(())
}
