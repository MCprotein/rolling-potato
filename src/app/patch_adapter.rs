//! Patch lifecycle, approval, verification, and recovery application adapter.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
#[cfg(test)]
use std::time::SystemTime;

use sha2::{Digest, Sha256};

use crate::adapters::filesystem::{layout as paths, lease};
use crate::app::extensions_adapter::{hooks, plugin, skill};
use crate::app::policy_adapter::{self as policy, Decision, PathMode};
use crate::app::workflow_adapter::ledger;
use crate::app::workflow_adapter::state;
use crate::app::workflow_adapter::transcript;
use crate::app::workflow_adapter::transition;
use crate::foundation::error::AppError;
use crate::runtime_core::patch::application::{
    self as application_domain, ApplyAdmission, ApplyResult, RollbackAdmission, RollbackResult,
};
use crate::runtime_core::patch::approval::{self as approval_domain, APPROVAL_TOKEN_BYTES};
use crate::runtime_core::patch::proposal::{
    self as proposal_domain, parse_header as parse_proposal_header, required_header,
    validate_proposal_id, PatchPreview, PreviewInput, ProposalRecord, RecordParse,
    MAX_PATCH_FILE_BYTES,
};
use crate::runtime_core::patch::verification::{
    self as verification_domain, RecoveryAdmission, VerificationPlan, VerificationResult,
};
use crate::surfaces::tui::outcome::unsupported_source_platform_outcome;
#[cfg(test)]
use crate::surfaces::tui::outcome::TuiEffect;
#[cfg(test)]
use crate::surfaces::tui::outcome::TuiOutcomeStatus;
use crate::surfaces::tui::outcome::{
    exact_tui_outcome, TuiOutcome, TuiOutcomeCode, TuiOutcomeContext,
};
use crate::surfaces::tui::runtime_bridge::{OneShotSecret, SelectionLease, TuiGateKind};

pub use crate::runtime_core::patch::proposal::{
    PatchProposalDetail, PatchProposalSummary, WorkflowProposal,
};

const MAX_PROPOSAL_RECORD_BYTES: usize = 2 * 1024 * 1024;

mod approval_transaction;
mod execution;
mod guard;
mod proposal_builder;
mod proposal_store;
mod resume;
mod terminal;
mod verification;
mod workflow_contract;
mod workflow_execution;

use approval_transaction::{approve_prepared_skill_transaction, prepared_approval_receipt_exists};
pub(crate) use approval_transaction::{
    recover_prepared_approval_bundle, recover_prepared_verification_bundle,
};
use execution::{
    apply_proposal, build_verification_plan, format_verification_result, restore_from_rollback,
    run_verification,
};
use guard::{
    approval_prelock_test_barrier, load_workflow_under_approval_lock, restore_bytes, ApprovalLock,
};
pub(crate) use guard::{
    approval_projection_fault, approval_transaction_fault, verification_approval_transaction_fault,
};
use proposal_builder::{
    build_preview, current_source_hash, issue_approval_token, resolve_target_for, sha256_bytes,
    write_proposal_record,
};
pub(crate) use proposal_store::proposal_detail_for_workflow_bounded;
#[cfg(test)]
use proposal_store::summary_from_path;
use proposal_store::{
    dry_run_approval_report, load_proposal_record, rollback_path_for_record,
    validate_applied_proposal, validate_token_hash,
};
#[cfg(test)]
pub use resume::proposal_summaries;
pub(crate) use resume::resume_workflow_for_tui;
pub use resume::{preflight_resume_workflow, resume_workflow_report};
pub use terminal::cancel_workflow_report;
#[cfg(test)]
pub(crate) use terminal::denial_phase_outcome_code;
#[cfg(test)]
pub use terminal::deny_pending_gate;
pub(crate) use terminal::{cancel_workflow_for_tui, deny_pending_gate_for_tui};
pub(crate) use verification::verify_for_tui;
pub use verification::verify_report;
pub(crate) use workflow_contract::is_stale_selection_error;
use workflow_contract::{
    failure_report, load_validated_approval_workflow, stale_selection_error, success_report,
    validate_outcome_id, validate_workflow_binding,
};
pub use workflow_execution::rotate_workflow_token_report;
use workflow_execution::{
    continue_approved_workflow, ensure_plugin_completion_event,
    ensure_plugin_completion_event_under_transition, finalize_verified_skill,
    plugin_completion_recovery_report, validate_completed_plugin_workflow,
    validate_completed_workflow, validate_failing_test_before, workflow_skill_runtime,
};

