use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::time::SystemTime;

use sha2::{Digest, Sha256};

use crate::app::AppError;
use crate::ledger;
use crate::paths;
use crate::policy::{self, Decision, PathMode};
use crate::state;

const MAX_PATCH_FILE_BYTES: u64 = 256 * 1024;
const MAX_VERIFICATION_OUTPUT_CHARS: usize = 2_000;

#[derive(Debug, Clone, PartialEq, Eq)]
struct PatchPreview {
    proposal_id: String,
    approval_token: String,
    relative_path: String,
    original_sha256: String,
    proposed_sha256: String,
    replacements: usize,
    diff: String,
    proposal_path: PathBuf,
    proposed_content: String,
    workflow_id: String,
    action_id: String,
    verification_command: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProposalRecord {
    proposal_id: String,
    approval_token_hash: String,
    relative_path: String,
    original_sha256: String,
    proposed_sha256: String,
    proposed_content: String,
    proposal_path: PathBuf,
    workflow_id: String,
    action_id: String,
    verification_command: String,
    artifact_hash: String,
    legacy_plaintext_token: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowProposal {
    pub proposal_id: String,
    pub approval_token: String,
    pub relative_path: String,
    pub original_sha256: String,
    pub proposed_sha256: String,
    pub diff: String,
    pub verification_command: String,
    pub proposal_hash: String,
    pub approval_credential_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchProposalSummary {
    pub proposal_id: String,
    pub relative_path: String,
    pub original_sha256: String,
    pub proposed_sha256: String,
    pub replacements: String,
    pub status: String,
    pub proposal_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchProposalDetail {
    pub summary: PatchProposalSummary,
    pub diff: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ApplyResult {
    relative_path: String,
    original_sha256: String,
    applied_sha256: String,
    rollback_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RollbackResult {
    restored: bool,
    status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VerificationPlan {
    command: String,
    argv: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VerificationResult {
    command: String,
    exit_code: String,
    stdout: String,
    stderr: String,
}

impl VerificationResult {
    fn passed(&self) -> bool {
        self.exit_code == "0"
    }
}

pub fn preview_report(path: &str, find: &str, replace: &str) -> Result<String, AppError> {
    let preview = build_preview(path, find, replace, "", "", "")?;
    write_proposal_record(&preview)?;
    let event_id = state::record_event(
        "patch.preview.prepared",
        "patch diff preview prepared",
        &format!(
            "proposal_id={} path={} replacements={} original_sha256={} proposed_sha256={} proposal_path={}",
            preview.proposal_id,
            preview.relative_path,
            preview.replacements,
            preview.original_sha256,
            preview.proposed_sha256,
            preview.proposal_path.display()
        ),
    )?;

    Ok(format!(
        "patch preview\n- status: diff-only\n- path: {}\n- proposal id: {}\n- replacements: {}\n- original sha256: {}\n- proposed sha256: {}\n- approval required: 불가\n- proposal record: {}\n- write gate: canonical-workflow-only\n- ledger event: {}\n- boundary: standalone preview는 diff 표시 전용이며 approve/apply/verification을 수행할 수 없습니다. 실제 변경은 rpotato run이 만든 canonical workflow proposal만 허용합니다.\n- diff:\n{}",
        preview.relative_path,
        preview.proposal_id,
        preview.replacements,
        preview.original_sha256,
        preview.proposed_sha256,
        preview.proposal_path.display(),
        event_id,
        preview.diff
    ))
}

pub fn prepare_workflow_proposal(
    workflow_id: &str,
    action_id: &str,
    path: &str,
    find: &str,
    replace: &str,
    verification_command: &str,
) -> Result<WorkflowProposal, AppError> {
    build_verification_plan(verification_command)?;
    let preview = build_preview(
        path,
        find,
        replace,
        workflow_id,
        action_id,
        verification_command,
    )?;
    write_proposal_record(&preview)?;
    let proposal_bytes = fs::read(&preview.proposal_path)
        .map_err(|err| AppError::runtime(format!("proposal hash reread 실패: {err}")))?;
    let approval_credential_hash = sha256_text(&preview.approval_token);
    Ok(WorkflowProposal {
        proposal_id: preview.proposal_id,
        approval_token: preview.approval_token,
        relative_path: preview.relative_path,
        original_sha256: preview.original_sha256,
        proposed_sha256: preview.proposed_sha256,
        diff: preview.diff,
        verification_command: preview.verification_command,
        proposal_hash: sha256_bytes(&proposal_bytes),
        approval_credential_hash,
    })
}

pub fn approve_report(
    proposal_id: &str,
    token: &str,
    dry_run: bool,
    verify_command: Option<&str>,
) -> Result<String, AppError> {
    validate_proposal_id(proposal_id)?;
    let proposal_path = paths::project_patch_proposals_dir().join(format!("{proposal_id}.txt"));
    let record = load_proposal_record(proposal_id, &proposal_path)?;
    if record.workflow_id.is_empty() {
        return Err(AppError::blocked(
            "patch approve 차단\n- 이유: standalone preview는 diff-only이며 mutation 권위가 없습니다.\n- 동작: rpotato run으로 canonical workflow proposal을 생성하세요.",
        ));
    }

    if verify_command.is_some() {
        return Err(AppError::blocked(
            "patch approve 차단\n- 이유: patch 적용 승인과 verification command 승인은 분리되어 있습니다.\n- 동작: patch approve 후 발급되는 credential로 rpotato patch verify를 실행하세요.",
        ));
    }

    if dry_run {
        let discovered_active = state::active_workflow_id()?;
        let workflow =
            load_validated_approval_workflow(&record, token, discovered_active.as_deref())?;
        if workflow.phase == "complete" {
            crate::evidence::validate_patch_stop_gate(&workflow)?;
            state::clear_terminal_workflow_pointer(&workflow)?;
            return Ok(success_report(&workflow));
        }
        if workflow.phase == "failed" {
            return Err(AppError::blocked(failure_report(&workflow)));
        }
        return dry_run_approval_report(&record, verify_command);
    }

    approval_prelock_test_barrier()?;
    let _approval_lock = ApprovalLock::acquire(&record.proposal_id)?;
    let discovered_active = state::active_workflow_id()?;
    let mut workflow =
        load_validated_approval_workflow(&record, token, discovered_active.as_deref())?;
    if workflow.phase == "complete" {
        crate::evidence::validate_patch_stop_gate(&workflow)?;
        state::clear_terminal_workflow_pointer(&workflow)?;
        return Ok(success_report(&workflow));
    }
    if workflow.phase == "failed" {
        return Err(AppError::blocked(failure_report(&workflow)));
    }

    if workflow.phase == "pending-approval" {
        workflow.phase = "approved".to_string();
        workflow.approval_state = "approved".to_string();
        workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision)?;
    } else if workflow.phase != "approved" {
        return Err(AppError::blocked(format!(
            "patch approve 차단\n- 이유: workflow phase가 approval/apply 재개를 허용하지 않습니다.\n- phase: {}",
            workflow.phase
        )));
    }

    continue_approved_workflow(record, Some(workflow), None)
}

pub fn verify_report(proposal_id: &str, token: &str) -> Result<String, AppError> {
    validate_proposal_id(proposal_id)?;
    let _approval_lock = ApprovalLock::acquire(proposal_id)?;
    let active = state::active_workflow_id()?;
    let proposal_path = paths::project_patch_proposals_dir().join(format!("{proposal_id}.txt"));
    let record = load_proposal_record(proposal_id, &proposal_path)?;
    if record.workflow_id.is_empty() {
        return Err(AppError::blocked(
            "patch verify 차단\n- 이유: workflow proposal만 verification을 실행할 수 있습니다.",
        ));
    }
    let mut workflow = state::load_workflow(&record.workflow_id)?;
    if !workflow.is_terminal() && active.as_deref() != Some(record.workflow_id.as_str()) {
        return Err(AppError::blocked(
            "patch verify 차단\n- 이유: active workflow/current pointer가 일치하지 않습니다.",
        ));
    }
    validate_workflow_binding(&workflow, &record)?;
    validate_token_hash(&workflow.verification_credential_hash, token, &record)?;
    if workflow.phase == "complete" {
        crate::evidence::validate_patch_stop_gate(&workflow)?;
        state::clear_terminal_workflow_pointer(&workflow)?;
        return Ok(success_report(&workflow));
    }
    if workflow.phase == "failed" {
        return Err(AppError::blocked(failure_report(&workflow)));
    }
    if workflow.phase != "pending-verification-approval"
        && workflow.phase != "verification-approved"
    {
        return Err(AppError::blocked(format!(
            "patch verify 차단\n- 이유: verification approval을 받을 수 없는 phase입니다.\n- phase: {}",
            workflow.phase
        )));
    }
    if workflow.phase == "pending-verification-approval" {
        workflow.phase = "verification-approved".to_string();
        workflow.verification_approval_state = "approved".to_string();
        workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision)?;
    }
    let plan = build_verification_plan(&record.verification_command)?;
    continue_approved_workflow(record, Some(workflow), Some(plan))
}

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
            workflow.approval_credential_hash = sha256_text(&token);
            workflow.approval_state = "pending-rotated".to_string();
            "patch-apply"
        }
        "pending-verification-approval" => {
            workflow.verification_credential_hash = sha256_text(&token);
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

fn continue_approved_workflow(
    record: ProposalRecord,
    mut workflow: Option<state::WorkflowRecord>,
    verification_plan: Option<VerificationPlan>,
) -> Result<String, AppError> {
    let apply = match apply_proposal(&record) {
        Ok(apply) => apply,
        Err(err) => {
            if let Some(current) = workflow.as_mut() {
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
    };
    let verification = if let Some(plan) = verification_plan.as_ref() {
        if let Some(current) = workflow.as_mut() {
            if current.phase != "verification-approved" {
                return Err(AppError::blocked(format!(
                    "verification 시작 차단\n- 이유: verification-approved phase가 아닙니다.\n- phase: {}",
                    current.phase
                )));
            }
            current.phase = "verification-started".to_string();
            *current = state::checkpoint_workflow(current.clone(), current.revision)?;
            if cfg!(debug_assertions)
                && std::env::var("RPOTATO_TEST_VERIFICATION_FAULT").as_deref()
                    == Ok("after-started-checkpoint")
            {
                return Err(AppError::runtime("injected verification-started crash"));
            }
        }
        let verification = run_verification(plan);
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
        current.phase = "verified".to_string();
        *current = state::checkpoint_workflow(current.clone(), current.revision)?;
        crate::evidence::evaluate_patch_stop_gate(current)?;
        current.phase = "complete".to_string();
        *current = state::checkpoint_workflow(current.clone(), current.revision)?;
        state::clear_terminal_workflow_pointer(current)?;
        return Ok(success_report(current));
    }

    if let Some(current) = workflow.as_mut() {
        let verification_token = issue_approval_token()?;
        current.phase = "pending-verification-approval".to_string();
        current.approval_state = "applied".to_string();
        current.verification_credential_hash = sha256_text(&verification_token);
        current.verification_approval_state = "pending".to_string();
        current.result_summary = "patch applied; verification approval pending".to_string();
        *current = state::checkpoint_workflow(current.clone(), current.revision)?;
        return Ok(format!(
            "patch approve\n- status: applied-awaiting-verification\n- proposal id: {}\n- path: {}\n- approval token: accepted\n- applied sha256: {}\n- verification command: {}\n- verification approval: required\n- verification command approval: rpotato patch verify {} --token {}\n- ledger event: {}\n- boundary: patch만 적용했으며 verification command는 아직 실행하지 않았습니다.",
            record.proposal_id,
            apply.relative_path,
            apply.applied_sha256,
            ledger::redact_text(&record.verification_command),
            record.proposal_id,
            verification_token,
            event_id
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

pub fn proposal_summaries(limit: usize) -> Result<Vec<PatchProposalSummary>, AppError> {
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
    for entry in entries {
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

pub fn resume_workflow_report(workflow_id: &str) -> Result<String, AppError> {
    let (mut workflow, _approval_lock) = load_workflow_under_approval_lock(workflow_id)?;
    match workflow.phase.as_str() {
        "model-pending" | "action-recorded" => {
            workflow.failure_reason = format!("resume-incomplete-{}", workflow.phase);
            workflow.phase = "failed".to_string();
            workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision)?;
            Err(AppError::blocked(format!("workflow 재개 실패\n- workflow id: {}\n- 이유: 중간 phase는 backend 또는 command를 자동 재실행하지 않습니다.\n- terminal phase: failed\n- validation gap: {}", workflow.workflow_id, workflow.failure_reason)))
        }
        "pending-approval" => {
            let detail = proposal_detail(&workflow.proposal_id)?;
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
        "approved" => {
            let proposal_path = paths::project_patch_proposals_dir()
                .join(format!("{}.txt", workflow.proposal_id));
            let record = load_proposal_record(&workflow.proposal_id, &proposal_path)?;
            validate_workflow_binding(&workflow, &record)?;
            continue_approved_workflow(record, Some(workflow), None)
        }
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
        "verification-approved" => {
            let proposal_path = paths::project_patch_proposals_dir()
                .join(format!("{}.txt", workflow.proposal_id));
            let record = load_proposal_record(&workflow.proposal_id, &proposal_path)?;
            validate_workflow_binding(&workflow, &record)?;
            let plan = build_verification_plan(&workflow.verification_plan)?;
            continue_approved_workflow(record, Some(workflow), Some(plan))
        }
        "verification-started" => Err(AppError::blocked(
            {
                state::record_validation_gap(
                    "verification-inconclusive",
                    &format!("{}:verification-started", workflow.workflow_id),
                )?;
                "workflow 재개 차단\n- 이유: verification 시작 checkpoint 뒤 결과가 확정되지 않았습니다.\n- validation gap: verification-inconclusive\n- 동작: command를 자동 재실행하지 않습니다. `rpotato cancel`로 명시적으로 정리하세요."
            },
        )),
        "verified" => {
            crate::evidence::evaluate_patch_stop_gate(&workflow)?;
            workflow.phase = "complete".to_string();
            workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision)?;
            state::clear_terminal_workflow_pointer(&workflow)?;
            Ok(success_report(&workflow))
        }
        "complete" => {
            let proposal_path = paths::project_patch_proposals_dir()
                .join(format!("{}.txt", workflow.proposal_id));
            let record = load_proposal_record(&workflow.proposal_id, &proposal_path)?;
            validate_workflow_binding(&workflow, &record)?;
            crate::evidence::validate_patch_stop_gate(&workflow)?;
            state::clear_terminal_workflow_pointer(&workflow)?;
            Ok(success_report(&workflow))
        }
        "failed" | "cancelled" => Err(AppError::blocked(failure_report(&workflow))),
        other => Err(AppError::blocked(format!(
            "workflow resume 차단\n- 이유: 안전하게 재개할 수 없는 phase입니다.\n- phase: {other}\n- backend 호출: 없음"
        ))),
    }
}

pub fn cancel_workflow_report(workflow_id: &str) -> Result<String, AppError> {
    let (mut workflow, _approval_lock) = load_workflow_under_approval_lock(workflow_id)?;
    if workflow.phase == "complete" {
        let proposal_path =
            paths::project_patch_proposals_dir().join(format!("{}.txt", workflow.proposal_id));
        let record = load_proposal_record(&workflow.proposal_id, &proposal_path)?;
        validate_workflow_binding(&workflow, &record)?;
        crate::evidence::validate_patch_stop_gate(&workflow)?;
        state::clear_terminal_workflow_pointer(&workflow)?;
        return Err(AppError::blocked(
            "cancel 차단\n- 이유: 완료된 workflow는 취소할 수 없습니다.",
        ));
    }
    if matches!(workflow.phase.as_str(), "failed" | "cancelled") {
        state::clear_terminal_workflow_pointer(&workflow)?;
        return Ok(failure_report(&workflow));
    }
    if matches!(
        workflow.phase.as_str(),
        "approved"
            | "pending-verification-approval"
            | "verification-approved"
            | "verification-started"
            | "verified"
    ) {
        let proposal_path =
            paths::project_patch_proposals_dir().join(format!("{}.txt", workflow.proposal_id));
        let record = load_proposal_record(&workflow.proposal_id, &proposal_path)?;
        validate_workflow_binding(&workflow, &record)?;
        let rollback_path =
            proposal_path.with_file_name(format!("{}.rollback", record.proposal_id));
        let rollback = restore_from_rollback(&record, &rollback_path);
        if !rollback.restored {
            state::record_validation_gap(
                "cancel-rollback-conflict",
                &format!("{}:{}", workflow.workflow_id, rollback.status),
            )?;
            return Err(AppError::blocked(format!(
                "workflow cancel 차단\n- 이유: 적용된 source를 안전하게 복원하지 못했습니다.\n- rollback: {}\n- pointer: 유지",
                rollback.status
            )));
        }
    }
    workflow.phase = "cancelled".to_string();
    workflow.failure_reason = "user-cancelled".to_string();
    workflow.approval_state = "cancelled".to_string();
    workflow.verification_approval_state = "cancelled".to_string();
    workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision)?;
    state::record_validation_gap(
        "workflow-user-cancelled",
        &format!("{}:{}", workflow.workflow_id, workflow.phase),
    )?;
    state::clear_terminal_workflow_pointer(&workflow)?;
    Ok(format!(
        "workflow 취소 완료\n- workflow id: {}\n- phase: cancelled\n- source 복원: 검증됨 또는 적용 전\n- backend/verification 재실행: 없음",
        workflow.workflow_id
    ))
}

fn validate_workflow_binding(
    workflow: &state::WorkflowRecord,
    record: &ProposalRecord,
) -> Result<(), AppError> {
    if workflow.action_id != record.action_id
        || workflow.proposal_id != record.proposal_id
        || workflow.source_path != record.relative_path
        || workflow.before_hash != record.original_sha256
        || workflow.after_hash != record.proposed_sha256
        || workflow.verification_plan != record.verification_command
        || workflow.proposal_hash != record.artifact_hash
        || workflow.approval_state == "unresolved-conflict"
    {
        return Err(AppError::blocked(
            "patch approve 차단\n- 이유: workflow/action/proposal/hash/verification binding이 일치하지 않습니다.",
        ));
    }
    let source = fs::read_to_string(paths::project_root().join(&record.relative_path))
        .map_err(|err| AppError::blocked(format!("approval source reread 실패: {err}")))?;
    let current_hash = sha256_text(&source);
    let allowed = current_hash == record.original_sha256
        || (matches!(
            workflow.phase.as_str(),
            "approved"
                | "pending-verification-approval"
                | "verification-approved"
                | "verification-started"
                | "verified"
                | "complete"
        ) && current_hash == record.proposed_sha256);
    if !allowed {
        return Err(AppError::blocked(format!(
            "patch approve 차단\n- 이유: 대상 파일이 preview 이후 변경되었습니다.\n- expected original sha256: {}\n- current sha256: {}",
            record.original_sha256, current_hash
        )));
    }
    Ok(())
}

fn load_validated_approval_workflow(
    record: &ProposalRecord,
    token: &str,
    active_workflow_id: Option<&str>,
) -> Result<state::WorkflowRecord, AppError> {
    let workflow = state::load_workflow(&record.workflow_id)?;
    if !workflow.is_terminal() && active_workflow_id != Some(record.workflow_id.as_str()) {
        return Err(AppError::blocked(
            "patch approve 차단\n- 이유: current pointer/active workflow가 proposal workflow와 일치하지 않습니다.",
        ));
    }
    validate_workflow_binding(&workflow, record)?;
    validate_token_hash(&workflow.approval_credential_hash, token, record)?;
    Ok(workflow)
}

fn success_report(workflow: &state::WorkflowRecord) -> String {
    format!(
        "패치 작업 완료\n- 결과: 성공\n- workflow id: {}\n- action id: {}\n- proposal id: {}\n- 적용 파일: {}\n- 적용 SHA-256: {}\n- 검증: 통과 ({})\n- evidence id: {}\n- stop gate: 통과\n- 미해결 승인: 없음",
        workflow.workflow_id,
        workflow.action_id,
        workflow.proposal_id,
        workflow.source_path,
        workflow.after_hash,
        ledger::redact_text(&workflow.verification_plan),
        workflow.evidence_id
    )
}

fn failure_report(workflow: &state::WorkflowRecord) -> String {
    format!(
        "패치 작업 실패\n- 결과: 실패\n- workflow id: {}\n- proposal id: {}\n- 이유: {}\n- 성공 보고: 차단",
        workflow.workflow_id,
        display_none(&workflow.proposal_id),
        display_none(&workflow.failure_reason)
    )
}

pub fn proposal_detail(proposal_id: &str) -> Result<PatchProposalDetail, AppError> {
    validate_proposal_id(proposal_id)?;
    let proposal_path = paths::project_patch_proposals_dir().join(format!("{proposal_id}.txt"));
    let contents = fs::read_to_string(&proposal_path).map_err(|err| {
        AppError::blocked(format!(
            "patch proposal detail 차단\n- 이유: proposal record를 읽지 못했습니다.\n- proposal id: {}\n- path: {}\n- error: {}",
            proposal_id,
            proposal_path.display(),
            err
        ))
    })?;
    let (header, diff) = parse_proposal_header(&contents, &proposal_path)?;
    let recorded_id = required_header(&header, "proposal_id", &proposal_path)?;
    if recorded_id != proposal_id {
        return Err(AppError::blocked(format!(
            "patch proposal detail 차단\n- 이유: proposal id가 record와 일치하지 않습니다.\n- requested: {}\n- recorded: {}",
            proposal_id, recorded_id
        )));
    }

    Ok(PatchProposalDetail {
        summary: summary_from_header(&proposal_path, &header)?,
        diff: diff.trim_end().to_string(),
    })
}

fn summary_from_path(path: &Path) -> Result<PatchProposalSummary, AppError> {
    let contents = fs::read_to_string(path).map_err(|err| {
        AppError::runtime(format!(
            "patch proposal record를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    summary_from_record(path, &contents)
}

fn summary_from_record(path: &Path, contents: &str) -> Result<PatchProposalSummary, AppError> {
    let (header, _) = parse_proposal_header(contents, path)?;
    summary_from_header(path, &header)
}

fn summary_from_header(
    path: &Path,
    header: &std::collections::BTreeMap<String, String>,
) -> Result<PatchProposalSummary, AppError> {
    let proposal_id = required_header(header, "proposal_id", path)?;
    Ok(PatchProposalSummary {
        status: proposal_status(&proposal_id),
        proposal_id,
        relative_path: required_header(header, "path", path)?,
        original_sha256: required_header(header, "original_sha256", path)?,
        proposed_sha256: required_header(header, "proposed_sha256", path)?,
        replacements: header
            .get("replacements")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string()),
        proposal_path: path.to_path_buf(),
    })
}

fn proposal_status(proposal_id: &str) -> String {
    let rollback_path =
        paths::project_patch_proposals_dir().join(format!("{proposal_id}.rollback"));
    if rollback_path.exists() {
        "applied".to_string()
    } else {
        "pending-approval".to_string()
    }
}

fn load_proposal_record(
    proposal_id: &str,
    proposal_path: &Path,
) -> Result<ProposalRecord, AppError> {
    let contents = fs::read_to_string(proposal_path).map_err(|err| {
        AppError::blocked(format!(
            "patch approve 차단\n- 이유: proposal record를 읽지 못했습니다.\n- proposal id: {}\n- path: {}\n- error: {}",
            proposal_id,
            proposal_path.display(),
            err
        ))
    })?;
    let (header, _) = parse_proposal_header(&contents, proposal_path)?;
    let recorded_id = required_header(&header, "proposal_id", proposal_path)?;
    if recorded_id != proposal_id {
        return Err(AppError::blocked(format!(
            "patch approve 차단\n- 이유: proposal id가 record와 일치하지 않습니다.\n- requested: {}\n- recorded: {}",
            proposal_id, recorded_id
        )));
    }
    let proposed_sha256 = required_header(&header, "proposed_sha256", proposal_path)?;
    let proposed_content_hex =
        required_header(&header, "proposed_content_hex", proposal_path).map_err(|_| {
            AppError::blocked(format!(
                "patch approve 차단\n- 이유: v0.4.0 apply에는 proposed_content_hex가 필요합니다.\n- path: {}\n- 동작: patch preview를 다시 생성하세요.",
                proposal_path.display()
            ))
        })?;
    let proposed_content = decode_hex_text(&proposed_content_hex).map_err(|message| {
        AppError::blocked(format!(
            "patch approve 차단\n- 이유: proposal record의 proposed_content_hex를 해석하지 못했습니다.\n- path: {}\n- error: {}",
            proposal_path.display(),
            message
        ))
    })?;
    let decoded_sha256 = sha256_text(&proposed_content);
    if decoded_sha256 != proposed_sha256 {
        return Err(AppError::blocked(format!(
            "patch approve 차단\n- 이유: proposal record의 proposed content hash가 일치하지 않습니다.\n- expected: {}\n- actual: {}",
            proposed_sha256, decoded_sha256
        )));
    }

    let version = required_header(&header, "record_version", proposal_path)?;
    let legacy_plaintext_token = version == "2";
    if !matches!(version.as_str(), "2" | "4") {
        return Err(AppError::blocked(
            "patch approve 차단\n- 이유: 지원하지 않는 proposal record version입니다.",
        ));
    }
    if legacy_plaintext_token {
        if header.contains_key("approval_token_hash") {
            return Err(AppError::blocked(
                "proposal strict parse 차단\n- 이유: v2 record에 hash credential이 함께 존재합니다.",
            ));
        }
        let plaintext = required_header(&header, "approval_token", proposal_path)?;
        let scrubbed = contents
            .replacen("record_version=2", "record_version=4", 1)
            .replacen(
                &format!("approval_token={plaintext}"),
                &format!("approval_token_hash={}", sha256_text(&plaintext)),
                1,
            );
        state::atomic_replace_bytes(proposal_path, scrubbed.as_bytes())?;
        return Err(AppError::blocked(
            "legacy proposal migration 완료\n- plaintext token을 hash-only로 atomic scrub했습니다.\n- 동작: 기존 binding은 폐기하고 canonical workflow preview를 다시 생성하세요.",
        ));
    } else if header.contains_key("approval_token") {
        return Err(AppError::blocked(
            "proposal strict parse 차단\n- 이유: v4 record에 plaintext credential이 존재합니다.",
        ));
    }
    let approval_token_hash = required_header(&header, "approval_token_hash", proposal_path)?;
    Ok(ProposalRecord {
        proposal_id: recorded_id,
        approval_token_hash,
        relative_path: required_header(&header, "path", proposal_path)?,
        original_sha256: required_header(&header, "original_sha256", proposal_path)?,
        proposed_sha256,
        proposed_content,
        proposal_path: proposal_path.to_path_buf(),
        workflow_id: header.get("workflow_id").cloned().unwrap_or_default(),
        action_id: header.get("action_id").cloned().unwrap_or_default(),
        verification_command: header
            .get("verification_command_hex")
            .cloned()
            .map(|value| decode_hex_text(&value))
            .transpose()
            .map_err(|message| {
                AppError::blocked(format!("verification plan decode 실패: {message}"))
            })?
            .unwrap_or_default(),
        artifact_hash: sha256_bytes(contents.as_bytes()),
        legacy_plaintext_token,
    })
}

fn parse_proposal_header<'a>(
    contents: &'a str,
    path: &Path,
) -> Result<(std::collections::BTreeMap<String, String>, &'a str), AppError> {
    const ALLOWED: &[&str] = &[
        "record_version",
        "proposal_id",
        "workflow_id",
        "action_id",
        "path",
        "approval_token_hash",
        "approval_token",
        "original_sha256",
        "proposed_sha256",
        "verification_command_hex",
        "replacements",
        "content_encoding",
        "proposed_content_hex",
    ];
    let (head, diff) = contents.split_once("\n\n").ok_or_else(|| {
        AppError::blocked(format!(
            "proposal strict parse 차단\n- path: {}\n- 이유: header terminator 없음",
            path.display()
        ))
    })?;
    let mut map = std::collections::BTreeMap::new();
    for line in head.lines() {
        let (key, value) = line.split_once('=').ok_or_else(|| {
            AppError::blocked("proposal strict parse 차단\n- 이유: malformed field")
        })?;
        if !ALLOWED.contains(&key) {
            return Err(AppError::blocked(format!(
                "proposal strict parse 차단\n- 이유: unknown key: {key}"
            )));
        }
        if map.insert(key.to_string(), value.to_string()).is_some() {
            return Err(AppError::blocked(format!(
                "proposal strict parse 차단\n- 이유: duplicate key: {key}"
            )));
        }
    }
    Ok((map, diff))
}

fn required_header(
    map: &std::collections::BTreeMap<String, String>,
    key: &str,
    path: &Path,
) -> Result<String, AppError> {
    map.get(key).cloned().ok_or_else(|| {
        AppError::blocked(format!(
            "patch approve 차단\n- 이유: proposal record에 {key} 값이 없습니다.\n- path: {}",
            path.display()
        ))
    })
}

fn validate_token_hash(
    expected_hash: &str,
    token: &str,
    record: &ProposalRecord,
) -> Result<(), AppError> {
    if constant_time_eq(expected_hash, &sha256_text(token)) {
        return Ok(());
    }

    if let Err(persistence) = state::record_event(
        "patch.approval.rejected",
        "patch approval token rejected",
        &format!(
            "proposal_id={} workflow_id={} reason=token-mismatch",
            record.proposal_id,
            display_none(&record.workflow_id)
        ),
    ) {
        return Err(AppError::runtime(format!(
            "patch approval token mismatch; rejection event 저장 실패: {}",
            persistence.message
        )));
    }

    Err(AppError::blocked(format!(
        "patch approve 차단\n- 이유: approval token 불일치\n- proposal id: {}\n- approval prompt: 사용자 승인 필요",
        record.proposal_id
    )))
}

fn constant_time_eq(left: &str, right: &str) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.bytes()
        .zip(right.bytes())
        .fold(0_u8, |difference, (a, b)| difference | (a ^ b))
        == 0
}

fn dry_run_approval_report(
    record: &ProposalRecord,
    verify_command: Option<&str>,
) -> Result<String, AppError> {
    let event_id = state::record_event(
        "patch.approval.gate.passed",
        "patch approval gate passed",
        &format!(
            "proposal_id={} path={} dry_run=true proposal_path={} verify_command={}",
            record.proposal_id,
            record.relative_path,
            record.proposal_path.display(),
            verify_command
                .map(ledger::redact_text)
                .unwrap_or_else(|| "not-requested".to_string())
        ),
    )?;

    Ok(format!(
        "patch approve\n- status: gate-passed\n- proposal id: {}\n- path: {}\n- dry-run: true\n- approval token: accepted\n- proposal record: {}\n- verification command: {}\n- ledger event: {}\n- boundary: approval gate만 확인했습니다. --dry-run에서는 대상 파일 수정과 verification command 실행을 수행하지 않습니다.",
        record.proposal_id,
        record.relative_path,
        record.proposal_path.display(),
        verify_command
            .map(|command| format!("planned ({})", ledger::redact_text(command)))
            .unwrap_or_else(|| "not-requested".to_string()),
        event_id
    ))
}

fn apply_proposal(record: &ProposalRecord) -> Result<ApplyResult, AppError> {
    let target = resolve_target_for("patch approve", &record.relative_path)?;
    let read_decision = policy::classify_path(PathMode::Read, &target.relative_path)?;
    if read_decision.decision != Decision::Allow {
        return Err(AppError::blocked(format!(
            "patch approve 차단\n- 이유: target read policy가 allow가 아닙니다.\n- path: {}\n- decision: {}",
            target.relative_path,
            read_decision_label(read_decision.decision)
        )));
    }
    let write_decision = policy::classify_path(PathMode::Write, &target.relative_path)?;
    if write_decision.decision == Decision::Deny {
        return Err(AppError::blocked(format!(
            "patch approve 차단\n- 이유: target write policy가 deny입니다.\n- path: {}",
            target.relative_path
        )));
    }
    let metadata = fs::metadata(&target.absolute_path).map_err(|err| {
        AppError::runtime(format!(
            "patch approve 대상 파일 metadata를 읽지 못했습니다: {} ({err})",
            target.relative_path
        ))
    })?;
    if !metadata.is_file() {
        return Err(AppError::usage(format!(
            "patch approve 대상은 file이어야 합니다: {}",
            target.relative_path
        )));
    }
    if metadata.len() > MAX_PATCH_FILE_BYTES {
        return Err(AppError::blocked(format!(
            "patch approve 차단\n- 이유: 대상 파일이 patch 한도를 초과했습니다.\n- path: {}\n- size bytes: {}\n- max bytes: {}",
            target.relative_path,
            metadata.len(),
            MAX_PATCH_FILE_BYTES
        )));
    }

    let current = fs::read_to_string(&target.absolute_path).map_err(|err| {
        AppError::runtime(format!(
            "patch approve 대상 파일을 UTF-8 text로 읽지 못했습니다: {} ({err})",
            target.relative_path
        ))
    })?;
    let current_sha256 = sha256_text(&current);
    let rollback_path = record
        .proposal_path
        .with_file_name(format!("{}.rollback", record.proposal_id));
    if current_sha256 == record.proposed_sha256 && rollback_path.is_file() {
        let rollback_bytes = fs::read(&rollback_path).map_err(|err| {
            AppError::blocked(format!(
                "patch approve 차단\n- 이유: rollback record를 읽지 못했습니다.\n- error: {err}"
            ))
        })?;
        if sha256_bytes(&rollback_bytes) != record.original_sha256 {
            return Err(AppError::blocked(
                "patch approve 차단\n- 이유: rollback record hash가 original hash와 일치하지 않습니다.",
            ));
        }
        return Ok(ApplyResult {
            relative_path: target.relative_path,
            original_sha256: record.original_sha256.clone(),
            applied_sha256: record.proposed_sha256.clone(),
            rollback_path,
        });
    }
    if current_sha256 != record.original_sha256 {
        return Err(AppError::blocked(format!(
            "patch approve 차단\n- 이유: 대상 파일이 preview 이후 변경되었습니다.\n- path: {}\n- expected original sha256: {}\n- current sha256: {}\n- 동작: patch preview를 다시 생성하세요.",
            target.relative_path, record.original_sha256, current_sha256
        )));
    }

    state::atomic_replace_bytes(&rollback_path, current.as_bytes())?;
    let rollback_bytes = fs::read(&rollback_path).map_err(|err| {
        AppError::runtime(format!(
            "patch rollback record를 다시 읽지 못했습니다: {} ({err})",
            rollback_path.display()
        ))
    })?;
    if sha256_bytes(&rollback_bytes) != record.original_sha256 {
        return Err(AppError::blocked(
            "patch approve 차단\n- 이유: rollback record bytes hash 검증에 실패했습니다.",
        ));
    }

    let source_transaction = record
        .proposal_path
        .with_file_name(format!("{}.source.txn", record.proposal_id));
    if let Err(err) = state::guarded_source_replace(
        &target.absolute_path,
        &record.original_sha256,
        record.proposed_content.as_bytes(),
        &record.proposed_sha256,
        &source_transaction,
    ) {
        let rollback = restore_bytes(
            &target.absolute_path,
            current.as_bytes(),
            &record.proposed_sha256,
            &record.original_sha256,
            &source_transaction,
        );
        return Err(AppError::blocked(format!(
            "patch approve 실패\n- 이유: 대상 파일 쓰기에 실패했습니다.\n- path: {}\n- error: {}\n- rollback status: {}",
            target.relative_path, err.message, rollback.status
        )));
    }

    let applied = fs::read_to_string(&target.absolute_path).map_err(|err| {
        let rollback = restore_bytes(
            &target.absolute_path,
            current.as_bytes(),
            &record.proposed_sha256,
            &record.original_sha256,
            &source_transaction,
        );
        AppError::blocked(format!(
            "patch approve 실패\n- 이유: 적용 후 대상 파일을 읽지 못했습니다.\n- path: {}\n- error: {}\n- rollback status: {}",
            target.relative_path, err, rollback.status
        ))
    })?;
    let applied_sha256 = sha256_text(&applied);
    if applied_sha256 != record.proposed_sha256 {
        let rollback = restore_bytes(
            &target.absolute_path,
            current.as_bytes(),
            &record.proposed_sha256,
            &record.original_sha256,
            &source_transaction,
        );
        return Err(AppError::blocked(format!(
            "patch approve 실패\n- 이유: 적용 후 SHA-256이 proposal과 일치하지 않습니다.\n- path: {}\n- expected proposed sha256: {}\n- applied sha256: {}\n- rollback status: {}",
            target.relative_path, record.proposed_sha256, applied_sha256, rollback.status
        )));
    }

    Ok(ApplyResult {
        relative_path: target.relative_path,
        original_sha256: record.original_sha256.clone(),
        applied_sha256,
        rollback_path,
    })
}

fn build_verification_plan(command: &str) -> Result<VerificationPlan, AppError> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Err(AppError::usage(
            "patch approve verification command는 비어 있을 수 없습니다.",
        ));
    }
    let parsed = policy::parse_patch_verification(trimmed)?;
    Ok(VerificationPlan {
        command: parsed.display,
        argv: parsed.argv,
    })
}

fn run_verification(plan: &VerificationPlan) -> VerificationResult {
    let project_root =
        fs::canonicalize(paths::project_root()).unwrap_or_else(|_| paths::project_root());
    let output = ProcessCommand::new(&plan.argv[0])
        .args(&plan.argv[1..])
        .current_dir(project_root)
        .output();

    match output {
        Ok(output) => VerificationResult {
            command: plan.command.clone(),
            exit_code: output
                .status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "terminated-by-signal".to_string()),
            stdout: output_excerpt(&output.stdout),
            stderr: output_excerpt(&output.stderr),
        },
        Err(err) => VerificationResult {
            command: plan.command.clone(),
            exit_code: "spawn-error".to_string(),
            stdout: "(empty)".to_string(),
            stderr: output_text_excerpt(&err.to_string()),
        },
    }
}

fn format_verification_result(result: &VerificationResult) -> String {
    format!(
        "- verification command: {}\n- verification exit code: {}\n- verification stdout: {}\n- verification stderr: {}\n",
        ledger::redact_text(&result.command),
        result.exit_code,
        result.stdout,
        result.stderr
    )
}

fn restore_from_rollback(record: &ProposalRecord, rollback_path: &Path) -> RollbackResult {
    let target = match resolve_target_for("patch rollback", &record.relative_path) {
        Ok(target) => target,
        Err(err) => {
            return RollbackResult {
                restored: false,
                status: format!("restore-failed: {}", err.message),
            }
        }
    };
    let current = match fs::read(&target.absolute_path) {
        Ok(bytes) => bytes,
        Err(err) => {
            return RollbackResult {
                restored: false,
                status: format!("restore-conflict: target reread failed: {err}"),
            }
        }
    };
    let current_hash = sha256_bytes(&current);
    if current_hash == record.original_sha256 {
        return RollbackResult {
            restored: true,
            status: format!(
                "already-restored-and-verified sha256={}",
                record.original_sha256
            ),
        };
    }
    if current_hash != record.proposed_sha256 {
        return RollbackResult {
            restored: false,
            status: format!("restore-conflict: target changed concurrently current={current_hash}"),
        };
    }
    let original = match fs::read(rollback_path) {
        Ok(contents) => contents,
        Err(err) => {
            return RollbackResult {
                restored: false,
                status: format!("restore-failed: rollback record read error: {err}"),
            }
        }
    };
    if sha256_bytes(&original) != record.original_sha256 {
        return RollbackResult {
            restored: false,
            status: "restore-failed: rollback record hash mismatch".to_string(),
        };
    }
    let source_transaction = record
        .proposal_path
        .with_file_name(format!("{}.source.txn", record.proposal_id));
    restore_bytes(
        &target.absolute_path,
        &original,
        &record.proposed_sha256,
        &record.original_sha256,
        &source_transaction,
    )
}

struct ApprovalLock {
    _lease: crate::lease::RecoverableLease,
}

impl ApprovalLock {
    fn acquire(proposal_id: &str) -> Result<Self, AppError> {
        let path = paths::project_patch_proposals_dir().join(format!("{proposal_id}.approve.lock"));
        crate::lease::RecoverableLease::acquire(path, "patch approve")
            .map(|lease| Self { _lease: lease })
    }
}

fn approval_prelock_test_barrier() -> Result<(), AppError> {
    if !cfg!(debug_assertions) {
        return Ok(());
    }
    let Ok(base) = std::env::var("RPOTATO_TEST_APPROVAL_PRELOCK_BARRIER") else {
        return Ok(());
    };
    let ready = PathBuf::from(format!("{base}.ready"));
    let release = PathBuf::from(format!("{base}.release"));
    fs::write(&ready, b"ready")
        .map_err(|err| AppError::runtime(format!("approval test barrier 생성 실패: {err}")))?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !release.exists() {
        if std::time::Instant::now() >= deadline {
            return Err(AppError::runtime("approval test barrier timeout"));
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    Ok(())
}

fn load_workflow_under_approval_lock(
    workflow_id: &str,
) -> Result<(state::WorkflowRecord, Option<ApprovalLock>), AppError> {
    let discovered = state::load_workflow(workflow_id)?;
    if discovered.proposal_id.is_empty() {
        return Ok((discovered, None));
    }
    let lock = ApprovalLock::acquire(&discovered.proposal_id)?;
    let current = state::load_workflow(workflow_id)?;
    if current.proposal_id != discovered.proposal_id {
        return Err(AppError::blocked(
            "workflow 작업 차단\n- 이유: approval lease 획득 중 proposal binding이 변경되었습니다.",
        ));
    }
    Ok((current, Some(lock)))
}

fn restore_bytes(
    target: &Path,
    contents: &[u8],
    expected_current_hash: &str,
    expected_hash: &str,
    transaction_path: &Path,
) -> RollbackResult {
    if fs::read(target)
        .ok()
        .is_some_and(|bytes| sha256_bytes(&bytes) == expected_hash)
    {
        return RollbackResult {
            restored: true,
            status: format!("already-restored-and-verified sha256={expected_hash}"),
        };
    }
    if cfg!(debug_assertions)
        && std::env::var("RPOTATO_TEST_ROLLBACK_FAULT").as_deref() == Ok("replace-failure")
    {
        return RollbackResult {
            restored: false,
            status: "restore-failed: injected rollback replace failure".to_string(),
        };
    }
    if let Err(err) = state::guarded_source_replace(
        target,
        expected_current_hash,
        contents,
        expected_hash,
        transaction_path,
    ) {
        return RollbackResult {
            restored: false,
            status: format!("restore-failed: {}", err.message),
        };
    }
    match fs::read(target) {
        Ok(actual) if sha256_bytes(&actual) == expected_hash => RollbackResult {
            restored: true,
            status: format!("restored-and-verified sha256={expected_hash}"),
        },
        Ok(actual) => RollbackResult {
            restored: false,
            status: format!(
                "restore-failed: restored hash mismatch actual={}",
                sha256_bytes(&actual)
            ),
        },
        Err(err) => RollbackResult {
            restored: false,
            status: format!("restore-failed: restored bytes reread error: {err}"),
        },
    }
}

fn current_source_hash(relative_path: &str) -> Result<String, AppError> {
    let target = resolve_target_for("patch source hash", relative_path)?;
    fs::read(&target.absolute_path)
        .map(|bytes| sha256_bytes(&bytes))
        .map_err(|err| AppError::blocked(format!("source hash reread 실패: {err}")))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn build_preview(
    path: &str,
    find: &str,
    replace: &str,
    workflow_id: &str,
    action_id: &str,
    verification_command: &str,
) -> Result<PatchPreview, AppError> {
    if find.is_empty() {
        return Err(AppError::usage(
            "patch preview의 --find 값은 비어 있을 수 없습니다.",
        ));
    }
    let target = resolve_target(path)?;
    let read_decision = policy::classify_path(PathMode::Read, &target.relative_path)?;
    if read_decision.decision != Decision::Allow {
        return Err(AppError::blocked(format!(
            "patch preview 차단\n- 이유: target read policy가 allow가 아닙니다.\n- path: {}\n- decision: {}",
            target.relative_path,
            read_decision_label(read_decision.decision)
        )));
    }
    let write_decision = policy::classify_path(PathMode::Write, &target.relative_path)?;
    if write_decision.decision == Decision::Deny {
        return Err(AppError::blocked(format!(
            "patch preview 차단\n- 이유: target write policy가 deny입니다.\n- path: {}",
            target.relative_path
        )));
    }
    let metadata = fs::metadata(&target.absolute_path).map_err(|err| {
        AppError::runtime(format!(
            "patch preview 대상 파일 metadata를 읽지 못했습니다: {} ({err})",
            target.relative_path
        ))
    })?;
    if !metadata.is_file() {
        return Err(AppError::usage(format!(
            "patch preview 대상은 file이어야 합니다: {}",
            target.relative_path
        )));
    }
    if metadata.len() > MAX_PATCH_FILE_BYTES {
        return Err(AppError::blocked(format!(
            "patch preview 차단\n- 이유: 대상 파일이 preview 한도를 초과했습니다.\n- path: {}\n- size bytes: {}\n- max bytes: {}",
            target.relative_path,
            metadata.len(),
            MAX_PATCH_FILE_BYTES
        )));
    }
    let original = fs::read_to_string(&target.absolute_path).map_err(|err| {
        AppError::runtime(format!(
            "patch preview 대상 파일을 UTF-8 text로 읽지 못했습니다: {} ({err})",
            target.relative_path
        ))
    })?;
    let matches = original.matches(find).count();
    if matches == 0 {
        return Err(AppError::blocked(format!(
            "patch preview 차단\n- 이유: --find text를 대상 파일에서 찾지 못했습니다.\n- path: {}",
            target.relative_path
        )));
    }
    if matches > 1 {
        return Err(AppError::blocked(format!(
            "patch preview 차단\n- 이유: --find text가 여러 번 나타나 patch target이 모호합니다.\n- path: {}\n- matches: {}",
            target.relative_path, matches
        )));
    }
    let proposed = original.replacen(find, replace, 1);
    if proposed == original {
        return Err(AppError::blocked(format!(
            "patch preview 차단\n- 이유: proposed content가 original과 동일합니다.\n- path: {}",
            target.relative_path
        )));
    }

    let original_sha256 = sha256_text(&original);
    let proposed_sha256 = sha256_text(&proposed);
    let diff = render_unified_diff(&target.relative_path, &original, &proposed);
    let content_id = &sha256_text(&format!(
        "{}\n{}\n{}",
        target.relative_path, original_sha256, proposed_sha256
    ))[..16];
    let proposal_id = if workflow_id.is_empty() {
        format!("patch-proposal-standalone-{content_id}")
    } else {
        format!(
            "patch-proposal-wf-{}-act-{}-{content_id}",
            safe_id_tail(workflow_id),
            safe_id_tail(action_id)
        )
    };
    let approval_token = if workflow_id.is_empty() {
        String::new()
    } else {
        issue_approval_token()?
    };
    let proposal_path = paths::project_patch_proposals_dir().join(format!("{proposal_id}.txt"));

    Ok(PatchPreview {
        proposal_id,
        approval_token,
        relative_path: target.relative_path,
        original_sha256,
        proposed_sha256,
        replacements: matches,
        diff,
        proposal_path,
        proposed_content: proposed,
        workflow_id: workflow_id.to_string(),
        action_id: action_id.to_string(),
        verification_command: verification_command.to_string(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TargetPath {
    absolute_path: PathBuf,
    relative_path: String,
}

fn resolve_target(raw_path: &str) -> Result<TargetPath, AppError> {
    resolve_target_for("patch preview", raw_path)
}

fn resolve_target_for(operation: &str, raw_path: &str) -> Result<TargetPath, AppError> {
    if raw_path.trim().is_empty() {
        return Err(AppError::usage(format!(
            "{operation}는 비어 있지 않은 --path 값이 필요합니다.",
        )));
    }
    let project_root = fs::canonicalize(paths::project_root()).map_err(|err| {
        AppError::runtime(format!(
            "project root를 해석하지 못했습니다: {} ({err})",
            paths::project_root().display()
        ))
    })?;
    let raw = Path::new(raw_path);
    let candidate = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        project_root.join(raw)
    };
    let absolute_path = fs::canonicalize(&candidate).map_err(|err| {
        AppError::runtime(format!(
            "{operation} 대상 path를 해석하지 못했습니다: {} ({err})",
            candidate.display()
        ))
    })?;
    let relative_path = absolute_path
        .strip_prefix(&project_root)
        .map_err(|_| {
            AppError::blocked(format!(
                "{operation} 차단\n- 이유: project boundary 밖 path입니다.\n- path: {}",
                raw_path
            ))
        })?
        .to_string_lossy()
        .replace('\\', "/");

    Ok(TargetPath {
        absolute_path,
        relative_path,
    })
}

fn write_proposal_record(preview: &PatchPreview) -> Result<(), AppError> {
    if let Some(parent) = preview.proposal_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            AppError::runtime(format!(
                "patch proposal directory를 만들지 못했습니다: {} ({err})",
                parent.display()
            ))
        })?;
    }
    if preview.proposal_path.exists() {
        return Err(AppError::blocked(format!("patch proposal 저장 차단\n- 이유: immutable proposal artifact가 이미 존재합니다.\n- path: {}", preview.proposal_path.display())));
    }
    let body = format!(
            "record_version=4\nproposal_id={}\nworkflow_id={}\naction_id={}\npath={}\napproval_token_hash={}\noriginal_sha256={}\nproposed_sha256={}\nverification_command_hex={}\nreplacements={}\ncontent_encoding=utf8-hex\nproposed_content_hex={}\n\n{}\n",
            preview.proposal_id,
            preview.workflow_id,
            preview.action_id,
            preview.relative_path,
            sha256_text(&preview.approval_token),
            preview.original_sha256,
            preview.proposed_sha256,
            encode_hex_text(&preview.verification_command),
            preview.replacements,
            encode_hex_text(&preview.proposed_content),
            preview.diff
        );
    state::atomic_replace_bytes(&preview.proposal_path, body.as_bytes())
}

fn safe_id_tail(value: &str) -> &str {
    value.rsplit('-').next().unwrap_or(value)
}

fn render_unified_diff(path: &str, original: &str, proposed: &str) -> String {
    let old_lines = original.split('\n').collect::<Vec<_>>();
    let new_lines = proposed.split('\n').collect::<Vec<_>>();
    let mut prefix = 0usize;
    while prefix < old_lines.len()
        && prefix < new_lines.len()
        && old_lines[prefix] == new_lines[prefix]
    {
        prefix += 1;
    }

    let mut suffix = 0usize;
    while suffix + prefix < old_lines.len()
        && suffix + prefix < new_lines.len()
        && old_lines[old_lines.len() - 1 - suffix] == new_lines[new_lines.len() - 1 - suffix]
    {
        suffix += 1;
    }

    let context_before = prefix.saturating_sub(3);
    let context_after_old = (old_lines.len() - suffix + 3).min(old_lines.len());
    let context_after_new = (new_lines.len() - suffix + 3).min(new_lines.len());
    let old_start = context_before + 1;
    let new_start = context_before + 1;
    let old_count = context_after_old.saturating_sub(context_before).max(1);
    let new_count = context_after_new.saturating_sub(context_before).max(1);

    let mut diff = format!(
        "--- a/{path}\n+++ b/{path}\n@@ -{},{} +{},{} @@\n",
        old_start, old_count, new_start, new_count
    );
    for line in &old_lines[context_before..prefix] {
        diff.push_str(&format!(" {line}\n"));
    }
    for line in &old_lines[prefix..old_lines.len() - suffix] {
        diff.push_str(&format!("-{line}\n"));
    }
    for line in &new_lines[prefix..new_lines.len() - suffix] {
        diff.push_str(&format!("+{line}\n"));
    }
    for line in &old_lines[old_lines.len() - suffix..context_after_old] {
        diff.push_str(&format!(" {line}\n"));
    }
    diff
}

fn issue_approval_token() -> Result<String, AppError> {
    let mut bytes = [0_u8; 32];
    fill_os_random(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[cfg(unix)]
fn fill_os_random(bytes: &mut [u8]) -> Result<(), AppError> {
    File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(bytes))
        .map_err(|err| AppError::runtime(format!("OS CSPRNG nonce 발급 실패: {err}")))
}

#[cfg(windows)]
fn fill_os_random(bytes: &mut [u8]) -> Result<(), AppError> {
    type NtStatus = i32;
    #[link(name = "bcrypt")]
    extern "system" {
        fn BCryptGenRandom(
            algorithm: *mut std::ffi::c_void,
            buffer: *mut u8,
            length: u32,
            flags: u32,
        ) -> NtStatus;
    }
    const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 0x00000002;
    // SAFETY: the OS writes exactly `bytes.len()` bytes to the live mutable buffer.
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            bytes.as_mut_ptr(),
            bytes.len() as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status < 0 {
        Err(AppError::runtime(format!(
            "OS CSPRNG nonce 발급 실패: NTSTATUS {status:#x}"
        )))
    } else {
        Ok(())
    }
}

fn display_none(value: &str) -> &str {
    if value.is_empty() {
        "none"
    } else {
        value
    }
}

fn encode_hex_text(value: &str) -> String {
    let mut output = String::with_capacity(value.len() * 2);
    for byte in value.as_bytes() {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn decode_hex_text(value: &str) -> Result<String, String> {
    if !value.len().is_multiple_of(2) {
        return Err("hex length must be even".to_string());
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    let chars = value.as_bytes();
    let mut index = 0usize;
    while index < chars.len() {
        let high = hex_value(chars[index]).ok_or_else(|| "invalid high nibble".to_string())?;
        let low = hex_value(chars[index + 1]).ok_or_else(|| "invalid low nibble".to_string())?;
        bytes.push((high << 4) | low);
        index += 2;
    }
    String::from_utf8(bytes).map_err(|err| err.to_string())
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn output_excerpt(bytes: &[u8]) -> String {
    output_text_excerpt(&String::from_utf8_lossy(bytes))
}

fn output_text_excerpt(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "(empty)".to_string();
    }
    let mut output = trimmed
        .chars()
        .take(MAX_VERIFICATION_OUTPUT_CHARS)
        .collect::<String>()
        .replace('\n', "\\n");
    if trimmed.chars().count() > MAX_VERIFICATION_OUTPUT_CHARS {
        output.push_str("...");
    }
    output
}

fn validate_proposal_id(proposal_id: &str) -> Result<(), AppError> {
    if proposal_id.starts_with("patch-proposal-")
        && proposal_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
    {
        return Ok(());
    }

    Err(AppError::usage(
        "patch approve proposal id 형식이 올바르지 않습니다.",
    ))
}

fn sha256_text(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let bytes = hasher.finalize();
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn read_decision_label(decision: Decision) -> &'static str {
    match decision {
        Decision::Allow => "allow",
        Decision::Ask => "ask",
        Decision::Deny => "deny",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_creates_diff_record_without_modifying_target() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!("rpotato-patch-test-{}", std::process::id()));
        let project_root = root.join("project");
        fs::create_dir_all(project_root.join("src")).unwrap();
        let target = project_root.join("src/lib.rs");
        fs::write(&target, "fn answer() -> i32 {\n    1\n}\n").unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let report = preview_report("src/lib.rs", "    1", "    2").unwrap();
        let contents = fs::read_to_string(&target).unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");

        assert_eq!(contents, "fn answer() -> i32 {\n    1\n}\n");
        assert!(report.contains("status: diff-only"));
        assert!(report.contains("-    1"));
        assert!(report.contains("+    2"));
        assert!(report.contains("standalone preview는 diff 표시 전용"));
    }

    #[test]
    fn approve_accepts_recorded_token_in_dry_run() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root =
            std::env::temp_dir().join(format!("rpotato-patch-approve-test-{}", std::process::id()));
        let (_target, _workflow, proposal) = create_pending_workflow(&root, "pwd");
        let approval =
            approve_report(&proposal.proposal_id, &proposal.approval_token, true, None).unwrap();
        clear_patch_test_env(&root);

        assert!(approval.contains("status: gate-passed"));
        assert!(approval.contains("boundary: approval gate만 확인했습니다"));
    }

    #[test]
    fn approve_applies_recorded_patch() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root =
            std::env::temp_dir().join(format!("rpotato-patch-apply-test-{}", std::process::id()));
        let (target, _workflow, proposal) = create_pending_workflow(&root, "pwd");
        let approval =
            approve_report(&proposal.proposal_id, &proposal.approval_token, false, None).unwrap();
        let contents = fs::read_to_string(&target).unwrap();
        let rollback_path = root
            .join("project")
            .join(".rpotato")
            .join("patch-proposals")
            .join(format!("{}.rollback", proposal.proposal_id));
        let rollback_exists = rollback_path.exists();
        clear_patch_test_env(&root);

        assert_eq!(contents, "pub const X: i32 = 2;\n");
        assert!(rollback_exists);
        assert!(approval.contains("status: applied-awaiting-verification"));
        assert!(approval.contains("verification command는 아직 실행하지 않았습니다"));
        assert!(!approval.contains("stop gate: 통과"));
    }

    #[test]
    fn approve_blocks_changed_target_before_apply() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-patch-changed-target-test-{}",
            std::process::id()
        ));
        let (target, _workflow, proposal) = create_pending_workflow(&root, "pwd");
        fs::write(&target, "pub const X: i32 = 3;\n").unwrap();
        let err = approve_report(&proposal.proposal_id, &proposal.approval_token, false, None)
            .unwrap_err();
        let contents = fs::read_to_string(&target).unwrap();
        clear_patch_test_env(&root);

        assert_eq!(err.code, 3);
        assert!(err.message.contains("preview 이후 변경"));
        assert_eq!(contents, "pub const X: i32 = 3;\n");
    }

    #[test]
    fn approve_rejects_inline_verification_command_before_apply() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-patch-verify-block-test-{}",
            std::process::id()
        ));
        let (target, _workflow, proposal) = create_pending_workflow(&root, "pwd");
        let err = approve_report(
            &proposal.proposal_id,
            &proposal.approval_token,
            false,
            Some("echo hi"),
        )
        .unwrap_err();
        let contents = fs::read_to_string(&target).unwrap();
        clear_patch_test_env(&root);

        assert_eq!(err.code, 3);
        assert!(err
            .message
            .contains("verification command 승인은 분리되어 있습니다"));
        assert_eq!(contents, "pub const X: i32 = 1;\n");
    }

    #[cfg(unix)]
    #[test]
    fn verification_runs_only_after_separate_approval() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-patch-verify-run-test-{}",
            std::process::id()
        ));
        let (target, _workflow, proposal) = create_pending_workflow(&root, "pwd");
        let approval =
            approve_report(&proposal.proposal_id, &proposal.approval_token, false, None).unwrap();
        let pending = state::load_workflow(&_workflow.workflow_id).unwrap();
        let verify_token = verification_token(&approval);
        let apply_token_rejected =
            verify_report(&proposal.proposal_id, &proposal.approval_token).unwrap_err();
        let verify_token_rejected =
            approve_report(&proposal.proposal_id, &verify_token, false, None).unwrap_err();
        let verified = verify_report(&proposal.proposal_id, &verify_token).unwrap();
        let contents = fs::read_to_string(&target).unwrap();
        clear_patch_test_env(&root);

        assert_eq!(contents, "pub const X: i32 = 2;\n");
        assert_eq!(pending.phase, "pending-verification-approval");
        assert!(pending.evidence_id.is_empty());
        assert_eq!(apply_token_rejected.code, 3);
        assert_eq!(verify_token_rejected.code, 3);
        assert!(verified.contains("검증: 통과"));
        assert!(
            crate::korean_guard::validate(&verified),
            "guard rejected report: {verified}"
        );
    }

    #[test]
    fn proposal_summary_and_detail_read_preview_record() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-patch-tui-read-test-{}",
            std::process::id()
        ));
        let project_root = root.join("project");
        fs::create_dir_all(project_root.join("src")).unwrap();
        fs::write(project_root.join("src/lib.rs"), "pub const X: i32 = 1;\n").unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let report = preview_report("src/lib.rs", "1", "2").unwrap();
        let proposal_id = report_value(&report, "proposal id").unwrap();
        let summaries = proposal_summaries(5).unwrap();
        let detail = proposal_detail(&proposal_id).unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].proposal_id, proposal_id);
        assert_eq!(summaries[0].status, "pending-approval");
        assert_eq!(detail.summary.relative_path, "src/lib.rs");
        assert!(detail.diff.contains("-pub const X: i32 = 1;"));
        assert!(detail.diff.contains("+pub const X: i32 = 2;"));
    }

