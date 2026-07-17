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

mod execution;
mod guard;
mod proposal_builder;
mod proposal_store;
mod resume;
mod terminal;
mod verification;
mod workflow_contract;
mod workflow_execution;

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

fn prepared_approval_receipt_exists(
    record: &ProposalRecord,
    workflow: &state::WorkflowRecord,
    intent_id: &str,
) -> Result<bool, AppError> {
    let expected_types = [
        "runtime.intent.accepted",
        "workflow.checkpoint",
        "patch.apply.approved",
        "hook.dispatched",
        "hook.dispatched",
        "hook.dispatched",
        "hook.dispatched",
        "patch.applied",
        "transcript.recorded",
        "workflow.checkpoint",
    ];
    let e0_details = format!(
        "intent_id={intent_id} intent_kind=approve-patch workflow_id={} proposal_id={}",
        workflow.workflow_id, record.proposal_id
    );
    let events = ledger::read_runtime_events()?;
    let Some(start) = events.iter().position(|event| {
        event.event_type == "runtime.intent.accepted"
            && event.project_id == workflow.project_id
            && event.session_id == workflow.session_id
            && event.details == e0_details
    }) else {
        return Ok(false);
    };
    let Some(receipt) = events.get(start..start + expected_types.len()) else {
        return Ok(false);
    };
    if receipt
        .iter()
        .zip(expected_types)
        .any(|(event, expected)| event.event_type != expected)
    {
        return Ok(false);
    }
    let e7 = &receipt[7];
    let e9 = &receipt[9];
    Ok(e7
        .details
        .contains(&format!("proposal_id={}", record.proposal_id))
        && e7
            .details
            .contains(&format!("applied_sha256={}", record.proposed_sha256))
        && e9.details.contains(&format!(
            "workflow_id={} revision={} artifact_hash={}",
            workflow.workflow_id, workflow.revision, workflow.artifact_hash
        )))
}

struct ApprovalSourcePreflight {
    relative_path: String,
    before: String,
    source_install: transition::SourceInstallV1,
}