struct ApprovalDispatch {
    report: String,
    verification_credential: Option<OneShotSecret>,
}

impl ApprovalDispatch {
    fn without_secret(report: String) -> Self {
        Self {
            report,
            verification_credential: None,
        }
    }

    fn write_cli(mut self, proposal_id: &str) -> Result<(), AppError> {
        use std::io::Write;

        let mut stdout = std::io::stdout().lock();
        stdout
            .write_all(self.report.as_bytes())
            .map_err(|err| AppError::runtime(format!("patch approve 출력 실패: {err}")))?;
        if let Some(credential) = self.verification_credential.take() {
            stdout
                .write_all(
                    format!(
                        "\n- verification command approval: rpotato patch verify {proposal_id} --token "
                    )
                    .as_bytes(),
                )
                .map_err(|err| AppError::runtime(format!("patch approve 출력 실패: {err}")))?;
            credential
                .expose(|plaintext| stdout.write_all(plaintext.as_bytes()))
                .map_err(|err| AppError::runtime(format!("patch credential 출력 실패: {err}")))?;
        }
        stdout
            .write_all(b"\n")
            .and_then(|_| stdout.flush())
            .map_err(|err| AppError::runtime(format!("patch approve 출력 실패: {err}")))
    }

    #[cfg(test)]
    fn into_test_report(mut self, proposal_id: &str) -> String {
        if let Some(credential) = self.verification_credential.take() {
            credential.expose(|plaintext| {
                self.report
                    .push_str("\n- verification command approval: rpotato patch verify ");
                self.report.push_str(proposal_id);
                self.report.push_str(" --token ");
                self.report.push_str(plaintext);
            });
        }
        self.report
    }
}

pub fn validate_skill_verification(skill_id: &str, command: &str) -> Result<(), AppError> {
    let plan = build_verification_plan(command)?;
    if skill_id == "fix-test" && !verification_domain::is_test_plan(&plan) {
        return Err(AppError::blocked(
            "fix-test verification 차단\n- 이유: fix-test는 실제 `cargo test` command로만 전후 evidence를 만들 수 있습니다.",
        ));
    }
    Ok(())
}

