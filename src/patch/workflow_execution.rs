use super::*;

pub fn rotate_workflow_token_report(proposal_id: &str) -> Result<String, AppError> {
    validate_proposal_id(proposal_id)?;
    let _approval_lock = ApprovalLock::acquire(proposal_id)?;
    let active = state::active_workflow_id()?;
    let path = paths::project_patch_proposals_dir().join(format!("{proposal_id}.txt"));
    let record = load_proposal_record(proposal_id, &path)?;
    if record.workflow_id.is_empty() {
        return Err(AppError::blocked(
            "approval token 재발급 차단\n- 이유: workflow proposal만 rotate할 수 있습니다.",
        ));
    }
    if active.as_deref() != Some(record.workflow_id.as_str()) {
        return Err(AppError::blocked("approval token 재발급 차단\n- 이유: active workflow/current pointer가 일치하지 않습니다."));
    }
    let mut workflow = state::load_workflow(&record.workflow_id)?;
    validate_workflow_binding(&workflow, &record)?;
    let token = issue_approval_token()?;
    let gate = match workflow.phase.as_str() {
        "pending-approval" => {
            workflow.approval_credential_hash = approval_domain::hash_token(&token);
            workflow.approval_state = "pending-rotated".to_string();
            "patch-apply"
        }
        "pending-verification-approval" => {
            workflow.verification_credential_hash = approval_domain::hash_token(&token);
            workflow.verification_approval_state = "pending-rotated".to_string();
            "verification-command"
        }
        _ => {
            return Err(AppError::blocked(
                "approval token 재발급 차단\n- 이유: pending approval phase에서만 가능합니다.",
            ))
        }
    };
    workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision)?;
    Ok(format!("승인 token 재발급\n- gate: {}\n- workflow id: {}\n- proposal id: {}\n- workflow revision: {}\n- 새 approval token: {}\n- 이전 token: 폐기됨\n- token 재표시: 불가", gate, workflow.workflow_id, workflow.proposal_id, workflow.revision, token))
}