fn approve_prepared_skill_transaction(
    record: ProposalRecord,
    observed_workflow: state::WorkflowRecord,
    intent_id: &str,
    expected_lease: Option<&SelectionLease>,
) -> Result<ApprovalDispatch, AppError> {
    let identity = ledger::validated_current_identity()?;
    let current_lease = state::current_state_lease_view()?;
    let observed_ledger = ledger::validated_ledger_binding()?;
    let source = prepare_approval_source(&record, intent_id)?;

    let transition_guard = transition::TransitionGuard::acquire_for(
        &identity.project_id,
        transition::CurrentStateIntent::ApprovePatch,
    )?;
    if let Some(lease) = expected_lease {
        if !state::tui_lease_matches_workflow_under_transition(
            lease,
            &observed_workflow.workflow_id,
        )? {
            return Err(stale_selection_error());
        }
    }
    let workflow_guard = state::WorkflowCheckpointGuard::acquire(&observed_workflow.workflow_id)?;
    let current = workflow_guard.load_current()?;
    if current != observed_workflow {
        return Err(AppError::blocked(
            "prepared approval workflow가 lock 획득 전에 변경되었습니다.",
        ));
    }
    validate_workflow_binding(&current, &record)?;
    let mut runtime = workflow_skill_runtime(&current)?.ok_or_else(|| {
        AppError::blocked("prepared approval은 registered built-in skill workflow가 필요합니다.")
    })?;
    validate_skill_verification(&runtime.active_skill_id, &record.verification_command)?;
    validate_failing_test_before(&current, &runtime)?;
    if runtime.state != skill::SkillState::AwaitingApproval {
        return Err(AppError::blocked(format!(
            "skill side effect 차단\n- workflow phase: {}\n- skill state: {}\n- expected skill state: awaiting-approval",
            current.phase,
            runtime.state.label()
        )));
    }
    if state::current_state_lease_view_under_transition()? != current_lease {
        return Err(AppError::blocked(
            "prepared approval current-state lease가 lock 획득 전에 변경되었습니다.",
        ));
    }

    let writer = ledger::LedgerWriterGuard::acquire()?;
    let ledger_binding = writer.binding()?;
    if ledger_binding != observed_ledger {
        return Err(AppError::blocked(
            "prepared approval ledger head가 lock 획득 전에 변경되었습니다.",
        ));
    }

    let mut approved = current.clone();
    approved.phase = "approved".to_string();
    approved.approval_state = "approved".to_string();
    let r1 = workflow_guard.prepare_revision(&current, approved)?;

    let e0 = ledger::new_event_for(
        &identity,
        "runtime.intent.accepted",
        "interactive runtime intent accepted",
        &format!(
            "intent_id={intent_id} intent_kind=approve-patch workflow_id={} proposal_id={}",
            current.workflow_id, record.proposal_id
        ),
    );
    let e2 = ledger::new_event_for(
        &identity,
        "patch.apply.approved",
        "patch apply approval durably accepted",
        &format!(
            "intent_id={intent_id} workflow_id={} proposal_id={} path={} original_sha256={} proposed_sha256={}",
            current.workflow_id,
            record.proposal_id,
            record.relative_path,
            record.original_sha256,
            record.proposed_sha256
        ),
    );
    let e3 = prepare_transaction_hook_event(
        &r1.record,
        &mut runtime,
        "pre_tool_call",
        "apply_patch",
        &identity,
    )?;
    let e4 = prepare_transaction_hook_event(
        &r1.record,
        &mut runtime,
        "pre_patch_apply",
        "apply_patch",
        &identity,
    )?;
    let e5 = prepare_transaction_hook_event(
        &r1.record,
        &mut runtime,
        "post_patch_apply",
        "apply_patch",
        &identity,
    )?;
    let e6 = prepare_transaction_hook_event(
        &r1.record,
        &mut runtime,
        "post_tool_result",
        "apply_patch",
        &identity,
    )?;
    runtime.record_stop_criterion("patch_applied");
    runtime.transition(skill::SkillState::AwaitingVerification)?;
    let e7 = ledger::new_event_for(
        &identity,
        "patch.applied",
        "approved patch applied",
        &format!(
            "proposal_id={} path={} original_sha256={} applied_sha256={} verification=not-requested",
            record.proposal_id,
            record.relative_path,
            record.original_sha256,
            record.proposed_sha256
        ),
    );
    let source_pointer = crate::context::SourcePointer {
        path: source.relative_path.clone(),
        stable_ref: format!("{}:1", source.relative_path),
        chars: 0,
        fingerprint: record.proposed_sha256.clone(),
        snippet: String::new(),
    };
    let transcript = transcript::prepare_no_stream_tool_turn(
        &r1.record,
        &e7.event_id,
        &format!(
            "patch applied: proposal_id={} path={} original_sha256={} applied_sha256={}",
            record.proposal_id,
            record.relative_path,
            record.original_sha256,
            record.proposed_sha256
        ),
        &[source_pointer],
    )?;
    let verification_plaintext = issue_approval_token()?;
    let mut pending = r1.record.clone();
    pending.phase = "pending-verification-approval".to_string();
    pending.approval_state = "applied".to_string();
    pending.verification_credential_hash = sha256_text(&verification_plaintext);
    let verification_token = OneShotSecret::new(verification_plaintext)?;
    pending.verification_approval_state = "pending".to_string();
    pending.result_summary = "patch applied; verification approval pending".to_string();
    runtime.store_in_workflow(&mut pending);
    let r2 = workflow_guard.prepare_revision(&r1.record, pending)?;

    let semantic_events = vec![
        e0,
        r1.event.clone(),
        e2,
        e3,
        e4,
        e5,
        e6,
        e7.clone(),
        transcript.event.clone(),
        r2.event.clone(),
    ];
    let planned = writer.plan_events(&semantic_events)?;
    let final_binding = ledger::LedgerBinding {
        event_count: planned[9].ordinal,
        event_id: Some(planned[9].event.event_id.clone()),
        event_hash: planned[9].event_hash.clone(),
    };
    let current_image = state::prepare_current_image(&r2.record, &final_binding)?;
    let mut bundle = transition::prepare_source_bundle_with_context(
        intent_id,
        Some(&current.workflow_id),
        source.source_install,
        source.before.as_bytes(),
        record.proposed_content.as_bytes(),
        transition::PreparedBundleContext {
            identity: &identity,
            lease: &current_lease,
            ledger_binding,
        },
    )?;
    transition::bind_planned_events(&mut bundle, &planned)?;
    let lag = transition::prepare_projection_lag_member(intent_id, &planned)?;
    let members =
        prepared_approval_members(&r1, &r2, &transcript, &current_image, lag, &semantic_events);
    transition::bind_additional_members(&mut bundle, members)?;
    state::transition_project_current_state_prepared_approval(state::PreparedApprovalTransition {
        transition_guard: Some(&transition_guard),
        workflow_guard: &workflow_guard,
        writer: &writer,
        planned: &planned,
        bundle: &bundle,
        r1: &r1,
        r2: &r2,
        transcript: &transcript,
        current: &current_image,
        events: &semantic_events,
    })?;
    let rollback_path = transition::resolve_prepared_project_path(
        &bundle
            .source_install
            .as_ref()
            .ok_or_else(|| AppError::blocked("prepared approval source_install_v1 누락"))?
            .rollback_final,
    )?;
    Ok(ApprovalDispatch {
        report: format!(
        "patch approve\n- status: applied-awaiting-verification\n- proposal id: {}\n- path: {}\n- approval token: accepted\n- applied sha256: {}\n- rollback record: {}\n- verification command: {}\n- verification approval: required\n- ledger event: {}\n- intent: {}\n- boundary: exact prepared journal과 E0..E9를 수렴한 뒤 patch만 적용했으며 verification command는 아직 실행하지 않았습니다.",
        record.proposal_id,
        source.relative_path,
        record.proposed_sha256,
        rollback_path.display(),
        ledger::redact_text(&record.verification_command),
        e7.event_id,
        intent_id,
        ),
        verification_credential: Some(verification_token),
    })
}