pub fn record_failing_test_before(
    workflow: &state::WorkflowRecord,
    command: &str,
) -> Result<String, AppError> {
    validate_skill_verification("fix-test", command)?;
    let plan = build_verification_plan(command)?;
    let result = run_verification(&plan);
    let failed_exit = result
        .exit_code
        .parse::<i32>()
        .ok()
        .is_some_and(|code| code != 0);
    if !failed_exit {
        return Err(AppError::blocked(format!(
            "fix-test 시작 차단\n- 이유: patch 전 실제 test failure를 관측하지 못했습니다.\n- exit code: {}\n- command: {}",
            result.exit_code,
            ledger::redact_text(&result.command)
        )));
    }
    state::record_event(
        "skill.test_failure.observed",
        "fix-test patch 전 실패 관측",
        &format!(
            "workflow_id={} command_hash={} exit_code={} stdout_hash={} stderr_hash={}",
            workflow.workflow_id,
            state::sha256_text(&plan.command),
            result.exit_code,
            state::sha256_text(&result.stdout),
            state::sha256_text(&result.stderr)
        ),
    )
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

pub fn approve_to_stdout(
    proposal_id: &str,
    token: &str,
    dry_run: bool,
    verify_command: Option<&str>,
) -> Result<(), AppError> {
    let intent_id = format!("intent-approve-{proposal_id}");
    approve_dispatch_for_intent(
        proposal_id,
        token,
        dry_run,
        verify_command,
        &intent_id,
        None,
    )?
    .write_cli(proposal_id)
}

#[cfg(test)]
pub fn approve_report(
    proposal_id: &str,
    token: &str,
    dry_run: bool,
    verify_command: Option<&str>,
) -> Result<String, AppError> {
    let intent_id = format!("intent-approve-{proposal_id}");
    approve_report_for_intent(proposal_id, token, dry_run, verify_command, &intent_id)
}

#[cfg(test)]
pub(crate) fn approve_report_for_intent(
    proposal_id: &str,
    token: &str,
    dry_run: bool,
    verify_command: Option<&str>,
    intent_id: &str,
) -> Result<String, AppError> {
    approve_dispatch_for_intent(proposal_id, token, dry_run, verify_command, intent_id, None)
        .map(|dispatch| dispatch.into_test_report(proposal_id))
}

fn approve_dispatch_for_intent(
    proposal_id: &str,
    token: &str,
    dry_run: bool,
    verify_command: Option<&str>,
    intent_id: &str,
    expected_lease: Option<&SelectionLease>,
) -> Result<ApprovalDispatch, AppError> {
    validate_proposal_id(proposal_id)?;
    validate_outcome_id(intent_id, "intent")?;
    ensure_source_install_platform_supported(cfg!(unix), std::env::consts::OS, dry_run)?;
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
            validate_completed_workflow(&workflow)?;
            state::clear_terminal_workflow_pointer(&workflow)?;
            return Ok(ApprovalDispatch::without_secret(success_report(&workflow)));
        }
        if workflow.phase == "failed" {
            return Err(AppError::blocked(failure_report(&workflow)));
        }
        return dry_run_approval_report(&record, verify_command)
            .map(ApprovalDispatch::without_secret);
    }

    approval_prelock_test_barrier()?;
    let _approval_lock = ApprovalLock::acquire(&record.proposal_id)?;
    let discovered_active = state::active_workflow_id()?;
    let workflow = load_validated_approval_workflow(&record, token, discovered_active.as_deref())?;
    if workflow.phase == "complete" {
        validate_completed_workflow(&workflow)?;
        state::clear_terminal_workflow_pointer(&workflow)?;
        return Ok(ApprovalDispatch::without_secret(success_report(&workflow)));
    }
    if workflow.phase == "failed" {
        return Err(AppError::blocked(failure_report(&workflow)));
    }
    if workflow.phase == "pending-verification-approval"
        && prepared_approval_receipt_exists(&record, &workflow, intent_id)?
    {
        return Ok(ApprovalDispatch::without_secret(format!(
            "patch approve\n- status: refresh-only\n- code: secret.refresh-only\n- proposal id: {}\n- workflow id: {}\n- intent: {}\n- applied sha256: {}\n- verification approval: pending\n- boundary: 동일 intent의 exact E0..E9 커밋 영수증만 반환하며 approval token 또는 verification credential을 다시 표시하지 않습니다.",
            record.proposal_id,
            workflow.workflow_id,
            intent_id,
            record.proposed_sha256,
        )));
    }

    if workflow.phase == "pending-approval" {
        if workflow.active_skill_id.is_empty() {
            return Err(AppError::blocked(
                "patch approve 차단\n- 이유: active built-in skill이 없는 legacy workflow는 exact prepared E0..E9 트랜잭션을 사용할 수 없습니다.\n- 동작: 새 canonical workflow proposal을 생성하세요.",
            ));
        }
        return approve_prepared_skill_transaction(record, workflow, intent_id, expected_lease);
    }

    Err(AppError::blocked(format!(
        "patch approve 차단\n- 이유: workflow phase가 exact prepared approval을 허용하지 않습니다.\n- phase: {}",
        workflow.phase
    )))
}

fn ensure_source_install_platform_supported(
    is_unix: bool,
    platform: &str,
    dry_run: bool,
) -> Result<(), AppError> {
    if !is_unix && !dry_run {
        return Err(AppError::blocked(
            unsupported_source_platform_outcome(platform)?.safe_message,
        ));
    }
    Ok(())
}

pub(crate) fn approve_for_tui(
    proposal_id: &str,
    token: &str,
    intent_id: &str,
    lease: &SelectionLease,
) -> Result<Option<OneShotSecret>, AppError> {
    let dispatch =
        approve_dispatch_for_intent(proposal_id, token, false, None, intent_id, Some(lease))?;
    Ok(dispatch.verification_credential)
}

fn display_none(value: &str) -> &str {
    if value.is_empty() {
        "none"
    } else {
        value
    }
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
#[path = "patch_adapter/tests/mod.rs"]
mod tests;