    #[test]
    fn approval_nonce_is_random_hash_only_and_not_reconstructable() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root =
            std::env::temp_dir().join(format!("rpotato-patch-random-token-{}", std::process::id()));
        let project_root = root.join("project");
        fs::create_dir_all(project_root.join("src")).unwrap();
        fs::write(project_root.join("src/lib.rs"), "pub const X: i32 = 1;\n").unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let first_token = issue_approval_token().unwrap();
        let (_target, _workflow, proposal) = create_pending_workflow(&root, "pwd");
        let second_token = proposal.approval_token.clone();
        let proposal_id = proposal.proposal_id;
        let record = fs::read_to_string(
            paths::project_patch_proposals_dir().join(format!("{proposal_id}.txt")),
        )
        .unwrap();
        let detail = proposal_detail(&proposal_id).unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);

        assert_eq!(first_token.len(), 64);
        assert_eq!(second_token.len(), 64);
        assert_ne!(first_token, second_token);
        assert!(!record.contains(&first_token));
        assert!(!record.contains(&second_token));
        assert!(record.contains(&format!(
            "approval_token_hash={}",
            sha256_text(&second_token)
        )));
        assert!(detail.diff.contains("pub const X"));
    }

    #[test]
    fn bad_token_does_not_consume_valid_nonce() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-patch-bad-then-good-{}",
            std::process::id()
        ));
        let (target, _workflow, proposal) = create_pending_workflow(&root, "pwd");
        let rejected =
            approve_report(&proposal.proposal_id, "wrong-token", false, None).unwrap_err();
        let accepted =
            approve_report(&proposal.proposal_id, &proposal.approval_token, false, None).unwrap();
        let contents = fs::read_to_string(&target).unwrap();
        clear_patch_test_env(&root);

        assert_eq!(rejected.code, 3);
        assert!(accepted.contains("status: applied-awaiting-verification"));
        assert_eq!(contents, "pub const X: i32 = 2;\n");
    }

    #[test]
    fn standalone_preview_never_overwrites_existing_artifact() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("standalone-overwrite");
        set_patch_test_env(&root);
        let target = root.join("project/src/lib.rs");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, "pub const X: i32 = 1;\n").unwrap();

        preview_report("src/lib.rs", "1", "2").unwrap();
        let error = preview_report("src/lib.rs", "1", "2").unwrap_err();
        let source = fs::read_to_string(&target).unwrap();

        clear_patch_test_env(&root);
        assert_eq!(error.code, 3);
        assert!(error.message.contains("이미 존재"));
        assert_eq!(source, "pub const X: i32 = 1;\n");
    }

    #[test]
    fn token_rotate_checkpoints_new_hash_and_invalidates_old_token() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("token-rotate");
        let (_target, workflow, proposal) = create_pending_workflow(&root, "pwd");
        let old_token = proposal.approval_token.clone();

        let report = rotate_workflow_token_report(&proposal.proposal_id).unwrap();
        let new_token = report_value(&report, "새 approval token").unwrap();
        let rotated = state::load_workflow(&workflow.workflow_id).unwrap();
        let old_error = approve_report(&proposal.proposal_id, &old_token, true, None).unwrap_err();
        let accepted = approve_report(&proposal.proposal_id, &new_token, true, None).unwrap();

        clear_patch_test_env(&root);
        assert_eq!(rotated.approval_state, "pending-rotated");
        assert_eq!(rotated.approval_credential_hash, sha256_text(&new_token));
        assert_ne!(rotated.approval_credential_hash, sha256_text(&old_token));
        assert_eq!(old_error.code, 3);
        assert!(accepted.contains("gate-passed"));
    }

    #[test]
    fn verification_token_rotate_invalidates_old_token() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("verification-token-rotate");
        let (_target, workflow, proposal) = create_pending_workflow(&root, "pwd");
        let approval =
            approve_report(&proposal.proposal_id, &proposal.approval_token, false, None).unwrap();
        let old_token = verification_token(&approval);

        let rotation = rotate_workflow_token_report(&proposal.proposal_id).unwrap();
        let new_token = report_value(&rotation, "새 approval token").unwrap();
        let rotated = state::load_workflow(&workflow.workflow_id).unwrap();
        let old_error = verify_report(&proposal.proposal_id, &old_token).unwrap_err();
        let verified = verify_report(&proposal.proposal_id, &new_token).unwrap();

        clear_patch_test_env(&root);
        assert_eq!(rotated.verification_approval_state, "pending-rotated");
        assert_eq!(
            rotated.verification_credential_hash,
            sha256_text(&new_token)
        );
        assert_ne!(
            rotated.verification_credential_hash,
            sha256_text(&old_token)
        );
        assert_eq!(old_error.code, 3);
        assert!(verified.contains("검증: 통과"));
    }

    #[test]
    fn proposal_and_canonical_token_tamper_fail_closed() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        for mode in ["proposal", "token"] {
            let root = patch_test_root(&format!("tamper-{mode}"));
            let (target, mut workflow, proposal) = create_pending_workflow(&root, "pwd");
            if mode == "proposal" {
                let path = paths::project_patch_proposals_dir()
                    .join(format!("{}.txt", proposal.proposal_id));
                let mut body = fs::read_to_string(&path).unwrap();
                body.push_str("tampered trailing bytes\n");
                fs::write(path, body).unwrap();
            } else {
                workflow.approval_credential_hash = "0".repeat(64);
                workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();
                assert_eq!(workflow.approval_credential_hash, "0".repeat(64));
            }

            let error =
                approve_report(&proposal.proposal_id, &proposal.approval_token, false, None)
                    .unwrap_err();
            let source = fs::read_to_string(&target).unwrap();
            clear_patch_test_env(&root);
            assert_eq!(error.code, 3);
            assert_eq!(source, "pub const X: i32 = 1;\n");
        }
    }

    #[test]
    fn legacy_v2_plaintext_proposal_requires_safe_repreview_migration() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("legacy-v2");
        let (target, _workflow, proposal) = create_pending_workflow(&root, "pwd");
        let proposal_id = proposal.proposal_id;
        let token = proposal.approval_token;
        let path = paths::project_patch_proposals_dir().join(format!("{proposal_id}.txt"));
        let body = fs::read_to_string(&path)
            .unwrap()
            .replacen("record_version=4", "record_version=2", 1)
            .replacen(
                &format!("approval_token_hash={}", sha256_text(&token)),
                &format!("approval_token={token}"),
                1,
            );
        fs::write(&path, body).unwrap();

        let error = approve_report(&proposal_id, &token, false, None).unwrap_err();
        let source = fs::read_to_string(&target).unwrap();
        let scrubbed = fs::read_to_string(&path).unwrap();
        clear_patch_test_env(&root);
        assert_eq!(error.code, 3);
        assert!(error.message.contains("hash-only로 atomic scrub"));
        assert!(!scrubbed.contains(&format!("approval_token={token}")));
        assert_eq!(source, "pub const X: i32 = 1;\n");
    }

    #[test]
    fn proposal_loader_rejects_duplicate_unknown_and_mixed_credential_fields() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("proposal-strict");
        set_patch_test_env(&root);
        let target = root.join("project/src/lib.rs");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, "pub const X: i32 = 1;\n").unwrap();
        let report = preview_report("src/lib.rs", "1", "2").unwrap();
        let proposal_id = report_value(&report, "proposal id").unwrap();
        let path = paths::project_patch_proposals_dir().join(format!("{proposal_id}.txt"));
        let original = fs::read_to_string(&path).unwrap();
        for malformed in [
            original.replacen("record_version=4", "record_version=4\nrecord_version=4", 1),
            original.replacen("path=", "unknown_key=x\npath=", 1),
            original.replacen(
                "approval_token_hash=",
                "approval_token=legacy\napproval_token_hash=",
                1,
            ),
        ] {
            fs::write(&path, malformed).unwrap();
            assert!(load_proposal_record(&proposal_id, &path).is_err());
        }
        clear_patch_test_env(&root);
    }

    #[test]
    fn verification_started_crash_never_auto_reruns_command() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("verification-started");
        let (target, workflow, proposal) = create_pending_workflow(&root, "pwd");
        let approval =
            approve_report(&proposal.proposal_id, &proposal.approval_token, false, None).unwrap();
        let verify_token = verification_token(&approval);
        std::env::set_var(
            "RPOTATO_TEST_VERIFICATION_FAULT",
            "after-started-checkpoint",
        );
        let injected = verify_report(&proposal.proposal_id, &verify_token).unwrap_err();
        std::env::remove_var("RPOTATO_TEST_VERIFICATION_FAULT");
        let started = state::load_workflow(&workflow.workflow_id).unwrap();
        let resume = resume_workflow_report(&workflow.workflow_id).unwrap_err();
        let source = fs::read_to_string(&target).unwrap();

        clear_patch_test_env(&root);
        assert_eq!(injected.code, 1);
        assert_eq!(started.phase, "verification-started");
        assert_eq!(source, "pub const X: i32 = 2;\n");
        assert!(resume.message.contains("자동 재실행하지 않습니다"));
    }

    #[test]
    fn verification_started_can_be_explicitly_cancelled_and_restores_source() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("verification-cancel");
        let (target, workflow, proposal) = create_pending_workflow(&root, "pwd");
        let approval =
            approve_report(&proposal.proposal_id, &proposal.approval_token, false, None).unwrap();
        let verify_token = verification_token(&approval);
        std::env::set_var(
            "RPOTATO_TEST_VERIFICATION_FAULT",
            "after-started-checkpoint",
        );
        verify_report(&proposal.proposal_id, &verify_token).unwrap_err();
        std::env::remove_var("RPOTATO_TEST_VERIFICATION_FAULT");

        let report = cancel_workflow_report(&workflow.workflow_id).unwrap();
        let cancelled = state::load_workflow(&workflow.workflow_id).unwrap();
        let source = fs::read_to_string(&target).unwrap();
        clear_patch_test_env(&root);
        assert!(report.contains("workflow 취소 완료"));
        assert_eq!(cancelled.phase, "cancelled");
        assert_eq!(source, "pub const X: i32 = 1;\n");
    }

    #[test]
    fn approved_checkpoint_can_be_cancelled_before_apply_without_rollback_record() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("approved-before-apply-cancel");
        let (target, mut workflow, _proposal) = create_pending_workflow(&root, "pwd");
        workflow.phase = "approved".to_string();
        workflow.approval_state = "approved".to_string();
        workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();

        let report = cancel_workflow_report(&workflow.workflow_id).unwrap();
        let cancelled = state::load_workflow(&workflow.workflow_id).unwrap();
        let source = fs::read_to_string(&target).unwrap();
        clear_patch_test_env(&root);

        assert!(report.contains("workflow 취소 완료"));
        assert_eq!(cancelled.phase, "cancelled");
        assert_eq!(source, "pub const X: i32 = 1;\n");
    }

    #[test]
    fn approve_reloads_cancelled_workflow_after_prelock_race() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("approve-cancel-race");
        let (target, mut workflow, proposal) = create_pending_workflow(&root, "pwd");
        workflow.phase = "approved".to_string();
        workflow.approval_state = "approved".to_string();
        workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();
        let barrier = root.join("approve-prelock");
        std::env::set_var("RPOTATO_TEST_APPROVAL_PRELOCK_BARRIER", &barrier);
        let proposal_id = proposal.proposal_id.clone();
        let token = proposal.approval_token.clone();
        let approve = std::thread::spawn(move || {
            approve_report(&proposal_id, &token, false, None).unwrap_err()
        });
        let ready = PathBuf::from(format!("{}.ready", barrier.display()));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !ready.exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "approve prelock barrier timeout"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        let cancelled_report = cancel_workflow_report(&workflow.workflow_id).unwrap();
        fs::write(
            PathBuf::from(format!("{}.release", barrier.display())),
            b"release",
        )
        .unwrap();
        let approve_error = approve.join().unwrap();
        std::env::remove_var("RPOTATO_TEST_APPROVAL_PRELOCK_BARRIER");
        let cancelled = state::load_workflow(&workflow.workflow_id).unwrap();
        let source = fs::read_to_string(&target).unwrap();
        clear_patch_test_env(&root);

        assert!(cancelled_report.contains("workflow 취소 완료"));
        assert_eq!(approve_error.code, 3);
        assert!(approve_error.message.contains("phase: cancelled"));
        assert_eq!(cancelled.phase, "cancelled");
        assert_eq!(source, "pub const X: i32 = 1;\n");
    }

    #[test]
    fn cancel_is_idempotent_after_source_was_already_restored() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("already-restored-cancel");
        let (target, workflow, proposal) = create_pending_workflow(&root, "pwd");
        let approval =
            approve_report(&proposal.proposal_id, &proposal.approval_token, false, None).unwrap();
        assert!(approval.contains("applied-awaiting-verification"));
        fs::write(&target, "pub const X: i32 = 1;\n").unwrap();
        let rollback_path =
            paths::project_patch_proposals_dir().join(format!("{}.rollback", proposal.proposal_id));
        fs::remove_file(rollback_path).unwrap();

        let report = cancel_workflow_report(&workflow.workflow_id).unwrap();
        let cancelled = state::load_workflow(&workflow.workflow_id).unwrap();
        let source = fs::read_to_string(&target).unwrap();
        clear_patch_test_env(&root);

        assert!(report.contains("workflow 취소 완료"));
        assert_eq!(cancelled.phase, "cancelled");
        assert_eq!(source, "pub const X: i32 = 1;\n");
    }

    #[test]
    fn source_replace_fault_windows_restore_original_bytes() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        for point in ["after-guard", "after-install"] {
            let root = patch_test_root(&format!("source-fault-{point}"));
            let (target, _workflow, proposal) = create_pending_workflow(&root, "pwd");
            std::env::set_var("RPOTATO_TEST_SOURCE_REPLACE_FAULT", point);
            let error =
                approve_report(&proposal.proposal_id, &proposal.approval_token, false, None)
                    .unwrap_err();
            std::env::remove_var("RPOTATO_TEST_SOURCE_REPLACE_FAULT");
            let source = fs::read_to_string(&target).unwrap();
            clear_patch_test_env(&root);
            assert!(matches!(error.code, 1 | 3), "point: {point}");
            assert_eq!(source, "pub const X: i32 = 1;\n", "point: {point}");
        }
    }

    #[test]
    fn incomplete_model_phases_resume_to_truthful_terminal_failure() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        for phase in ["model-pending", "action-recorded"] {
            let root = patch_test_root(&format!("resume-{phase}"));
            set_patch_test_env(&root);
            fs::create_dir_all(root.join("project")).unwrap();
            state::initialize().unwrap();
            let mut workflow = state::create_workflow("incomplete model phase").unwrap();
            if phase == "action-recorded" {
                workflow.phase = phase.to_string();
                workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();
            }

            let error = resume_workflow_report(&workflow.workflow_id).unwrap_err();
            let failed = state::load_workflow(&workflow.workflow_id).unwrap();
            clear_patch_test_env(&root);
            assert_eq!(error.code, 3);
            assert_eq!(failed.phase, "failed");
            assert_eq!(failed.failure_reason, format!("resume-incomplete-{phase}"));
            assert!(error
                .message
                .contains("backend 또는 command를 자동 재실행하지 않습니다"));
        }
    }

    #[test]
    fn approval_lock_excludes_concurrent_side_effect_owner() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("approval-lock");
        fs::create_dir_all(root.join("project")).unwrap();
        set_patch_test_env(&root);
        let first = ApprovalLock::acquire("patch-proposal-lock-test").unwrap();
        let second = match ApprovalLock::acquire("patch-proposal-lock-test") {
            Ok(_) => panic!("second lock unexpectedly succeeded"),
            Err(error) => error,
        };
        drop(first);
        let third = ApprovalLock::acquire("patch-proposal-lock-test").unwrap();
        drop(third);
        clear_patch_test_env(&root);
        assert!(second.message.contains("patch approve lock 차단"));
    }

    #[test]
    fn direct_approve_fails_closed_when_multiple_workflows_are_active() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("direct-multi-active");
        let (target, _workflow, proposal) = create_pending_workflow(&root, "pwd");
        state::create_workflow("second active").unwrap();

        let error = approve_report(&proposal.proposal_id, &proposal.approval_token, false, None)
            .unwrap_err();
        let source = fs::read_to_string(&target).unwrap();
        clear_patch_test_env(&root);
        assert_eq!(error.code, 3);
        assert!(error
            .message
            .contains("여러 non-terminal canonical workflow"));
        assert_eq!(source, "pub const X: i32 = 1;\n");
    }

    #[test]
    fn rollback_preserves_concurrent_user_edit() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("rollback-concurrent-edit");
        let (target, _workflow, proposal) = create_pending_workflow(&root, "pwd");
        let record = load_proposal_record(
            &proposal.proposal_id,
            &paths::project_patch_proposals_dir().join(format!("{}.txt", proposal.proposal_id)),
        )
        .unwrap();
        fs::write(&target, &record.proposed_content).unwrap();
        let rollback_path =
            paths::project_patch_proposals_dir().join(format!("{}.rollback", proposal.proposal_id));
        state::atomic_replace_bytes(&rollback_path, b"pub const X: i32 = 1;\n").unwrap();
        fs::write(&target, "pub const X: i32 = 99;\n").unwrap();

        let result = restore_from_rollback(&record, &rollback_path);
        let source = fs::read_to_string(&target).unwrap();
        clear_patch_test_env(&root);
        assert!(!result.restored);
        assert!(result.status.contains("restore-conflict"));
        assert_eq!(source, "pub const X: i32 = 99;\n");
    }

    #[test]
    fn rollback_tamper_and_replace_failure_are_reported_truthfully() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        for fault in ["tamper-record", "replace-failure"] {
            let root = std::env::temp_dir().join(format!(
                "rpotato-patch-rollback-{fault}-{}",
                std::process::id()
            ));
            let project_root = root.join("project");
            fs::create_dir_all(project_root.join("src")).unwrap();
            let target = project_root.join("src/lib.rs");
            fs::write(&target, "pub const X: i32 = 1;\n").unwrap();
            std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
            std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
            state::initialize().unwrap();

            let mut workflow = state::create_workflow("change X").unwrap();
            let proposal = prepare_workflow_proposal(
                &workflow.workflow_id,
                &workflow.action_id,
                "src/lib.rs",
                "1",
                "2",
                "cargo test",
            )
            .unwrap();
            workflow.source_path = proposal.relative_path.clone();
            workflow.source_hash = proposal.original_sha256.clone();
            workflow.before_hash = proposal.original_sha256.clone();
            workflow.after_hash = proposal.proposed_sha256.clone();
            workflow.proposal_id = proposal.proposal_id.clone();
            workflow.proposal_hash = proposal.proposal_hash.clone();
            workflow.approval_credential_hash = proposal.approval_credential_hash.clone();
            workflow.verification_plan = proposal.verification_command.clone();
            workflow.approval_state = "pending".to_string();
            workflow.phase = "pending-approval".to_string();
            workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();

            let approval =
                approve_report(&proposal.proposal_id, &proposal.approval_token, false, None)
                    .unwrap();
            let verify_token = verification_token(&approval);
            std::env::set_var("RPOTATO_TEST_ROLLBACK_FAULT", fault);
            let error = verify_report(&proposal.proposal_id, &verify_token).unwrap_err();
            std::env::remove_var("RPOTATO_TEST_ROLLBACK_FAULT");
            let failed = state::load_workflow(&workflow.workflow_id).unwrap();
            let source = fs::read_to_string(&target).unwrap();
            let evidence = fs::read_to_string(
                paths::project_evidence_dir().join(format!("{}.json", failed.evidence_id)),
            )
            .unwrap();

            std::env::remove_var("RPOTATO_PROJECT_ROOT");
            std::env::remove_var("RPOTATO_DATA_HOME");
            let _ = fs::remove_dir_all(root);

            assert_eq!(error.code, 3, "fault: {fault}");
            assert!(error.message.contains("rollback-failed"), "fault: {fault}");
            assert_eq!(failed.failure_reason, "verification-failed-rollback-failed");
            assert_eq!(source, "pub const X: i32 = 2;\n");
            assert!(evidence.contains(&format!("\"source_hash\": \"{}\"", sha256_text(&source))));
        }
    }

    #[test]
    fn preview_blocks_ambiguous_find_text() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root =
            std::env::temp_dir().join(format!("rpotato-patch-ambiguous-{}", std::process::id()));
        let project_root = root.join("project");
        fs::create_dir_all(&project_root).unwrap();
        fs::write(project_root.join("file.txt"), "same\nsame\n").unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let err = preview_report("file.txt", "same", "changed").unwrap_err();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");

        assert_eq!(err.code, 3);
        assert!(err.message.contains("여러 번"));
    }

    fn report_value(report: &str, key: &str) -> Option<String> {
        let prefix = format!("- {key}: ");
        report
            .lines()
            .find_map(|line| line.strip_prefix(&prefix).map(|value| value.to_string()))
    }

    fn verification_token(report: &str) -> String {
        report_value(report, "verification command approval")
            .and_then(|command| {
                command
                    .split_once(" --token ")
                    .map(|(_, token)| token.to_string())
            })
            .expect("verification approval token")
    }

    fn patch_test_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("rpotato-patch-{name}-{}", std::process::id()))
    }

    fn set_patch_test_env(root: &Path) {
        let _ = fs::remove_dir_all(root);
        std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    }

    fn clear_patch_test_env(root: &Path) {
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);
    }

    fn create_pending_workflow(
        root: &Path,
        verification: &str,
    ) -> (PathBuf, state::WorkflowRecord, WorkflowProposal) {
        set_patch_test_env(root);
        let target = root.join("project/src/lib.rs");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, "pub const X: i32 = 1;\n").unwrap();
        state::initialize().unwrap();
        let mut workflow = state::create_workflow("change X").unwrap();
        let proposal = prepare_workflow_proposal(
            &workflow.workflow_id,
            &workflow.action_id,
            "src/lib.rs",
            "1",
            "2",
            verification,
        )
        .unwrap();
        workflow.source_path = proposal.relative_path.clone();
        workflow.source_hash = proposal.original_sha256.clone();
        workflow.before_hash = proposal.original_sha256.clone();
        workflow.after_hash = proposal.proposed_sha256.clone();
        workflow.proposal_id = proposal.proposal_id.clone();
        workflow.proposal_hash = proposal.proposal_hash.clone();
        workflow.approval_credential_hash = proposal.approval_credential_hash.clone();
        workflow.verification_plan = proposal.verification_command.clone();
        workflow.approval_state = "pending".to_string();
        workflow.phase = "pending-approval".to_string();
        workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();
        (target, workflow, proposal)
    }
}