pub(super) fn continue_approved_workflow(
    record: ProposalRecord,
    mut workflow: Option<state::WorkflowRecord>,
    verification_plan: Option<VerificationPlan>,
) -> Result<String, AppError> {
    let mut skill_runtime = workflow
        .as_ref()
        .map(workflow_skill_runtime)
        .transpose()?
        .flatten();
    if let (Some(current), Some(runtime)) = (workflow.as_ref(), skill_runtime.as_ref()) {
        validate_skill_phase_for_side_effect(current, runtime)?;
        validate_skill_verification(&runtime.active_skill_id, &record.verification_command)?;
        validate_failing_test_before(current, runtime)?;
    }
    let first_apply = skill_runtime
        .as_ref()
        .is_some_and(|runtime| runtime.state == skill::SkillState::AwaitingApproval);
    if first_apply {
        let current = workflow.as_ref().expect("skill workflow requires workflow");
        let runtime = skill_runtime.as_mut().expect("checked above");
        dispatch_workflow_skill_hook(current, runtime, "pre_tool_call", "apply_patch")?;
        dispatch_workflow_skill_hook(current, runtime, "pre_patch_apply", "apply_patch")?;
    }
    let apply = if verification_plan.is_some() {
        validate_applied_proposal(&record)?
    } else {
        match apply_proposal(&record) {
            Ok(apply) => apply,
            Err(err) => {
                if let Some(current) = workflow.as_mut() {
                    if let Some(runtime) = skill_runtime.as_mut() {
                        let _ = runtime.transition(skill::SkillState::Failed);
                        runtime.store_in_workflow(current);
                    }
                    current.phase = "failed".to_string();
                    current.failure_reason = "guarded-apply-failed".to_string();
                    if let Err(persistence) =
                        state::checkpoint_workflow(current.clone(), current.revision)
                    {
                        let gap = state::record_validation_gap(
                            "workflow-failure-checkpoint",
                            &format!("{}:guarded-apply-failed", current.workflow_id),
                        )
                        .err()
                        .map(|gap| format!("\n- validation-gap error: {}", gap.message))
                        .unwrap_or_default();
                        return Err(AppError {
                            code: err.code,
                            message: format!(
                                "{}\n- failure checkpoint: 저장 실패\n- persistence error: {}{}",
                                err.message, persistence.message, gap
                            ),
                        });
                    }
                }
                return Err(err);
            }
        }
    };
    if first_apply {
        let current = workflow.as_ref().expect("skill workflow requires workflow");
        let runtime = skill_runtime.as_mut().expect("checked above");
        dispatch_workflow_skill_hook(current, runtime, "post_patch_apply", "apply_patch")?;
        dispatch_workflow_skill_hook(current, runtime, "post_tool_result", "apply_patch")?;
        runtime.record_stop_criterion("patch_applied");
        runtime.transition(skill::SkillState::AwaitingVerification)?;
    }
    let verification = if let Some(plan) = verification_plan.as_ref() {
        if let Some(current) = workflow.as_mut() {
            if current.phase != "verification-started" {
                return Err(AppError::blocked(format!(
                    "verification 시작 차단\n- 이유: prepared verification-started phase가 아닙니다.\n- phase: {}",
                    current.phase
                )));
            }
            if let Some(runtime) = skill_runtime.as_ref() {
                runtime.store_in_workflow(current);
            }
            if cfg!(debug_assertions)
                && std::env::var("RPOTATO_TEST_VERIFICATION_FAULT").as_deref()
                    == Ok("after-started-checkpoint")
            {
                return Err(AppError::runtime("injected verification-started crash"));
            }
        }
        if let (Some(current), Some(runtime)) = (workflow.as_ref(), skill_runtime.as_mut()) {
            dispatch_workflow_skill_hook(current, runtime, "pre_tool_call", "run_command")?;
            dispatch_workflow_skill_hook(current, runtime, "pre_command_run", "run_command")?;
        }
        let verification = run_verification(plan);
        if let (Some(current), Some(runtime)) = (workflow.as_ref(), skill_runtime.as_mut()) {
            dispatch_workflow_skill_hook(current, runtime, "post_command_run", "run_command")?;
            dispatch_workflow_skill_hook(current, runtime, "post_tool_result", "run_command")?;
        }
        if !verification.passed() {
            if cfg!(debug_assertions)
                && std::env::var("RPOTATO_TEST_ROLLBACK_FAULT").as_deref() == Ok("tamper-record")
            {
                let _ = fs::write(&apply.rollback_path, b"tampered rollback bytes");
            }
            let rollback = restore_from_rollback(&record, &apply.rollback_path);
            if rollback.status.starts_with("restore-conflict:") {
                state::record_validation_gap(
                    "rollback-source-conflict",
                    &format!("{}:{}", record.proposal_id, rollback.status),
                )?;
            }
            let actual_source_hash = current_source_hash(&record.relative_path)
                .unwrap_or_else(|_| "unreadable".to_string());
            if let Some(current) = workflow.as_mut() {
                let evidence = crate::evidence::record_patch_verification(
                    current,
                    &verification.command,
                    false,
                    &verification.exit_code,
                    &actual_source_hash,
                    &verification.stdout,
                    &verification.stderr,
                )?;
                current.evidence_id = evidence.evidence_id;
                current.evidence_hash = evidence.artifact_hash;
                if let Some(runtime) = skill_runtime.as_mut() {
                    let _ = runtime.transition(skill::SkillState::Failed);
                    runtime.store_in_workflow(current);
                }
                current.phase = "failed".to_string();
                current.failure_reason = if rollback.restored {
                    "verification-failed-rolled-back"
                } else {
                    "verification-failed-rollback-failed"
                }
                .to_string();
                *current = state::checkpoint_workflow(current.clone(), current.revision)?;
            }
            let event_type = if rollback.restored {
                "patch.verification.failed_rolled_back"
            } else {
                "patch.verification.failed_rollback_failed"
            };
            let event_id = state::record_event(
                event_type,
                "patch verification failed and rollback result was verified",
                &format!(
                    "proposal_id={} path={} command={} exit_code={} rollback={}",
                    record.proposal_id,
                    record.relative_path,
                    ledger::redact_text(&verification.command),
                    verification.exit_code,
                    rollback.status
                ),
            )?;
            let status = if rollback.restored {
                "verification-failed-rolled-back"
            } else {
                "verification-failed-rollback-failed"
            };
            return Err(AppError::blocked(format!(
                "패치 승인 실패\n- status: {}\n- proposal id: {}\n- path: {}\n- approval token: accepted\n- original sha256: {}\n- attempted sha256: {}\n- actual source sha256: {}\n- rollback record: {}\n- rollback status: {}\n- verification command: {}\n- verification exit code: {}\n- verification stdout: {}\n- verification stderr: {}\n- ledger event: {}\n- boundary: patch verification과 rollback 결과를 실제 bytes/hash로 확인했으며 성공으로 보고하지 않습니다.",
                status,
                record.proposal_id,
                record.relative_path,
                apply.original_sha256,
                apply.applied_sha256,
                actual_source_hash,
                apply.rollback_path.display(),
                rollback.status,
                ledger::redact_text(&verification.command),
                verification.exit_code,
                verification.stdout,
                verification.stderr,
                event_id
            )));
        }
        Some(verification)
    } else {
        None
    };

    let event_id = state::record_event(
        if verification.is_some() {
            "patch.verification.passed"
        } else {
            "patch.applied"
        },
        if verification.is_some() {
            "separately approved patch verification passed"
        } else {
            "approved patch applied"
        },
        &format!(
            "proposal_id={} path={} original_sha256={} applied_sha256={} verification={}",
            record.proposal_id,
            apply.relative_path,
            apply.original_sha256,
            apply.applied_sha256,
            verification
                .as_ref()
                .map(|result| result.exit_code.as_str())
                .unwrap_or("not-requested")
        ),
    )?;

    if let Some(current) = workflow.as_ref() {
        let updated_pointer = crate::context::SourcePointer {
            path: apply.relative_path.clone(),
            stable_ref: format!("{}:1", apply.relative_path),
            chars: 0,
            fingerprint: apply.applied_sha256.clone(),
            snippet: String::new(),
        };
        transcript::record_workflow_turn(
            current,
            "tool",
            &event_id,
            &format!(
                "patch applied: proposal_id={} path={} original_sha256={} applied_sha256={}",
                record.proposal_id,
                apply.relative_path,
                apply.original_sha256,
                apply.applied_sha256
            ),
            &[updated_pointer],
        )?;
    }

    if let (Some(current), Some(verification)) = (workflow.as_mut(), verification.as_ref()) {
        let evidence = crate::evidence::record_patch_verification(
            current,
            &verification.command,
            true,
            &verification.exit_code,
            &apply.applied_sha256,
            &verification.stdout,
            &verification.stderr,
        )?;
        current.evidence_id = evidence.evidence_id;
        current.evidence_hash = evidence.artifact_hash;
        if let Some(runtime) = skill_runtime.as_mut() {
            match runtime.active_skill_id.as_str() {
                "fix-test"
                    if verification_plan
                        .as_ref()
                        .is_some_and(verification_domain::is_test_plan) =>
                {
                    runtime.record_evidence("passing_test_after")
                }
                "small-patch" => runtime.record_evidence("targeted_verification"),
                _ => {}
            }
            runtime.record_stop_criterion("verification_passed");
            runtime.store_in_workflow(current);
        }
        current.phase = "verified".to_string();
        *current = state::checkpoint_workflow(current.clone(), current.revision)?;
        crate::evidence::evaluate_patch_stop_gate(current)?;
        finalize_verified_skill(current, skill_runtime.as_mut())?;
        current.phase = "complete".to_string();
        *current = state::checkpoint_workflow(current.clone(), current.revision)?;
        state::clear_terminal_workflow_pointer(current)?;
        return Ok(success_report(current));
    }

    if workflow.is_some() {
        return Err(AppError::blocked(
            "prepared verification plan 없이 workflow approval을 계속할 수 없습니다.",
        ));
    }

    Ok(format!(
        "patch approve\n- status: applied\n- proposal id: {}\n- path: {}\n- dry-run: false\n- approval token: accepted\n- original sha256: {}\n- applied sha256: {}\n- rollback record: {}\n- verification status: {}\n{}- ledger event: {}\n- boundary: 승인된 patch를 적용했습니다. verification command가 지정된 경우 allow 정책을 통과한 단순 argv 명령만 실행합니다.",
        record.proposal_id,
        apply.relative_path,
        apply.original_sha256,
        apply.applied_sha256,
        apply.rollback_path.display(),
        verification
            .as_ref()
            .map(|_| "passed")
            .unwrap_or("not-requested"),
        verification
            .as_ref()
            .map(format_verification_result)
            .unwrap_or_default(),
        event_id
    ))
}