fn prepare_transaction_hook_event(
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

fn prepared_approval_members(
    r1: &state::PreparedWorkflowRevision,
    r2: &state::PreparedWorkflowRevision,
    transcript: &transcript::PreparedTranscriptTurn,
    current: &state::PreparedCurrentImage,
    lag: transition::PreparedMember,
    events: &[ledger::LedgerEvent],
) -> Vec<transition::PreparedMember> {
    use transition::{PreparedMember, PreparedMemberBinding, PreparedMemberKind};
    let member = |kind,
                  path: String,
                  schema_version,
                  artifact_id: String,
                  causal_id: Option<String>,
                  event_id: String,
                  bytes_utf8: String,
                  expected_type: &str,
                  expected_identity: Option<String>,
                  role| PreparedMember {
        kind,
        path,
        schema_version,
        binding: PreparedMemberBinding {
            artifact_id: Some(artifact_id),
            causal_id,
            source_key: None,
            event_id: Some(event_id),
        },
        bytes_utf8,
        expected_type: expected_type.to_string(),
        expected_identity,
        readonly: false,
        mode: 0o600,
        ownership: None,
        semantic_role_rank: role,
    };
    vec![
        member(
            PreparedMemberKind::ToolOutput,
            transcript.tool_stored_path.clone(),
            1,
            transcript.tool_artifact_id.clone(),
            Some(events[7].event_id.clone()),
            events[7].event_id.clone(),
            transcript.tool_bytes.clone(),
            "absent",
            None,
            0,
        ),
        member(
            PreparedMemberKind::TranscriptV2,
            transcript.transcript_stored_path.clone(),
            2,
            transcript.record.record_id.clone(),
            Some(transcript.tool_artifact_id.clone()),
            events[8].event_id.clone(),
            transcript.transcript_bytes.clone(),
            "absent",
            None,
            0,
        ),
        member(
            PreparedMemberKind::WorkflowSnapshot,
            r1.snapshot_stored_path.clone(),
            4,
            r1.snapshot_member_id.clone(),
            None,
            events[1].event_id.clone(),
            r1.snapshot_bytes.clone(),
            "absent",
            None,
            0,
        ),
        member(
            PreparedMemberKind::WorkflowSnapshot,
            r2.snapshot_stored_path.clone(),
            4,
            r2.snapshot_member_id.clone(),
            None,
            events[9].event_id.clone(),
            r2.snapshot_bytes.clone(),
            "absent",
            None,
            1,
        ),
        member(
            PreparedMemberKind::WorkflowPointer,
            r1.pointer_stored_path.clone(),
            4,
            r1.pointer_member_id.clone(),
            Some(r1.snapshot_member_id.clone()),
            events[1].event_id.clone(),
            r1.pointer_bytes.clone(),
            "file",
            None,
            0,
        ),
        member(
            PreparedMemberKind::WorkflowPointer,
            r2.pointer_stored_path.clone(),
            4,
            r2.pointer_member_id.clone(),
            Some(r2.snapshot_member_id.clone()),
            events[9].event_id.clone(),
            r2.pointer_bytes.clone(),
            "file",
            None,
            1,
        ),
        member(
            PreparedMemberKind::CurrentImage,
            current.stored_path.clone(),
            2,
            current.artifact_id.clone(),
            Some(r2.snapshot_member_id.clone()),
            events[9].event_id.clone(),
            current.bytes.clone(),
            "file",
            None,
            0,
        ),
        lag,
    ]
}

pub(crate) fn recover_prepared_approval_bundle(
    bundle: &transition::PreparedSourceBundle,
    journal: &Path,
) -> Result<(), AppError> {
    let expected_event_types = [
        "runtime.intent.accepted",
        "workflow.checkpoint",
        "patch.apply.approved",
        "hook.dispatched",
        "hook.dispatched",
        "hook.dispatched",
        "hook.dispatched",
        "patch.applied",
        "transcript.recorded",
        "workflow.checkpoint",
    ];
    if bundle.additional_members.len() != 8
        || bundle.semantic_events.len() != expected_event_types.len()
        || bundle
            .semantic_events
            .iter()
            .zip(expected_event_types)
            .any(|(event, expected)| event.event_type != expected)
    {
        return Err(AppError::blocked(
            "prepared approval recovery exact E0..E9 shape 불일치",
        ));
    }
    let workflow_id = bundle
        .workflow_id
        .as_deref()
        .ok_or_else(|| AppError::blocked("prepared approval recovery workflow 누락"))?;
    let events = &bundle.semantic_events;
    let members = &bundle.additional_members;
    let planned = transition::planned_events(bundle)?;
    let r1 = state::decode_prepared_workflow_revision(
        workflow_id,
        &members[2],
        &members[4],
        &events[1],
    )?;
    let r2 = state::decode_prepared_workflow_revision(
        workflow_id,
        &members[3],
        &members[5],
        &events[9],
    )?;
    let expected_r2_revision = r1
        .record
        .revision
        .checked_add(1)
        .ok_or_else(|| AppError::blocked("prepared approval R+2 revision overflow"))?;
    if r2.record.revision != expected_r2_revision
        || r2.record.previous_hash != r1.record.artifact_hash
        || r2.record.project_id != bundle.project_id
        || r2.record.session_id != bundle.session_id
        || r1.record.project_id != bundle.project_id
        || r1.record.session_id != bundle.session_id
    {
        return Err(AppError::blocked(
            "prepared approval recovery R+1/R+2 chain 불일치",
        ));
    }
    validate_prepared_approval_semantics(bundle, &r1.record)?;
    let transcript =
        transcript::decode_prepared_no_stream_tool_turn(&members[0], &members[1], &events[8])?;
    if transcript.record.causal_id != events[7].event_id
        || transcript.record.workflow_id != workflow_id
    {
        return Err(AppError::blocked(
            "prepared approval recovery transcript E7 binding 불일치",
        ));
    }
    let final_binding = ledger::LedgerBinding {
        event_count: planned[9].ordinal,
        event_id: Some(planned[9].event.event_id.clone()),
        event_hash: planned[9].event_hash.clone(),
    };
    let current_image = state::decode_prepared_current_image(
        &members[6],
        &r2.record,
        &final_binding,
        &r2.snapshot_member_id,
        &events[9].event_id,
    )?;
    state::validate_current_state_recovery_cas(
        bundle.current_revision,
        &bundle.current_artifact_hash,
        Some(&current_image.bytes),
    )?;
    state::validate_prepared_source_parent(bundle)?;

    let workflow_guard = recovery_context(
        "lock-workflow",
        state::WorkflowCheckpointGuard::acquire(workflow_id),
    )?;
    let predecessor_revision = r1
        .record
        .revision
        .checked_sub(1)
        .ok_or_else(|| AppError::blocked("prepared approval predecessor revision underflow"))?;
    let allowed = [
        (predecessor_revision, r1.record.previous_hash.as_str()),
        (r1.record.revision, r1.record.artifact_hash.as_str()),
        (r2.record.revision, r2.record.artifact_hash.as_str()),
    ];
    let installed = recovery_context(
        "load-workflow",
        workflow_guard.load_recovery_current(&allowed),
    )?;
    let valid_predecessor = installed.revision.checked_add(1) == Some(r1.record.revision)
        && installed.artifact_hash == r1.record.previous_hash;
    if installed != r1.record && installed != r2.record && !valid_predecessor {
        return Err(AppError::blocked(
            "prepared approval recovery workflow predecessor conflict",
        ));
    }
    let writer = recovery_context("lock-ledger", ledger::LedgerWriterGuard::acquire())?;
    recovery_context(
        "prepared-approval-transition",
        state::recover_project_current_state_prepared_approval(
            state::PreparedApprovalTransition {
                transition_guard: None,
                workflow_guard: &workflow_guard,
                writer: &writer,
                planned: &planned,
                bundle,
                r1: &r1,
                r2: &r2,
                transcript: &transcript,
                current: &current_image,
                events,
            },
            journal,
        ),
    )
}

pub(crate) fn recover_prepared_verification_bundle(
    bundle: &transition::PreparedSourceBundle,
    journal: &Path,
) -> Result<(), AppError> {
    let expected_event_types = [
        "runtime.intent.accepted",
        "workflow.checkpoint",
        "patch.verification.approved",
    ];
    if bundle.intent_kind != "approve-verification"
        || bundle.source_install.is_some()
        || bundle.additional_members.len() != 3
        || bundle.semantic_events.len() != expected_event_types.len()
        || bundle
            .semantic_events
            .iter()
            .zip(expected_event_types)
            .any(|(event, expected)| event.event_type != expected)
    {
        return Err(AppError::blocked(
            "prepared verification recovery exact shape 불일치",
        ));
    }
    let workflow_id = bundle
        .workflow_id
        .as_deref()
        .ok_or_else(|| AppError::blocked("prepared verification recovery workflow 누락"))?;
    let events = &bundle.semantic_events;
    let members = &bundle.additional_members;
    let planned = transition::planned_events(bundle)?;
    let revision = state::decode_prepared_workflow_revision(
        workflow_id,
        &members[0],
        &members[1],
        &events[1],
    )?;
    if revision.record.project_id != bundle.project_id
        || revision.record.session_id != bundle.session_id
        || revision.record.phase != "verification-started"
        || revision.record.verification_approval_state != "approved"
    {
        return Err(AppError::blocked(
            "prepared verification workflow semantic binding 불일치",
        ));
    }
    let e0_details = format!(
        "intent_id={} intent_kind=approve-verification workflow_id={} proposal_id={}",
        bundle.intent_id, revision.record.workflow_id, revision.record.proposal_id
    );
    let e2_details = format!(
        "intent_id={} workflow_id={} proposal_id={} gate=verification-command revision={} artifact_hash={} command_hash={}",
        bundle.intent_id,
        revision.record.workflow_id,
        revision.record.proposal_id,
        revision.record.revision,
        revision.record.artifact_hash,
        sha256_text(&revision.record.verification_plan),
    );
    if events[0].summary != "interactive runtime intent accepted"
        || events[0].details != e0_details
        || events[2].summary != "verification command approval durably accepted"
        || events[2].details != e2_details
    {
        return Err(AppError::blocked(
            "prepared verification E0/E2 semantic binding 불일치",
        ));
    }
    let runtime = workflow_skill_runtime(&revision.record)?.ok_or_else(|| {
        AppError::blocked("prepared verification active built-in skill manifest 누락")
    })?;
    if runtime.state != skill::SkillState::AwaitingVerification {
        return Err(AppError::blocked(
            "prepared verification skill state binding 불일치",
        ));
    }
    let final_binding = ledger::LedgerBinding {
        event_count: planned[2].ordinal,
        event_id: Some(planned[2].event.event_id.clone()),
        event_hash: planned[2].event_hash.clone(),
    };
    let current_image = state::decode_prepared_current_image(
        &members[2],
        &revision.record,
        &final_binding,
        &revision.snapshot_member_id,
        &events[2].event_id,
    )?;
    state::validate_current_state_recovery_cas(
        bundle.current_revision,
        &bundle.current_artifact_hash,
        Some(&current_image.bytes),
    )?;

    let workflow_guard = recovery_context(
        "verification-lock-workflow",
        state::WorkflowCheckpointGuard::acquire(workflow_id),
    )?;
    let predecessor_revision =
        revision.record.revision.checked_sub(1).ok_or_else(|| {
            AppError::blocked("prepared verification predecessor revision underflow")
        })?;
    let allowed = [
        (predecessor_revision, revision.record.previous_hash.as_str()),
        (
            revision.record.revision,
            revision.record.artifact_hash.as_str(),
        ),
    ];
    let installed = recovery_context(
        "verification-load-workflow",
        workflow_guard.load_recovery_current(&allowed),
    )?;
    let valid_predecessor = installed.revision.checked_add(1) == Some(revision.record.revision)
        && installed.artifact_hash == revision.record.previous_hash;
    if installed != revision.record && !valid_predecessor {
        return Err(AppError::blocked(
            "prepared verification recovery workflow predecessor conflict",
        ));
    }
    let writer = recovery_context(
        "verification-lock-ledger",
        ledger::LedgerWriterGuard::acquire(),
    )?;
    recovery_context(
        "prepared-verification-transition",
        state::recover_project_current_state_prepared_verification(
            state::PreparedVerificationTransition {
                transition_guard: None,
                workflow_guard: &workflow_guard,
                writer: &writer,
                planned: &planned,
                bundle,
                revision: &revision,
                current: &current_image,
                events,
            },
            journal,
        ),
    )
}

fn validate_prepared_approval_semantics(
    bundle: &transition::PreparedSourceBundle,
    approved: &state::WorkflowRecord,
) -> Result<(), AppError> {
    let events = &bundle.semantic_events;
    let source_install = bundle
        .source_install
        .as_ref()
        .ok_or_else(|| AppError::blocked("prepared approval source_install_v1 누락"))?;
    let identity = ledger::RuntimeIdentity {
        project_id: bundle.project_id.clone(),
        session_id: bundle.session_id.clone(),
        project_root: paths::project_root().display().to_string(),
    };
    let e0_details = format!(
        "intent_id={} intent_kind=approve-patch workflow_id={} proposal_id={}",
        bundle.intent_id, approved.workflow_id, approved.proposal_id
    );
    let e2_details = format!(
        "intent_id={} workflow_id={} proposal_id={} path={} original_sha256={} proposed_sha256={}",
        bundle.intent_id,
        approved.workflow_id,
        approved.proposal_id,
        approved.source_path,
        approved.before_hash,
        approved.after_hash
    );
    let e7_details = format!(
        "proposal_id={} path={} original_sha256={} applied_sha256={} verification=not-requested",
        approved.proposal_id, approved.source_path, approved.before_hash, approved.after_hash
    );
    if approved.proposal_id.is_empty()
        || source_install.target.path != approved.source_path
        || source_install.before_sha256 != approved.before_hash
        || source_install.proposed_sha256 != approved.after_hash
        || events[0].summary != "interactive runtime intent accepted"
        || events[0].details != e0_details
        || events[2].summary != "patch apply approval durably accepted"
        || events[2].details != e2_details
        || events[7].summary != "approved patch applied"
        || events[7].details != e7_details
    {
        return Err(AppError::blocked(
            "prepared approval E0/E2/E7 source/workflow semantic binding 불일치",
        ));
    }
    let manifest = skill::find_skill(&approved.active_skill_id).ok_or_else(|| {
        AppError::blocked("prepared approval active built-in skill manifest 누락")
    })?;
    for (index, hook, tool) in [
        (3, "pre_tool_call", Some("apply_patch")),
        (4, "pre_patch_apply", None),
        (5, "post_patch_apply", None),
        (6, "post_tool_result", Some("apply_patch")),
    ] {
        hooks::validate_prepared_native_lifecycle_event(
            hooks::HookInput {
                hook,
                workflow_id: Some(&approved.workflow_id),
                active_skill_id: Some(&approved.active_skill_id),
                mode: manifest.mode,
                payload: "apply_patch",
            },
            tool,
            &identity,
            &events[index],
        )?;
    }
    Ok(())
}

fn recovery_context<T>(stage: &str, result: Result<T, AppError>) -> Result<T, AppError> {
    result.map_err(|error| AppError {
        code: error.code,
        message: format!(
            "prepared approval recovery stage 실패\n- stage: {stage}\n- error: {}",
            error.message
        ),
    })
}

fn prepare_approval_source(
    record: &ProposalRecord,
    intent_id: &str,
) -> Result<ApprovalSourcePreflight, AppError> {
    let target = resolve_target_for("patch approve", &record.relative_path)?;
    let read_decision = policy::classify_path(PathMode::Read, &target.relative_path)?;
    let write_decision = policy::classify_path(PathMode::Write, &target.relative_path)?;
    if read_decision.decision != Decision::Allow || write_decision.decision == Decision::Deny {
        return Err(AppError::blocked(
            "prepared patch approve source policy가 allow가 아닙니다.",
        ));
    }
    let metadata = fs::metadata(&target.absolute_path)
        .map_err(|err| AppError::blocked(format!("prepared patch target metadata 실패: {err}")))?;
    if !metadata.is_file() || metadata.len() > MAX_PATCH_FILE_BYTES {
        return Err(AppError::blocked(
            "prepared patch target type/size boundary 불일치",
        ));
    }
    let before = fs::read_to_string(&target.absolute_path)
        .map_err(|err| AppError::blocked(format!("prepared patch target read 실패: {err}")))?;
    let before_hash = sha256_text(&before);
    if before_hash != record.original_sha256
        || sha256_text(&record.proposed_content) != record.proposed_sha256
    {
        return Err(AppError::blocked(
            "prepared patch source/proposal hash binding 불일치",
        ));
    }
    let source_install = transition::prepare_source_install_v1(
        intent_id,
        &record.proposal_id,
        &target.absolute_path,
        before.as_bytes(),
        record.proposed_content.as_bytes(),
    )?;
    Ok(ApprovalSourcePreflight {
        relative_path: target.relative_path,
        before,
        source_install,
    })
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
#[path = "patch/tests/mod.rs"]
mod tests;