pub(super) fn workflow_skill_runtime(
    workflow: &state::WorkflowRecord,
) -> Result<Option<skill::SkillRuntimeState>, AppError> {
    if workflow.active_skill_id.is_empty() {
        return Ok(None);
    }
    skill::SkillRuntimeState::from_workflow(workflow).map(Some)
}

fn validate_skill_phase_for_side_effect(
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

pub(super) fn validate_failing_test_before(
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

pub(super) fn validate_completed_workflow(
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
    crate::evidence::validate_patch_stop_gate(workflow)
}

pub(super) fn validate_completed_plugin_workflow(
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
    let imported = plugin::revalidate_completed_codex_skill(
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

pub(super) fn ensure_plugin_completion_event(
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
        "instruction-only Codex plugin skill 실행 완료",
        &plugin_completion_event_details(workflow, imported),
    )?;
    Ok(())
}

pub(super) fn ensure_plugin_completion_event_under_transition(
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
        "instruction-only Codex plugin skill 실행 완료",
        &plugin_completion_event_details(workflow, imported),
    )?;
    Ok(())
}

pub(super) fn plugin_completion_recovery_report(workflow: &state::WorkflowRecord) -> String {
    format!(
        "plugin capability 복구 완료\n- 결과: 성공\n- workflow id: {}\n- skill id: {}\n- source: {}@{}\n- side effect: 없음\n- completion event: 확인됨\n- active pointer: 정리됨",
        workflow.workflow_id,
        workflow.active_skill_id,
        workflow.source_path,
        workflow.source_hash
    )
}

fn dispatch_workflow_skill_hook(
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

pub(super) fn finalize_verified_skill(
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
