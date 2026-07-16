use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
#[cfg(test)]
use std::time::SystemTime;

use sha2::{Digest, Sha256};

use crate::adapters::filesystem::{layout as paths, lease};
use crate::foundation::error::AppError;
use crate::ledger;
use crate::policy::{self, Decision, PathMode};
use crate::runtime::{
    exact_tui_outcome, OneShotSecret, TuiOutcome, TuiOutcomeCode, TuiOutcomeContext,
};
#[cfg(test)]
use crate::runtime::{TuiEffect, TuiOutcomeStatus};
use crate::runtime_core::patch::proposal::{
    self as proposal_domain, parse_header as parse_proposal_header, required_header,
    validate_proposal_id, PatchPreview, PreviewInput, ProposalRecord, RecordParse,
    MAX_PATCH_FILE_BYTES,
};
use crate::state;

pub use crate::runtime_core::patch::proposal::{
    PatchProposalDetail, PatchProposalSummary, WorkflowProposal,
};

const MAX_VERIFICATION_OUTPUT_CHARS: usize = 2_000;
const MAX_PROPOSAL_RECORD_BYTES: usize = 2 * 1024 * 1024;

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

pub fn validate_skill_verification(skill_id: &str, command: &str) -> Result<(), AppError> {
    let plan = build_verification_plan(command)?;
    if skill_id == "fix-test" && !is_test_verification(&plan) {
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
    expected_lease: Option<&crate::runtime::SelectionLease>,
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
            crate::runtime::unsupported_source_platform_outcome(platform)?.safe_message,
        ));
    }
    Ok(())
}

pub(crate) fn approve_for_tui(
    proposal_id: &str,
    token: &str,
    intent_id: &str,
    lease: &crate::runtime::SelectionLease,
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
    source_install: crate::transition::SourceInstallV1,
}

fn approve_prepared_skill_transaction(
    record: ProposalRecord,
    observed_workflow: state::WorkflowRecord,
    intent_id: &str,
    expected_lease: Option<&crate::runtime::SelectionLease>,
) -> Result<ApprovalDispatch, AppError> {
    let identity = ledger::validated_current_identity()?;
    let current_lease = state::current_state_lease_view()?;
    let observed_ledger = ledger::validated_ledger_binding()?;
    let source = prepare_approval_source(&record, intent_id)?;

    let transition_guard = crate::transition::TransitionGuard::acquire_for(
        &identity.project_id,
        crate::transition::CurrentStateIntent::ApprovePatch,
    )?;
    if let Some(lease) = expected_lease {
        if !crate::runtime::tui_lease_matches_workflow_under_transition(
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
    if runtime.state != crate::skill::SkillState::AwaitingApproval {
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
    runtime.transition(crate::skill::SkillState::AwaitingVerification)?;
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
    let transcript = crate::transcript::prepare_no_stream_tool_turn(
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
    let mut bundle = crate::transition::prepare_source_bundle_with_context(
        intent_id,
        Some(&current.workflow_id),
        source.source_install,
        source.before.as_bytes(),
        record.proposed_content.as_bytes(),
        crate::transition::PreparedBundleContext {
            identity: &identity,
            lease: &current_lease,
            ledger_binding,
        },
    )?;
    crate::transition::bind_planned_events(&mut bundle, &planned)?;
    let lag = crate::transition::prepare_projection_lag_member(intent_id, &planned)?;
    let members =
        prepared_approval_members(&r1, &r2, &transcript, &current_image, lag, &semantic_events);
    crate::transition::bind_additional_members(&mut bundle, members)?;
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
    let rollback_path = crate::transition::resolve_prepared_project_path(
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
    runtime: &mut crate::skill::SkillRuntimeState,
    hook: &str,
    tool: &str,
    identity: &ledger::RuntimeIdentity,
) -> Result<ledger::LedgerEvent, AppError> {
    let mode = crate::skill::find_skill(&runtime.active_skill_id)
        .map(|manifest| manifest.mode)
        .unwrap_or("unknown");
    let (_, event) = crate::hooks::prepare_native_lifecycle_event(
        crate::hooks::HookInput {
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
    transcript: &crate::transcript::PreparedTranscriptTurn,
    current: &state::PreparedCurrentImage,
    lag: crate::transition::PreparedMember,
    events: &[ledger::LedgerEvent],
) -> Vec<crate::transition::PreparedMember> {
    use crate::transition::{PreparedMember, PreparedMemberBinding, PreparedMemberKind};
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
    bundle: &crate::transition::PreparedSourceBundle,
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
    let planned = crate::transition::planned_events(bundle)?;
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
    let transcript = crate::transcript::decode_prepared_no_stream_tool_turn(
        &members[0],
        &members[1],
        &events[8],
    )?;
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
    bundle: &crate::transition::PreparedSourceBundle,
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
    let planned = crate::transition::planned_events(bundle)?;
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
    if runtime.state != crate::skill::SkillState::AwaitingVerification {
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
    bundle: &crate::transition::PreparedSourceBundle,
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
    let manifest = crate::skill::find_skill(&approved.active_skill_id).ok_or_else(|| {
        AppError::blocked("prepared approval active built-in skill manifest 누락")
    })?;
    for (index, hook, tool) in [
        (3, "pre_tool_call", Some("apply_patch")),
        (4, "pre_patch_apply", None),
        (5, "post_patch_apply", None),
        (6, "post_tool_result", Some("apply_patch")),
    ] {
        crate::hooks::validate_prepared_native_lifecycle_event(
            crate::hooks::HookInput {
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
    let source_install = crate::transition::prepare_source_install_v1(
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

pub fn verify_report(proposal_id: &str, token: &str) -> Result<String, AppError> {
    let intent_id = format!("intent-verify-{proposal_id}");
    verify_report_for_intent(proposal_id, token, &intent_id, None)
}

pub(crate) fn verify_for_tui(
    proposal_id: &str,
    token: &str,
    intent_id: &str,
    lease: &crate::runtime::SelectionLease,
) -> Result<String, AppError> {
    verify_report_for_intent(proposal_id, token, intent_id, Some(lease))
}

fn verify_report_for_intent(
    proposal_id: &str,
    token: &str,
    intent_id: &str,
    expected_lease: Option<&crate::runtime::SelectionLease>,
) -> Result<String, AppError> {
    validate_proposal_id(proposal_id)?;
    validate_outcome_id(intent_id, "intent")?;
    crate::transition::recover_pending_source_bundles()?;
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
        validate_completed_workflow(&workflow)?;
        state::clear_terminal_workflow_pointer(&workflow)?;
        return Ok(success_report(&workflow));
    }
    if workflow.phase == "failed" {
        return Err(AppError::blocked(failure_report(&workflow)));
    }
    if workflow.phase != "pending-verification-approval" {
        return Err(AppError::blocked(format!(
            "patch verify 차단\n- 이유: verification approval을 받을 수 없는 phase입니다.\n- phase: {}",
            workflow.phase
        )));
    }
    let plan = build_verification_plan(&record.verification_command)?;
    workflow =
        approve_prepared_verification_transaction(&record, workflow, intent_id, expected_lease)?;
    verification_approval_transaction_fault("after-commit")?;
    continue_approved_workflow(record, Some(workflow), Some(plan))
}

fn approve_prepared_verification_transaction(
    record: &ProposalRecord,
    observed_workflow: state::WorkflowRecord,
    intent_id: &str,
    expected_lease: Option<&crate::runtime::SelectionLease>,
) -> Result<state::WorkflowRecord, AppError> {
    let identity = ledger::validated_current_identity()?;
    let current_lease = state::current_state_lease_view()?;
    let observed_ledger = ledger::validated_ledger_binding()?;
    validate_applied_proposal(record)?;

    let transition_guard = crate::transition::TransitionGuard::acquire_for(
        &identity.project_id,
        crate::transition::CurrentStateIntent::ApproveVerification,
    )?;
    if let Some(lease) = expected_lease {
        if !crate::runtime::tui_lease_matches_workflow_under_transition(
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
            "prepared verification workflow가 lock 획득 전에 변경되었습니다.",
        ));
    }
    validate_workflow_binding(&current, record)?;
    if current.phase != "pending-verification-approval"
        || !matches!(
            current.verification_approval_state.as_str(),
            "pending" | "pending-rotated"
        )
    {
        return Err(AppError::blocked(
            "prepared verification approval gate 상태 불일치",
        ));
    }
    let runtime = workflow_skill_runtime(&current)?.ok_or_else(|| {
        AppError::blocked(
            "prepared verification은 registered built-in skill workflow가 필요합니다.",
        )
    })?;
    if runtime.state != crate::skill::SkillState::AwaitingVerification {
        return Err(AppError::blocked(format!(
            "verification side effect 차단\n- skill state: {}\n- expected skill state: awaiting-verification",
            runtime.state.label()
        )));
    }
    validate_skill_verification(&runtime.active_skill_id, &record.verification_command)?;
    validate_failing_test_before(&current, &runtime)?;
    if state::current_state_lease_view_under_transition()? != current_lease {
        return Err(AppError::blocked(
            "prepared verification current-state lease가 lock 획득 전에 변경되었습니다.",
        ));
    }

    let writer = ledger::LedgerWriterGuard::acquire()?;
    let ledger_binding = writer.binding()?;
    if ledger_binding != observed_ledger {
        return Err(AppError::blocked(
            "prepared verification ledger head가 lock 획득 전에 변경되었습니다.",
        ));
    }

    let mut started = current.clone();
    started.phase = "verification-started".to_string();
    started.verification_approval_state = "approved".to_string();
    runtime.store_in_workflow(&mut started);
    let r1 = workflow_guard.prepare_revision(&current, started)?;
    let e0 = ledger::new_event_for(
        &identity,
        "runtime.intent.accepted",
        "interactive runtime intent accepted",
        &format!(
            "intent_id={intent_id} intent_kind=approve-verification workflow_id={} proposal_id={}",
            current.workflow_id, record.proposal_id
        ),
    );
    let e2 = ledger::new_event_for(
        &identity,
        "patch.verification.approved",
        "verification command approval durably accepted",
        &format!(
            "intent_id={intent_id} workflow_id={} proposal_id={} gate=verification-command revision={} artifact_hash={} command_hash={}",
            r1.record.workflow_id,
            record.proposal_id,
            r1.record.revision,
            r1.record.artifact_hash,
            sha256_text(&record.verification_command),
        ),
    );
    let semantic_events = vec![e0, r1.event.clone(), e2];
    let planned = writer.plan_events(&semantic_events)?;
    let final_binding = ledger::LedgerBinding {
        event_count: planned[2].ordinal,
        event_id: Some(planned[2].event.event_id.clone()),
        event_hash: planned[2].event_hash.clone(),
    };
    let current_image =
        state::prepare_current_image_after(&r1.record, current.revision, &final_binding)?;
    let mut bundle = crate::transition::prepare_workflow_bundle_with_context(
        intent_id,
        "approve-verification",
        &current.workflow_id,
        crate::transition::PreparedBundleContext {
            identity: &identity,
            lease: &current_lease,
            ledger_binding,
        },
    )?;
    crate::transition::bind_planned_events(&mut bundle, &planned)?;
    crate::transition::bind_additional_members(
        &mut bundle,
        prepared_verification_members(&r1, &current_image, &semantic_events),
    )?;
    state::transition_project_current_state_prepared_verification(
        state::PreparedVerificationTransition {
            transition_guard: Some(&transition_guard),
            workflow_guard: &workflow_guard,
            writer: &writer,
            planned: &planned,
            bundle: &bundle,
            revision: &r1,
            current: &current_image,
            events: &semantic_events,
        },
    )?;
    Ok(r1.record)
}

fn prepared_verification_members(
    revision: &state::PreparedWorkflowRevision,
    current: &state::PreparedCurrentImage,
    events: &[ledger::LedgerEvent],
) -> Vec<crate::transition::PreparedMember> {
    use crate::transition::{PreparedMember, PreparedMemberBinding, PreparedMemberKind};
    let member = |kind,
                  path: String,
                  schema_version,
                  artifact_id: String,
                  causal_id: Option<String>,
                  event_id: String,
                  bytes_utf8: String,
                  expected_type: &str| PreparedMember {
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
        expected_identity: None,
        readonly: false,
        mode: 0o600,
        ownership: None,
        semantic_role_rank: 0,
    };
    vec![
        member(
            PreparedMemberKind::WorkflowSnapshot,
            revision.snapshot_stored_path.clone(),
            4,
            revision.snapshot_member_id.clone(),
            None,
            events[1].event_id.clone(),
            revision.snapshot_bytes.clone(),
            "absent",
        ),
        member(
            PreparedMemberKind::WorkflowPointer,
            revision.pointer_stored_path.clone(),
            4,
            revision.pointer_member_id.clone(),
            Some(revision.snapshot_member_id.clone()),
            events[1].event_id.clone(),
            revision.pointer_bytes.clone(),
            "file",
        ),
        member(
            PreparedMemberKind::CurrentImage,
            current.stored_path.clone(),
            2,
            current.artifact_id.clone(),
            Some(revision.snapshot_member_id.clone()),
            events[2].event_id.clone(),
            current.bytes.clone(),
            "file",
        ),
    ]
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
        .is_some_and(|runtime| runtime.state == crate::skill::SkillState::AwaitingApproval);
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
                        let _ = runtime.transition(crate::skill::SkillState::Failed);
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
        runtime.transition(crate::skill::SkillState::AwaitingVerification)?;
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
                    let _ = runtime.transition(crate::skill::SkillState::Failed);
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
        crate::transcript::record_workflow_turn(
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
                "fix-test" if verification_plan.as_ref().is_some_and(is_test_verification) => {
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

fn workflow_skill_runtime(
    workflow: &state::WorkflowRecord,
) -> Result<Option<crate::skill::SkillRuntimeState>, AppError> {
    if workflow.active_skill_id.is_empty() {
        return Ok(None);
    }
    crate::skill::SkillRuntimeState::from_workflow(workflow).map(Some)
}

fn validate_skill_phase_for_side_effect(
    workflow: &state::WorkflowRecord,
    runtime: &crate::skill::SkillRuntimeState,
) -> Result<(), AppError> {
    let expected = match workflow.phase.as_str() {
        "approved" => crate::skill::SkillState::AwaitingApproval,
        "verification-started" => crate::skill::SkillState::AwaitingVerification,
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

fn validate_failing_test_before(
    workflow: &state::WorkflowRecord,
    runtime: &crate::skill::SkillRuntimeState,
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

fn validate_completed_workflow(workflow: &state::WorkflowRecord) -> Result<(), AppError> {
    if workflow.phase != "complete" {
        return Err(AppError::blocked(
            "workflow complete 검증 차단\n- 이유: complete phase가 아닙니다.",
        ));
    }
    if let Some(runtime) = workflow_skill_runtime(workflow)? {
        if runtime.state != crate::skill::SkillState::Complete {
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

fn validate_completed_plugin_workflow(
    workflow: &state::WorkflowRecord,
) -> Result<crate::skill::ImportedSkillManifest, AppError> {
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
    let imported = crate::plugin::revalidate_completed_codex_skill(
        &workflow.active_skill_id,
        &workflow.source_path,
        &workflow.source_hash,
    )?;
    let resolved = crate::skill::ResolvedSkillManifest::Imported(imported.clone());
    let runtime = crate::skill::SkillRuntimeState::from_workflow_against(workflow, &resolved)?;
    if runtime.state != crate::skill::SkillState::Complete {
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
    imported: &crate::skill::ImportedSkillManifest,
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
    imported: &crate::skill::ImportedSkillManifest,
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

fn ensure_plugin_completion_event(
    workflow: &state::WorkflowRecord,
    imported: &crate::skill::ImportedSkillManifest,
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

fn ensure_plugin_completion_event_under_transition(
    transition_guard: &crate::transition::TransitionGuard,
    workflow: &state::WorkflowRecord,
    imported: &crate::skill::ImportedSkillManifest,
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

fn plugin_completion_recovery_report(workflow: &state::WorkflowRecord) -> String {
    format!(
        "plugin capability 복구 완료\n- 결과: 성공\n- workflow id: {}\n- skill id: {}\n- source: {}@{}\n- side effect: 없음\n- completion event: 확인됨\n- active pointer: 정리됨",
        workflow.workflow_id,
        workflow.active_skill_id,
        workflow.source_path,
        workflow.source_hash
    )
}

fn is_test_verification(plan: &VerificationPlan) -> bool {
    matches!(plan.argv.as_slice(), [cargo, test, ..] if cargo == "cargo" && test == "test")
}

fn dispatch_workflow_skill_hook(
    workflow: &state::WorkflowRecord,
    runtime: &mut crate::skill::SkillRuntimeState,
    hook: &str,
    tool: &str,
) -> Result<(), AppError> {
    crate::hooks::dispatch_native_lifecycle(
        crate::hooks::HookInput {
            hook,
            workflow_id: Some(&workflow.workflow_id),
            active_skill_id: Some(&runtime.active_skill_id),
            mode: crate::skill::find_skill(&runtime.active_skill_id)
                .map(|manifest| manifest.mode)
                .unwrap_or("unknown"),
            payload: tool,
        },
        matches!(hook, "pre_tool_call" | "post_tool_result").then_some(tool),
    )?;
    runtime.record_hook(hook)
}

fn finalize_verified_skill(
    workflow: &mut state::WorkflowRecord,
    runtime: Option<&mut crate::skill::SkillRuntimeState>,
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
    runtime.transition(crate::skill::SkillState::StopPassed)?;
    runtime.transition(crate::skill::SkillState::Complete)?;
    runtime.store_in_workflow(workflow);
    Ok(())
}

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
            crate::evidence::validate_patch_stop_gate(&workflow)
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
        "verification-started" => Err(AppError::blocked(
            "workflow resume preflight 차단\n- 이유: verification 결과가 확정되지 않아 session을 선택할 수 없습니다.",
        )),
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
                let _ = runtime.transition(crate::skill::SkillState::Failed);
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
    lease: &crate::runtime::SelectionLease,
) -> Result<(), AppError> {
    validate_outcome_id(workflow_id, "workflow")?;
    validate_outcome_id(intent_id, "intent")?;
    let (observed, _approval_lock) = load_workflow_under_approval_lock(workflow_id)?;
    let transition_guard = crate::transition::TransitionGuard::acquire_for(
        &lease.project_id,
        crate::transition::CurrentStateIntent::Resume,
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
    if !crate::runtime::tui_lease_matches_workflow_under_transition(lease, workflow_id)? {
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
            crate::evidence::evaluate_patch_stop_gate(&workflow)?;
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

pub fn cancel_workflow_report(workflow_id: &str) -> Result<String, AppError> {
    let intent_id = format!("intent-cancel-{}", workflow_id);
    let workflow = cancel_workflow_transaction(workflow_id, &intent_id, None).map_err(|error| {
        if let Some(reason) = error.message.strip_prefix("internal.rollback-conflict:") {
            AppError::blocked(format!(
                "workflow cancel 차단\n- 이유: 적용된 source를 안전하게 복원하지 못했습니다.\n- rollback: {reason}\n- pointer: 유지"
            ))
        } else if let Some(phase) = error.message.strip_prefix("internal.cancel-terminal:") {
            AppError::blocked(format!(
                "cancel 차단\n- 이유: terminal workflow는 취소할 수 없습니다.\n- phase: {phase}"
            ))
        } else {
            error
        }
    })?;
    Ok(format!(
        "workflow 취소 완료\n- workflow id: {}\n- phase: cancelled\n- source 복원: 검증됨 또는 적용 전\n- backend/verification 재실행: 없음",
        workflow.workflow_id
    ))
}

pub(crate) fn cancel_workflow_for_tui(
    workflow_id: &str,
    intent_id: &str,
    lease: &crate::runtime::SelectionLease,
) -> Result<(), AppError> {
    cancel_workflow_transaction(workflow_id, intent_id, Some(lease)).map(|_| ())
}

fn cancel_workflow_transaction(
    workflow_id: &str,
    intent_id: &str,
    expected_lease: Option<&crate::runtime::SelectionLease>,
) -> Result<state::WorkflowRecord, AppError> {
    validate_outcome_id(intent_id, "intent")?;
    let (observed, _approval_lock) = load_workflow_under_approval_lock(workflow_id)?;
    if observed.phase == "complete" {
        return Err(AppError::blocked(format!(
            "internal.cancel-terminal:{}",
            observed.phase
        )));
    }
    if matches!(observed.phase.as_str(), "failed" | "cancelled") {
        return Err(AppError::blocked(format!(
            "internal.cancel-terminal:{}",
            observed.phase
        )));
    }
    let identity = ledger::validated_current_identity()?;
    let transition_guard = crate::transition::TransitionGuard::acquire_for(
        &identity.project_id,
        crate::transition::CurrentStateIntent::Cancel,
    )?;
    if let Some(lease) = expected_lease {
        if !crate::runtime::tui_lease_matches_workflow_under_transition(lease, workflow_id)? {
            return Err(stale_selection_error());
        }
    }
    let workflow_guard = state::WorkflowCheckpointGuard::acquire(workflow_id)?;
    let current = workflow_guard.load_current()?;
    if current != observed {
        return Err(stale_selection_error());
    }
    let source = if workflow_has_applied_source(&current) {
        let record = load_bound_proposal(&current)?;
        prepare_terminal_rollback_source(&record, intent_id, false)?
    } else {
        None
    };
    let mut terminal = current.clone();
    terminal.phase = "cancelled".to_string();
    terminal.failure_reason = "user-cancelled".to_string();
    terminal.approval_state = "cancelled".to_string();
    terminal.verification_approval_state = "cancelled".to_string();
    if let Some(mut runtime) = workflow_skill_runtime(&terminal)? {
        runtime.transition(crate::skill::SkillState::Cancelled)?;
        runtime.store_in_workflow(&mut terminal);
    }
    state::transition_project_current_state_prepared_terminal_action(
        &transition_guard,
        &workflow_guard,
        state::TerminalActionRequest {
            intent_id,
            intent_kind: "cancel-workflow",
            identity: &identity,
            before: &current,
            terminal,
            audit_event_type: "workflow.user-cancelled",
            audit_summary: "workflow cancelled by user",
            audit_details: "reason=user-cancelled",
            source,
        },
    )
}

#[cfg(test)]
pub fn deny_pending_gate(workflow_id: &str, intent_id: &str) -> Result<TuiOutcome, AppError> {
    deny_pending_gate_transaction(workflow_id, intent_id, None)
}

pub(crate) fn deny_pending_gate_for_tui(
    workflow_id: &str,
    intent_id: &str,
    gate_id: &str,
    gate_kind: crate::runtime::TuiGateKind,
    lease: &crate::runtime::SelectionLease,
) -> Result<TuiOutcome, AppError> {
    deny_pending_gate_transaction(workflow_id, intent_id, Some((gate_id, gate_kind, lease)))
}

fn deny_pending_gate_transaction(
    workflow_id: &str,
    intent_id: &str,
    expected: Option<(
        &str,
        crate::runtime::TuiGateKind,
        &crate::runtime::SelectionLease,
    )>,
) -> Result<TuiOutcome, AppError> {
    validate_outcome_id(intent_id, "intent")?;
    let (observed, _approval_lock) = load_workflow_under_approval_lock(workflow_id)?;
    validate_outcome_id(&observed.workflow_id, "workflow")?;
    if observed.phase == "cancelled"
        && observed.failure_reason == "user-denied-patch"
        && terminal_action_receipt_exists(intent_id, workflow_id, "patch.apply.denied")?
    {
        validate_stored_terminal_gate(
            &observed,
            expected,
            crate::runtime::TuiGateKind::PatchApply,
        )?;
        return deny_patch_accepted(intent_id, &observed.workflow_id);
    }
    if observed.phase == "cancelled"
        && observed.failure_reason == "user-denied-verification"
        && terminal_action_receipt_exists(intent_id, workflow_id, "patch.verification.denied")?
    {
        validate_stored_terminal_gate(
            &observed,
            expected,
            crate::runtime::TuiGateKind::VerificationCommand,
        )?;
        return deny_verification_accepted(intent_id, &observed.workflow_id);
    }
    let identity = ledger::validated_current_identity()?;
    let transition_guard = crate::transition::TransitionGuard::acquire_for(
        &identity.project_id,
        crate::transition::CurrentStateIntent::Cancel,
    )?;
    let workflow_guard = state::WorkflowCheckpointGuard::acquire(workflow_id)?;
    let workflow = workflow_guard.load_current()?;
    if workflow != observed {
        return Err(stale_selection_error());
    }
    if workflow.is_terminal() {
        if let Some((gate_id, gate_kind, lease)) = expected {
            if !crate::runtime::tui_lease_matches_terminal_selection_under_transition(
                lease,
                workflow_id,
            )? {
                return Err(stale_selection_error());
            }
            validate_terminal_gate(&workflow, gate_id, gate_kind)?;
        }
        return exact_tui_outcome(
            TuiOutcomeCode::DenyBlockedTerminalState,
            TuiOutcomeContext {
                intent_id: Some(intent_id),
                workflow_id: Some(&workflow.workflow_id),
                phase: Some(&workflow.phase),
                ..TuiOutcomeContext::default()
            },
        );
    }
    if let Some((_, _, lease)) = expected {
        if !crate::runtime::tui_lease_matches_workflow_under_transition(lease, workflow_id)? {
            return Err(stale_selection_error());
        }
    }
    if let Some((gate_id, gate_kind, _)) = expected {
        validate_terminal_gate(&workflow, gate_id, gate_kind)?;
    }
    match denial_phase_outcome_code(&workflow.phase) {
        Some(TuiOutcomeCode::DenyPatchAccepted) => {
            let mut terminal = workflow.clone();
            terminal.phase = "cancelled".to_string();
            terminal.failure_reason = "user-denied-patch".to_string();
            terminal.approval_state = "denied".to_string();
            terminal.verification_approval_state = "not-issued".to_string();
            if let Some(mut skill_runtime) = workflow_skill_runtime(&terminal)? {
                skill_runtime.transition(crate::skill::SkillState::Cancelled)?;
                skill_runtime.store_in_workflow(&mut terminal);
            }
            let committed = state::transition_project_current_state_prepared_terminal_action(
                &transition_guard,
                &workflow_guard,
                state::TerminalActionRequest {
                    intent_id,
                    intent_kind: "deny-patch",
                    identity: &identity,
                    before: &workflow,
                    terminal,
                    audit_event_type: "patch.apply.denied",
                    audit_summary: "patch apply approval denied",
                    audit_details: "gate=patch-apply effect=none",
                    source: None,
                },
            )?;
            deny_patch_accepted(intent_id, &committed.workflow_id)
        }
        Some(TuiOutcomeCode::DenyVerificationRolledBack) => {
            let record = load_bound_proposal(&workflow)?;
            let source = match prepare_terminal_rollback_source(&record, intent_id, true) {
                Ok(Some(source)) => source,
                Ok(None) => {
                    return Err(AppError::blocked(
                        "prepared verification denial rollback receipt 누락",
                    ))
                }
                Err(error) if error.message.starts_with("internal.rollback-conflict:") => {
                    return exact_tui_outcome(
                        TuiOutcomeCode::RollbackConflict,
                        TuiOutcomeContext {
                            intent_id: Some(intent_id),
                            workflow_id: Some(&workflow.workflow_id),
                            ..TuiOutcomeContext::default()
                        },
                    )
                }
                Err(error) => return Err(error),
            };
            let mut terminal = workflow.clone();
            terminal.phase = "cancelled".to_string();
            terminal.failure_reason = "user-denied-verification".to_string();
            terminal.approval_state = "applied-then-rolled-back".to_string();
            terminal.verification_approval_state = "denied".to_string();
            if let Some(mut skill_runtime) = workflow_skill_runtime(&terminal)? {
                skill_runtime.transition(crate::skill::SkillState::Cancelled)?;
                skill_runtime.store_in_workflow(&mut terminal);
            }
            let committed = state::transition_project_current_state_prepared_terminal_action(
                &transition_guard,
                &workflow_guard,
                state::TerminalActionRequest {
                    intent_id,
                    intent_kind: "deny-verification",
                    identity: &identity,
                    before: &workflow,
                    terminal,
                    audit_event_type: "patch.verification.denied",
                    audit_summary: "verification approval denied and source rolled back",
                    audit_details: "gate=verification-command rollback=restored",
                    source: Some(source),
                },
            )?;
            deny_verification_accepted(intent_id, &committed.workflow_id)
        }
        Some(TuiOutcomeCode::DenyBlockedNotPending) => {
            exact_tui_outcome(
                TuiOutcomeCode::DenyBlockedNotPending,
                TuiOutcomeContext {
                    intent_id: Some(intent_id),
                    workflow_id: Some(&workflow.workflow_id),
                    phase: Some(&workflow.phase),
                    ..TuiOutcomeContext::default()
                },
            )
        }
        Some(TuiOutcomeCode::DenyBlockedTerminalState) => exact_tui_outcome(
            TuiOutcomeCode::DenyBlockedTerminalState,
            TuiOutcomeContext {
                intent_id: Some(intent_id),
                workflow_id: Some(&workflow.workflow_id),
                phase: Some(&workflow.phase),
                ..TuiOutcomeContext::default()
            },
        ),
        Some(other) => Err(AppError::blocked(format!(
            "승인 거부 차단\n- code: deny.corrupt-state\n- mapped outcome: {}\n- 동작: 허용되지 않은 denial outcome을 실행하지 않았습니다.",
            other.as_str()
        ))),
        None => Err(AppError::blocked(
            "승인 거부 차단\n- code: deny.corrupt-state\n- 동작: 알 수 없는 workflow phase를 출력하거나 변경하지 않았습니다.",
        )),
    }
}

pub(crate) fn denial_phase_outcome_code(phase: &str) -> Option<TuiOutcomeCode> {
    match phase {
        "pending-approval" => Some(TuiOutcomeCode::DenyPatchAccepted),
        "pending-verification-approval" => Some(TuiOutcomeCode::DenyVerificationRolledBack),
        "approved" | "verification-approved" | "verification-started" | "verified" => {
            Some(TuiOutcomeCode::DenyBlockedNotPending)
        }
        "complete" | "failed" | "cancelled" => Some(TuiOutcomeCode::DenyBlockedTerminalState),
        _ => None,
    }
}

fn deny_patch_accepted(intent_id: &str, workflow_id: &str) -> Result<TuiOutcome, AppError> {
    exact_tui_outcome(
        TuiOutcomeCode::DenyPatchAccepted,
        TuiOutcomeContext {
            intent_id: Some(intent_id),
            workflow_id: Some(workflow_id),
            ..TuiOutcomeContext::default()
        },
    )
}

fn deny_verification_accepted(intent_id: &str, workflow_id: &str) -> Result<TuiOutcome, AppError> {
    exact_tui_outcome(
        TuiOutcomeCode::DenyVerificationRolledBack,
        TuiOutcomeContext {
            intent_id: Some(intent_id),
            workflow_id: Some(workflow_id),
            ..TuiOutcomeContext::default()
        },
    )
}

fn workflow_has_applied_source(workflow: &state::WorkflowRecord) -> bool {
    matches!(
        workflow.phase.as_str(),
        "approved"
            | "pending-verification-approval"
            | "verification-approved"
            | "verification-started"
            | "verified"
    ) || matches!(
        workflow.approval_state.as_str(),
        "applied" | "approved" | "applied-then-rolled-back"
    )
}

fn load_bound_proposal(workflow: &state::WorkflowRecord) -> Result<ProposalRecord, AppError> {
    let proposal_path =
        paths::project_patch_proposals_dir().join(format!("{}.txt", workflow.proposal_id));
    let record = load_proposal_record(&workflow.proposal_id, &proposal_path)?;
    validate_workflow_binding(workflow, &record)?;
    Ok(record)
}

fn prepare_terminal_rollback_source(
    record: &ProposalRecord,
    intent_id: &str,
    require_receipt: bool,
) -> Result<Option<state::PreparedTerminalSource>, AppError> {
    let target = resolve_target_for("terminal rollback", &record.relative_path)?;
    let current = fs::read(&target.absolute_path)
        .map_err(|err| AppError::blocked(format!("terminal rollback target read 실패: {err}")))?;
    let current_hash = sha256_bytes(&current);
    if current_hash != record.proposed_sha256 && current_hash != record.original_sha256 {
        return Err(AppError::blocked(format!(
            "internal.rollback-conflict:target-sha256={current_hash}"
        )));
    }
    if current_hash == record.original_sha256 && !require_receipt {
        return Ok(None);
    }
    let rollback_path = rollback_path_for_record(record)?;
    let original = fs::read(&rollback_path)
        .map_err(|err| AppError::blocked(format!("terminal rollback record read 실패: {err}")))?;
    if sha256_bytes(&original) != record.original_sha256 {
        return Err(AppError::blocked(
            "internal.rollback-conflict:rollback-record-hash",
        ));
    }
    let plan = crate::transition::prepare_source_install_v1(
        intent_id,
        &record.proposal_id,
        &target.absolute_path,
        &current,
        &original,
    )?;
    Ok(Some(state::PreparedTerminalSource {
        plan,
        before: current,
        proposed: original,
    }))
}

fn validate_terminal_gate(
    workflow: &state::WorkflowRecord,
    gate_id: &str,
    gate_kind: crate::runtime::TuiGateKind,
) -> Result<(), AppError> {
    validate_outcome_id(gate_id, "gate")?;
    let expected_kind = match (workflow.phase.as_str(), workflow.failure_reason.as_str()) {
        ("cancelled", "user-denied-patch") => crate::runtime::TuiGateKind::PatchApply,
        ("cancelled", "user-denied-verification") => {
            crate::runtime::TuiGateKind::VerificationCommand
        }
        ("pending-approval" | "approved", _) => crate::runtime::TuiGateKind::PatchApply,
        (
            "pending-verification-approval"
            | "verification-approved"
            | "verification-started"
            | "verified",
            _,
        ) => crate::runtime::TuiGateKind::VerificationCommand,
        _ if matches!(
            workflow.approval_state.as_str(),
            "pending" | "pending-rotated"
        ) =>
        {
            crate::runtime::TuiGateKind::PatchApply
        }
        _ => crate::runtime::TuiGateKind::VerificationCommand,
    };
    if gate_id != workflow.proposal_id || gate_kind != expected_kind {
        return Err(stale_selection_error());
    }
    Ok(())
}

fn validate_stored_terminal_gate(
    workflow: &state::WorkflowRecord,
    expected: Option<(
        &str,
        crate::runtime::TuiGateKind,
        &crate::runtime::SelectionLease,
    )>,
    expected_kind: crate::runtime::TuiGateKind,
) -> Result<(), AppError> {
    if let Some((gate_id, gate_kind, lease)) = expected {
        if gate_id != workflow.proposal_id
            || gate_kind != expected_kind
            || lease.selected_object_id != workflow.workflow_id
        {
            return Err(stale_selection_error());
        }
    }
    Ok(())
}

fn terminal_action_receipt_exists(
    intent_id: &str,
    workflow_id: &str,
    event_type: &str,
) -> Result<bool, AppError> {
    ledger::event_details_match(
        event_type,
        &[("intent_id", intent_id), ("workflow_id", workflow_id)],
    )
}

fn validate_outcome_id(value: &str, kind: &str) -> Result<(), AppError> {
    let valid = !value.is_empty()
        && value.len() <= 96
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.')
        });
    if valid {
        Ok(())
    } else {
        Err(AppError::blocked(format!(
            "결과 식별자 검증 차단\n- kind: {kind}\n- 동작: 신뢰할 수 없는 식별자를 출력하지 않았습니다."
        )))
    }
}

const STALE_SELECTION_ERROR: &str = "internal.tui-selection-stale-under-action-lock";

fn stale_selection_error() -> AppError {
    AppError::blocked(STALE_SELECTION_ERROR)
}

pub(crate) fn is_stale_selection_error(error: &AppError) -> bool {
    error.code == 3 && error.message == STALE_SELECTION_ERROR
}

fn validate_workflow_binding(
    workflow: &state::WorkflowRecord,
    record: &ProposalRecord,
) -> Result<(), AppError> {
    if workflow.workflow_id != record.workflow_id
        || workflow.action_id != record.action_id
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

pub(crate) fn proposal_detail_for_workflow_bounded(
    workflow: &state::WorkflowRecord,
    proposal_id: &str,
    max_bytes: usize,
) -> Result<PatchProposalDetail, AppError> {
    if workflow.proposal_id != proposal_id {
        return Err(stale_selection_error());
    }
    validate_proposal_id(proposal_id)?;
    let proposal_path = paths::project_patch_proposals_dir().join(format!("{proposal_id}.txt"));
    let contents = read_proposal_contents_bounded(proposal_id, &proposal_path, max_bytes)?;
    let record = parse_proposal_record_contents(proposal_id, &proposal_path, &contents, false)?;
    validate_workflow_binding(workflow, &record)?;
    let (header, diff) = parse_proposal_header(&contents, &proposal_path)?;
    Ok(PatchProposalDetail {
        summary: summary_from_header(&proposal_path, &header)?,
        diff: diff.trim_end().to_string(),
    })
}

fn read_proposal_contents_bounded(
    proposal_id: &str,
    proposal_path: &Path,
    max_bytes: usize,
) -> Result<String, AppError> {
    let metadata = fs::symlink_metadata(proposal_path).map_err(|err| {
        AppError::blocked(format!(
            "patch proposal read 차단\n- 이유: proposal metadata를 읽지 못했습니다.\n- proposal id: {}\n- path: {}\n- error: {}",
            proposal_id,
            proposal_path.display(),
            err
        ))
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AppError::blocked(
            "patch proposal regular-file boundary 불일치",
        ));
    }
    if metadata.len() > u64::try_from(max_bytes).unwrap_or(u64::MAX) {
        return Err(AppError::blocked("patch proposal byte budget 초과"));
    }
    let mut file = File::open(proposal_path).map_err(|err| {
        AppError::blocked(format!(
            "patch proposal read 차단\n- 이유: proposal record를 읽지 못했습니다.\n- proposal id: {}\n- path: {}\n- error: {}",
            proposal_id,
            proposal_path.display(),
            err
        ))
    })?;
    let mut bytes = Vec::with_capacity(
        usize::try_from(metadata.len())
            .unwrap_or(max_bytes)
            .min(max_bytes),
    );
    file.by_ref()
        .take(u64::try_from(max_bytes.saturating_add(1)).unwrap_or(u64::MAX))
        .read_to_end(&mut bytes)
        .map_err(|err| AppError::blocked(format!("patch proposal bounded read 실패: {err}")))?;
    if bytes.len() > max_bytes {
        return Err(AppError::blocked("patch proposal byte budget 초과"));
    }
    String::from_utf8(bytes).map_err(|_| AppError::blocked("patch proposal UTF-8 불일치"))
}

#[cfg(test)]
fn summary_from_path(path: &Path) -> Result<PatchProposalSummary, AppError> {
    let metadata = fs::symlink_metadata(path).map_err(|err| {
        AppError::blocked(format!(
            "patch proposal summary metadata를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > 2 * 1024 * 1024
    {
        return Err(AppError::blocked(
            "patch proposal summary regular-file/byte budget 불일치",
        ));
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|err| {
            AppError::runtime(format!(
                "patch proposal record를 열지 못했습니다: {} ({err})",
                path.display()
            ))
        })?
        .take(64 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|err| AppError::blocked(format!("patch proposal header read 실패: {err}")))?;
    let prefix = String::from_utf8(bytes)
        .map_err(|_| AppError::blocked("patch proposal header UTF-8 불일치"))?;
    let end = prefix.find("\n\n").ok_or_else(|| {
        AppError::blocked("patch proposal header가 64KiB read budget을 초과했습니다.")
    })?;
    summary_from_record(path, &prefix[..end + 2])
}

#[cfg(test)]
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
    let rollback_dir = paths::project_state_dir().join("patches").join(proposal_id);
    let applied = fs::read_dir(rollback_dir).ok().is_some_and(|entries| {
        entries.filter_map(Result::ok).any(|entry| {
            entry.path().extension().and_then(|value| value.to_str()) == Some("rollback")
        })
    });
    if applied {
        "applied".to_string()
    } else {
        "pending-approval".to_string()
    }
}

fn rollback_path_for_record(record: &ProposalRecord) -> Result<PathBuf, AppError> {
    let target = resolve_target_for("patch rollback path", &record.relative_path)?;
    let legacy = crate::transition::source_install_rollback_path(
        &format!("intent-source-{}", record.proposal_id),
        &record.proposal_id,
        &target.absolute_path,
        &record.original_sha256,
        &record.proposed_sha256,
    )?;
    if legacy.is_file() {
        return Ok(legacy);
    }

    let directory = paths::project_state_dir()
        .join("patches")
        .join(&record.proposal_id);
    let mut candidates = fs::read_dir(&directory)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter_map(|entry| {
                    let path = entry.path();
                    let metadata = fs::symlink_metadata(&path).ok()?;
                    (metadata.file_type().is_file()
                        && !metadata.file_type().is_symlink()
                        && path.extension().and_then(|value| value.to_str()) == Some("rollback"))
                    .then_some(path)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    candidates.sort();
    if let Some(valid) = candidates.iter().find(|path| {
        fs::read(path)
            .map(|bytes| sha256_bytes(&bytes) == record.original_sha256)
            .unwrap_or(false)
    }) {
        return Ok(valid.clone());
    }
    Ok(candidates.into_iter().next().unwrap_or(legacy))
}

fn validate_applied_proposal(record: &ProposalRecord) -> Result<ApplyResult, AppError> {
    let target = resolve_target_for("patch verification", &record.relative_path)?;
    let current = fs::read(&target.absolute_path).map_err(|err| {
        AppError::blocked(format!(
            "patch verification source reread 실패: {} ({err})",
            target.relative_path
        ))
    })?;
    let current_sha256 = sha256_bytes(&current);
    if current_sha256 != record.proposed_sha256 {
        return Err(AppError::blocked(format!(
            "patch verification 차단\n- 이유: 적용된 source hash가 proposal과 일치하지 않습니다.\n- path: {}\n- expected proposed sha256: {}\n- current sha256: {}",
            target.relative_path, record.proposed_sha256, current_sha256
        )));
    }
    let rollback_path = rollback_path_for_record(record)?;
    let rollback = fs::read(&rollback_path).map_err(|err| {
        AppError::blocked(format!(
            "patch verification 차단\n- 이유: rollback record를 읽지 못했습니다.\n- path: {}\n- error: {err}",
            rollback_path.display()
        ))
    })?;
    if sha256_bytes(&rollback) != record.original_sha256 {
        return Err(AppError::blocked(
            "patch verification 차단\n- 이유: rollback record hash가 original hash와 일치하지 않습니다.",
        ));
    }
    Ok(ApplyResult {
        relative_path: target.relative_path,
        original_sha256: record.original_sha256.clone(),
        applied_sha256: current_sha256,
        rollback_path,
    })
}

fn load_proposal_record(
    proposal_id: &str,
    proposal_path: &Path,
) -> Result<ProposalRecord, AppError> {
    let contents =
        read_proposal_contents_bounded(proposal_id, proposal_path, MAX_PROPOSAL_RECORD_BYTES)?;
    parse_proposal_record_contents(proposal_id, proposal_path, &contents, true)
}

fn parse_proposal_record_contents(
    proposal_id: &str,
    proposal_path: &Path,
    contents: &str,
    allow_legacy_migration: bool,
) -> Result<ProposalRecord, AppError> {
    match proposal_domain::parse_record(
        proposal_id,
        proposal_path,
        contents,
        allow_legacy_migration,
    )? {
        RecordParse::Canonical(record) => Ok(record),
        RecordParse::LegacyMigration { scrubbed } => {
            state::atomic_replace_bytes(proposal_path, scrubbed.as_bytes())?;
            Err(AppError::blocked(
                "legacy proposal migration 완료\n- plaintext token을 hash-only로 atomic scrub했습니다.\n- 동작: 기존 binding은 폐기하고 canonical workflow preview를 다시 생성하세요.",
            ))
        }
    }
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

    let mut current = fs::read_to_string(&target.absolute_path).map_err(|err| {
        AppError::runtime(format!(
            "patch approve 대상 파일을 UTF-8 text로 읽지 못했습니다: {} ({err})",
            target.relative_path
        ))
    })?;
    let source_intent_id = format!("intent-source-{}", record.proposal_id);
    let identity = ledger::validated_current_identity()?;
    let pending_journal =
        paths::project_transition_journal_file(&identity.project_id, &source_intent_id);
    if pending_journal.exists() {
        let body = fs::read_to_string(&pending_journal).map_err(|err| {
            AppError::blocked(format!("prepared source journal 읽기 실패: {err}"))
        })?;
        let bundle = crate::transition::parse_prepared_source_bundle(&body)?;
        let source_install = bundle
            .source_install
            .as_ref()
            .ok_or_else(|| AppError::blocked("prepared source journal source_install_v1 누락"))?;
        if bundle.intent_id != source_intent_id
            || source_install.before_sha256 != record.original_sha256
            || source_install.proposed_sha256 != record.proposed_sha256
        {
            return Err(AppError::blocked(
                "prepared source journal proposal binding 불일치",
            ));
        }
        state::install_prepared_source_bundle(&bundle, &pending_journal)?;
        crate::transition::remove_committed_source_bundle(&bundle, &pending_journal)?;
        current = fs::read_to_string(&target.absolute_path).map_err(|err| {
            AppError::blocked(format!("recovered source target 읽기 실패: {err}"))
        })?;
    }
    let current_sha256 = sha256_text(&current);
    let rollback_path = crate::transition::source_install_rollback_path(
        &source_intent_id,
        &record.proposal_id,
        &target.absolute_path,
        &record.original_sha256,
        &record.proposed_sha256,
    )?;
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

    let source_plan = crate::transition::prepare_source_install_v1(
        &source_intent_id,
        &record.proposal_id,
        &target.absolute_path,
        current.as_bytes(),
        record.proposed_content.as_bytes(),
    )?;
    let bundle = crate::transition::prepare_source_bundle(
        &source_intent_id,
        (!record.workflow_id.is_empty()).then_some(record.workflow_id.as_str()),
        source_plan,
        current.as_bytes(),
        record.proposed_content.as_bytes(),
    )?;
    let journal_path = crate::transition::commit_prepared_source_bundle(&bundle)?;
    if let Err(err) = state::install_prepared_source_bundle(&bundle, &journal_path) {
        return Err(AppError::blocked(format!(
            "patch approve 복구 필요\n- code: source-install.recovery-required\n- path: {}\n- error: {}\n- journal: {}\n- 동작: committed journal과 rollback/guard 증거를 보존했습니다.",
            target.relative_path,
            err.message,
            journal_path.display()
        )));
    }
    crate::transition::remove_committed_source_bundle(&bundle, &journal_path)?;

    let applied = fs::read_to_string(&target.absolute_path).map_err(|err| {
        let rollback = restore_bytes(
            &target.absolute_path,
            current.as_bytes(),
            &record.proposed_sha256,
            &record.original_sha256,
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
    restore_bytes(
        &target.absolute_path,
        &original,
        &record.proposed_sha256,
        &record.original_sha256,
    )
}

struct ApprovalLock {
    _lease: lease::RecoverableLease,
}

impl ApprovalLock {
    fn acquire(proposal_id: &str) -> Result<Self, AppError> {
        let path = paths::project_patch_proposals_dir().join(format!("{proposal_id}.approve.lock"));
        lease::RecoverableLease::acquire(path, "patch approve").map(|lease| Self { _lease: lease })
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

pub(crate) fn approval_transaction_fault(stage: &str) -> Result<(), AppError> {
    if cfg!(debug_assertions)
        && std::env::var("RPOTATO_TEST_APPROVAL_TRANSACTION_FAULT").as_deref() == Ok(stage)
    {
        return Err(AppError::runtime(format!(
            "injected prepared approval transaction fault: {stage}"
        )));
    }
    Ok(())
}

pub(crate) fn verification_approval_transaction_fault(stage: &str) -> Result<(), AppError> {
    if cfg!(debug_assertions)
        && std::env::var("RPOTATO_TEST_VERIFICATION_APPROVAL_FAULT").as_deref() == Ok(stage)
    {
        return Err(AppError::runtime(format!(
            "injected prepared verification approval fault: {stage}"
        )));
    }
    Ok(())
}

pub(crate) fn approval_projection_fault() -> Result<(), AppError> {
    if cfg!(debug_assertions)
        && std::env::var("RPOTATO_TEST_APPROVAL_PROJECTION_FAULT").as_deref() == Ok("converge")
    {
        return Err(AppError::runtime(
            "injected prepared approval projection convergence fault",
        ));
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
    let current = match fs::read(target) {
        Ok(current) if sha256_bytes(&current) == expected_current_hash => current,
        Ok(current) => {
            return RollbackResult {
                restored: false,
                status: format!(
                    "restore-conflict: target changed concurrently current={}",
                    sha256_bytes(&current)
                ),
            }
        }
        Err(err) => {
            return RollbackResult {
                restored: false,
                status: format!("restore-failed: target read error: {err}"),
            }
        }
    };
    let plan = match crate::transition::prepare_source_install_v1(
        &format!("intent-rollback-{}", &expected_hash[..16]),
        "proposal-rollback",
        target,
        &current,
        contents,
    ) {
        Ok(plan) => plan,
        Err(err) => {
            return RollbackResult {
                restored: false,
                status: format!(
                    "restore-failed: rollback plan preparation failed: {}",
                    err.message
                ),
            }
        }
    };
    let bundle = match crate::transition::prepare_source_bundle(
        &format!("intent-rollback-{}", &expected_hash[..16]),
        None,
        plan,
        &current,
        contents,
    ) {
        Ok(bundle) => bundle,
        Err(err) => {
            return RollbackResult {
                restored: false,
                status: format!(
                    "restore-failed: rollback bundle preparation failed: {}",
                    err.message
                ),
            }
        }
    };
    let journal_path = match crate::transition::commit_prepared_source_bundle(&bundle) {
        Ok(path) => path,
        Err(err) => {
            return RollbackResult {
                restored: false,
                status: format!(
                    "restore-failed: rollback journal commit failed: {}",
                    err.message
                ),
            }
        }
    };
    if let Err(err) = state::install_prepared_source_bundle(&bundle, &journal_path) {
        return RollbackResult {
            restored: false,
            status: format!("restore-failed: {}", err.message),
        };
    }
    if let Err(err) = crate::transition::remove_committed_source_bundle(&bundle, &journal_path) {
        return RollbackResult {
            restored: false,
            status: format!(
                "restore-failed: rollback journal cleanup failed: {}",
                err.message
            ),
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
            "patch preview 차단\\n- 이유: target read policy가 allow가 아닙니다.\\n- path: {}\\n- decision: {}",
            target.relative_path,
            read_decision_label(read_decision.decision)
        )));
    }
    let write_decision = policy::classify_path(PathMode::Write, &target.relative_path)?;
    if write_decision.decision == Decision::Deny {
        return Err(AppError::blocked(format!(
            "patch preview 차단\\n- 이유: target write policy가 deny입니다.\\n- path: {}",
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
    if metadata.len() > proposal_domain::MAX_PATCH_FILE_BYTES {
        return Err(AppError::blocked(format!(
            "patch preview 차단\\n- 이유: 대상 파일이 preview 한도를 초과했습니다.\\n- path: {}\\n- size bytes: {}\\n- max bytes: {}",
            target.relative_path,
            metadata.len(),
            proposal_domain::MAX_PATCH_FILE_BYTES
        )));
    }
    let original = fs::read_to_string(&target.absolute_path).map_err(|err| {
        AppError::runtime(format!(
            "patch preview 대상 파일을 UTF-8 text로 읽지 못했습니다: {} ({err})",
            target.relative_path
        ))
    })?;
    let approval_token = if workflow_id.is_empty() {
        String::new()
    } else {
        issue_approval_token()?
    };

    proposal_domain::build_preview(PreviewInput {
        relative_path: &target.relative_path,
        original: &original,
        find,
        replace,
        workflow_id,
        action_id,
        verification_command,
        approval_token,
        proposal_dir: &paths::project_patch_proposals_dir(),
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
    let relative = absolute_path.strip_prefix(&project_root).map_err(|_| {
        AppError::blocked(format!(
            "{operation} 차단\n- 이유: project boundary 밖 path입니다.\n- path: {}",
            raw_path
        ))
    })?;
    let relative_path = relative
        .to_str()
        .ok_or_else(|| {
            AppError::blocked(format!(
                "{operation} 차단\n- 이유: canonical project-relative path가 UTF-8이 아닙니다.\n- 동작: proposal, journal, event, source를 변경하지 않았습니다."
            ))
        })?
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
    let body = proposal_domain::render_record(preview);
    state::atomic_replace_bytes(&preview.proposal_path, body.as_bytes())
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
    fn cli_approval_materializes_typed_verification_credential_once() {
        let token = "ab".repeat(32);
        let dispatch = ApprovalDispatch {
            report: "patch approve\n- status: applied-awaiting-verification".to_string(),
            verification_credential: Some(OneShotSecret::new(token.clone()).unwrap()),
        };

        let report = dispatch.into_test_report("proposal-one");

        assert_eq!(report.matches(&token).count(), 1);
        assert!(report
            .contains("verification command approval: rpotato patch verify proposal-one --token"));
    }

    #[test]
    fn fix_test_requires_cargo_test_verification() {
        let error = validate_skill_verification("fix-test", "pwd").unwrap_err();

        assert_eq!(error.code, 3);
        assert!(error.message.contains("cargo test"));
        validate_skill_verification("fix-test", "cargo test").unwrap();
        validate_skill_verification("small-patch", "pwd").unwrap();
    }

    #[test]
    fn skill_phase_mismatch_blocks_before_patch_apply() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("skill-phase-mismatch");
        let (target, mut workflow, proposal) = create_pending_workflow(&root, "pwd");
        let mut runtime = crate::skill::SkillRuntimeState::new("small-patch", "explicit").unwrap();
        for state in [
            crate::skill::SkillState::ContextReady,
            crate::skill::SkillState::ModelRequested,
            crate::skill::SkillState::ActionRecorded,
        ] {
            runtime.transition(state).unwrap();
        }
        runtime.store_in_workflow(&mut workflow);
        state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();

        let error = approve_report(&proposal.proposal_id, &proposal.approval_token, false, None)
            .unwrap_err();
        let source = fs::read_to_string(&target).unwrap();
        clear_patch_test_env(&root);

        assert_eq!(error.code, 3);
        assert!(error.message.contains("skill side effect 차단"));
        assert!(error
            .message
            .contains("expected skill state: awaiting-approval"));
        assert_eq!(source, "pub const X: i32 = 1;\n");
    }

    #[test]
    fn completed_workflow_requires_complete_skill_state() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("complete-skill-state");
        let (_target, mut workflow, _proposal) = create_pending_workflow(&root, "pwd");
        let mut runtime = crate::skill::SkillRuntimeState::new("small-patch", "explicit").unwrap();
        for state in [
            crate::skill::SkillState::ContextReady,
            crate::skill::SkillState::ModelRequested,
            crate::skill::SkillState::ActionRecorded,
            crate::skill::SkillState::AwaitingApproval,
            crate::skill::SkillState::AwaitingVerification,
            crate::skill::SkillState::StopPassed,
        ] {
            runtime.transition(state).unwrap();
        }
        runtime.store_in_workflow(&mut workflow);
        workflow.phase = "complete".to_string();

        let error = validate_completed_workflow(&workflow).unwrap_err();
        clear_patch_test_env(&root);

        assert_eq!(error.code, 3);
        assert!(error.message.contains("skill state: stop-passed"));
    }

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
        let rollback_dir = root
            .join("project")
            .join(".rpotato")
            .join("patches")
            .join(&proposal.proposal_id);
        let rollback_exists = fs::read_dir(&rollback_dir)
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| {
                entry.path().extension().and_then(|value| value.to_str()) == Some("rollback")
            });
        clear_patch_test_env(&root);

        assert_eq!(contents, "pub const X: i32 = 2;\n");
        assert!(rollback_exists);
        assert!(approval.contains("status: applied-awaiting-verification"));
        assert!(approval.contains("verification command는 아직 실행하지 않았습니다"));
        assert!(!approval.contains("stop gate: 통과"));
    }

    #[test]
    fn approval_without_active_skill_fails_before_any_source_effect() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("approval-without-skill");
        let (target, mut workflow, proposal) = create_pending_workflow(&root, "pwd");
        workflow.active_skill_id.clear();
        workflow.skill_invocation.clear();
        workflow.skill_state.clear();
        workflow.skill_completed_hooks.clear();
        workflow.skill_evidence.clear();
        workflow.skill_stop_criteria.clear();
        state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();
        let before_events = ledger::read_runtime_events().unwrap().len();

        let error = approve_report(&proposal.proposal_id, &proposal.approval_token, false, None)
            .unwrap_err();

        assert!(error.message.contains("active built-in skill"));
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "pub const X: i32 = 1;\n"
        );
        assert_eq!(ledger::read_runtime_events().unwrap().len(), before_events);
        clear_patch_test_env(&root);
    }

    #[test]
    fn prepared_skill_approval_commits_exact_e0_e9_and_single_current_revision() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("prepared-skill-approval");
        let (target, mut workflow, proposal) = create_pending_workflow(&root, "pwd");
        let mut skill = crate::skill::SkillRuntimeState::new("small-patch", "explicit").unwrap();
        for state in [
            crate::skill::SkillState::ContextReady,
            crate::skill::SkillState::ModelRequested,
            crate::skill::SkillState::ActionRecorded,
            crate::skill::SkillState::AwaitingApproval,
        ] {
            skill.transition(state).unwrap();
        }
        skill.store_in_workflow(&mut workflow);
        workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();
        let before_workflow_revision = workflow.revision;
        let before_current_revision = state::current_state_lease_view().unwrap().revision;
        let before_events = ledger::read_runtime_events().unwrap().len();
        let intent_id = "intent-prepared-skill-approval";

        let report = approve_report_for_intent(
            &proposal.proposal_id,
            &proposal.approval_token,
            false,
            None,
            intent_id,
        )
        .unwrap();
        let after = state::load_workflow(&workflow.workflow_id).unwrap();
        let events = ledger::read_runtime_events().unwrap();
        let suffix = &events[before_events..];

        assert_eq!(
            fs::read_to_string(target).unwrap(),
            "pub const X: i32 = 2;\n"
        );
        assert_eq!(after.revision, before_workflow_revision + 2);
        assert_eq!(after.phase, "pending-verification-approval");
        assert_eq!(
            state::current_state_lease_view().unwrap().revision,
            before_current_revision + 1
        );
        assert_eq!(suffix.len(), 10);
        assert_eq!(
            suffix
                .iter()
                .map(|event| event.event_type.as_str())
                .collect::<Vec<_>>(),
            vec![
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
            ]
        );
        assert!(report.contains("exact prepared journal과 E0..E9"));
        assert!(!paths::project_transition_journal_file(&workflow.project_id, intent_id).exists());
        assert!(!paths::projection_lag_dir().exists());
        clear_patch_test_env(&root);
    }

    #[test]
    fn workflow_pointer_crash_between_r1_r2_installs_recovers_to_r2() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("prepared-pointer-crash");
        let (target, mut workflow, proposal) = create_pending_workflow(&root, "pwd");
        let mut skill = crate::skill::SkillRuntimeState::new("small-patch", "explicit").unwrap();
        for skill_state in [
            crate::skill::SkillState::ContextReady,
            crate::skill::SkillState::ModelRequested,
            crate::skill::SkillState::ActionRecorded,
            crate::skill::SkillState::AwaitingApproval,
        ] {
            skill.transition(skill_state).unwrap();
        }
        skill.store_in_workflow(&mut workflow);
        workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();
        let before_workflow_revision = workflow.revision;
        let before_current_revision = state::current_state_lease_view().unwrap().revision;
        let before_event_count = ledger::read_runtime_events().unwrap().len();
        let intent_id = "intent-prepared-pointer-crash";
        std::env::set_var("RPOTATO_TEST_APPROVAL_TRANSACTION_FAULT", "T3");

        let error = approve_report_for_intent(
            &proposal.proposal_id,
            &proposal.approval_token,
            false,
            None,
            intent_id,
        )
        .unwrap_err();
        std::env::remove_var("RPOTATO_TEST_APPROVAL_TRANSACTION_FAULT");

        assert!(
            error
                .message
                .contains("injected prepared approval transaction fault: T3"),
            "unexpected error: {}",
            error.message
        );
        let r1_pointer =
            fs::read_to_string(paths::project_workflow_file(&workflow.workflow_id)).unwrap();
        assert!(r1_pointer.contains(&format!(
            "\"committed_revision\": {}",
            before_workflow_revision + 1
        )));
        let r1_snapshot = fs::read_to_string(paths::project_workflow_snapshot_file(
            &workflow.workflow_id,
            before_workflow_revision + 1,
        ))
        .unwrap();
        assert!(r1_snapshot.contains("\"phase\": \"approved\""));
        assert!(paths::project_transition_journal_file(&workflow.project_id, intent_id).exists());

        let repair_required = crate::transition::recover_pending_source_bundles().unwrap_err();
        assert!(
            repair_required
                .message
                .contains("projection.repair-required"),
            "unexpected first recovery result: {}",
            repair_required.message
        );
        assert_eq!(
            crate::transition::recover_pending_source_bundles().unwrap(),
            1
        );
        let r2 = state::load_workflow(&workflow.workflow_id).unwrap();
        let current_after = state::current_state_lease_view().unwrap();
        let events_after = ledger::read_runtime_events().unwrap();
        assert_eq!(r2.revision, before_workflow_revision + 2);
        assert_eq!(r2.phase, "pending-verification-approval");
        assert_eq!(current_after.revision, before_current_revision + 1);
        assert_eq!(events_after.len(), before_event_count + 10);
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "pub const X: i32 = 2;\n"
        );
        assert!(!paths::project_transition_journal_file(&workflow.project_id, intent_id).exists());
        assert!(
            fs::read_dir(paths::projection_lag_dir())
                .unwrap()
                .next()
                .is_none(),
            "projection lag marker cleanup must leave no durable entries"
        );

        assert_eq!(
            crate::transition::recover_pending_source_bundles().unwrap(),
            0
        );
        assert_eq!(state::load_workflow(&workflow.workflow_id).unwrap(), r2);
        assert_eq!(state::current_state_lease_view().unwrap(), current_after);
        assert_eq!(ledger::read_runtime_events().unwrap(), events_after);
        clear_patch_test_env(&root);
    }

    #[test]
    fn same_approval_intent_after_cleanup_is_refresh_only_with_zero_delta() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("prepared-same-intent-retry");
        let (_target, mut workflow, proposal) = create_pending_workflow(&root, "pwd");
        let mut skill = crate::skill::SkillRuntimeState::new("small-patch", "explicit").unwrap();
        for skill_state in [
            crate::skill::SkillState::ContextReady,
            crate::skill::SkillState::ModelRequested,
            crate::skill::SkillState::ActionRecorded,
            crate::skill::SkillState::AwaitingApproval,
        ] {
            skill.transition(skill_state).unwrap();
        }
        skill.store_in_workflow(&mut workflow);
        state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();
        let intent_id = "intent-prepared-same-retry";
        approve_report_for_intent(
            &proposal.proposal_id,
            &proposal.approval_token,
            false,
            None,
            intent_id,
        )
        .unwrap();
        let workflow_pointer = paths::project_workflow_file(&workflow.workflow_id);
        let workflow_before = fs::read_to_string(&workflow_pointer).unwrap();
        let current_before = fs::read_to_string(paths::current_state_file()).unwrap();
        let events_before = ledger::read_runtime_events().unwrap();

        let retry = approve_report_for_intent(
            &proposal.proposal_id,
            &proposal.approval_token,
            false,
            None,
            intent_id,
        )
        .unwrap();

        assert!(retry.contains("status: refresh-only"));
        assert!(retry.contains("code: secret.refresh-only"));
        assert!(!retry.contains("verification command approval:"));
        assert_eq!(
            fs::read_to_string(&workflow_pointer).unwrap(),
            workflow_before
        );
        assert_eq!(
            fs::read_to_string(paths::current_state_file()).unwrap(),
            current_before
        );
        assert_eq!(ledger::read_runtime_events().unwrap(), events_before);
        clear_patch_test_env(&root);
    }

    #[test]
    fn prepared_approval_t1_t10_faults_recover_exactly_once() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        for stage in [
            "T1",
            "T2",
            "T3-before-pointer",
            "T3",
            "T4",
            "T5",
            "T6",
            "T7",
            "T8-before-pointer",
            "T8",
            "T9",
            "T10",
        ] {
            let root = patch_test_root(&format!("prepared-recover-{stage}"));
            let (target, workflow, proposal) = create_pending_workflow(&root, "pwd");
            let before_workflow_revision = workflow.revision;
            let before_current_revision = state::current_state_lease_view().unwrap().revision;
            let before_event_count = ledger::read_runtime_events().unwrap().len();
            let intent_id = format!("intent-prepared-recover-{}", stage.to_ascii_lowercase());
            std::env::set_var("RPOTATO_TEST_APPROVAL_TRANSACTION_FAULT", stage);

            let error = approve_report_for_intent(
                &proposal.proposal_id,
                &proposal.approval_token,
                false,
                None,
                &intent_id,
            )
            .unwrap_err();
            std::env::remove_var("RPOTATO_TEST_APPROVAL_TRANSACTION_FAULT");
            assert!(
                error.message.contains(stage),
                "stage: {stage}, error: {}",
                error.message
            );
            assert!(
                paths::project_transition_journal_file(&workflow.project_id, &intent_id).exists()
            );

            if stage == "T10" {
                assert_eq!(
                    crate::transition::recover_pending_source_bundles().unwrap(),
                    1,
                    "stage: {stage}"
                );
            } else {
                let interrupted = crate::transition::recover_pending_source_bundles().unwrap_err();
                assert!(
                    interrupted.message.contains("projection.repair-required"),
                    "stage: {stage}, error: {}",
                    interrupted.message
                );
                let journal =
                    paths::project_transition_journal_file(&workflow.project_id, &intent_id);
                let bundle = crate::transition::parse_prepared_source_bundle(
                    &fs::read_to_string(&journal).unwrap(),
                )
                .unwrap();
                assert!(crate::transition::projection_lag_path(&bundle)
                    .unwrap()
                    .exists());
                assert_eq!(
                    crate::transition::recover_pending_source_bundles().unwrap(),
                    1,
                    "stage: {stage}"
                );
            }
            let recovered = state::load_workflow(&workflow.workflow_id).unwrap();
            let current = state::current_state_lease_view().unwrap();
            let events = ledger::read_runtime_events().unwrap();
            assert_eq!(
                recovered.revision,
                before_workflow_revision + 2,
                "stage: {stage}"
            );
            assert_eq!(
                recovered.phase, "pending-verification-approval",
                "stage: {stage}"
            );
            assert_eq!(
                current.revision,
                before_current_revision + 1,
                "stage: {stage}"
            );
            assert_eq!(events.len(), before_event_count + 10, "stage: {stage}");
            assert_eq!(
                fs::read_to_string(&target).unwrap(),
                "pub const X: i32 = 2;\n",
                "stage: {stage}"
            );
            assert!(
                !paths::project_transition_journal_file(&workflow.project_id, &intent_id).exists()
            );
            assert_eq!(
                crate::transition::recover_pending_source_bundles().unwrap(),
                0,
                "stage: {stage}"
            );
            assert_eq!(
                state::load_workflow(&workflow.workflow_id).unwrap(),
                recovered
            );
            assert_eq!(state::current_state_lease_view().unwrap(), current);
            assert_eq!(ledger::read_runtime_events().unwrap(), events);
            if stage == "T5" {
                let rotation = rotate_workflow_token_report(&proposal.proposal_id).unwrap();
                let replacement = report_value(&rotation, "새 approval token").unwrap();
                let verified = verify_report(&proposal.proposal_id, &replacement).unwrap();
                assert!(verified.contains("패치 작업 완료"));
                assert_eq!(
                    state::load_workflow(&workflow.workflow_id).unwrap().phase,
                    "complete"
                );
            }
            clear_patch_test_env(&root);
        }
    }

    #[test]
    fn second_intent_after_t1_recovers_or_blocks_before_competing_journal() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("prepared-second-intent-after-t1");
        let (_target, mut workflow, proposal) = create_pending_workflow(&root, "pwd");
        let mut skill = crate::skill::SkillRuntimeState::new("small-patch", "explicit").unwrap();
        for skill_state in [
            crate::skill::SkillState::ContextReady,
            crate::skill::SkillState::ModelRequested,
            crate::skill::SkillState::ActionRecorded,
            crate::skill::SkillState::AwaitingApproval,
        ] {
            skill.transition(skill_state).unwrap();
        }
        skill.store_in_workflow(&mut workflow);
        state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();
        let first_intent = "intent-prepared-first-t1";
        let second_intent = "intent-prepared-second";
        std::env::set_var("RPOTATO_TEST_APPROVAL_TRANSACTION_FAULT", "T1");
        approve_report_for_intent(
            &proposal.proposal_id,
            &proposal.approval_token,
            false,
            None,
            first_intent,
        )
        .unwrap_err();
        std::env::remove_var("RPOTATO_TEST_APPROVAL_TRANSACTION_FAULT");

        let second = approve_report_for_intent(
            &proposal.proposal_id,
            &proposal.approval_token,
            false,
            None,
            second_intent,
        )
        .unwrap_err();

        assert!(second.message.contains("projection.repair-required"));
        assert!(
            paths::project_transition_journal_file(&workflow.project_id, first_intent).exists()
        );
        assert!(
            !paths::project_transition_journal_file(&workflow.project_id, second_intent).exists()
        );
        let prepared = fs::read_dir(paths::project_transition_journal_dir(&workflow.project_id))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                name.ends_with(".prepared.json") || name.ends_with(".prepared.json.tmp")
            })
            .count();
        assert_eq!(prepared, 1);
        clear_patch_test_env(&root);
    }

    #[test]
    fn source_install_unsupported_platform_blocks_before_all_effects() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("unsupported-platform-zero-effects");
        let _ = fs::remove_dir_all(&root);
        std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let error = ensure_source_install_platform_supported(false, "windows", false).unwrap_err();

        assert!(error
            .message
            .contains("source-install.unsupported-platform"));
        assert!(!root.exists());
        assert!(ensure_source_install_platform_supported(false, "windows", true).is_ok());
        let source = include_str!("patch.rs");
        let dispatch = source
            .split_once("fn approve_dispatch_for_intent(")
            .unwrap()
            .1
            .split_once("fn ensure_source_install_platform_supported(")
            .unwrap()
            .0;
        assert!(
            dispatch
                .find("ensure_source_install_platform_supported")
                .unwrap()
                < dispatch.find("let proposal_path").unwrap()
        );
        clear_patch_test_env(&root);
    }

    #[test]
    fn t10_lag_install_failure_preserves_committed_journal() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("t10-lag-install-failure");
        let (_target, workflow, proposal) = create_prepared_pending_workflow(&root, "pwd");
        let intent_id = "intent-t10-lag-install-failure";
        std::env::set_var("RPOTATO_TEST_APPROVAL_PROJECTION_FAULT", "converge");
        std::env::set_var("RPOTATO_TEST_PROJECTION_LAG_FAULT", "temp-fsync");

        let error = approve_report_for_intent(
            &proposal.proposal_id,
            &proposal.approval_token,
            false,
            None,
            intent_id,
        )
        .unwrap_err();
        std::env::remove_var("RPOTATO_TEST_APPROVAL_PROJECTION_FAULT");
        std::env::remove_var("RPOTATO_TEST_PROJECTION_LAG_FAULT");

        let journal = paths::project_transition_journal_file(&workflow.project_id, intent_id);
        let bundle =
            crate::transition::parse_prepared_source_bundle(&fs::read_to_string(&journal).unwrap())
                .unwrap();
        let lag = crate::transition::projection_lag_path(&bundle).unwrap();
        assert!(error.message.contains("projection.lag-install-failed"));
        assert!(journal.exists());
        assert!(!lag.exists());
        assert_eq!(
            fs::read_to_string(lag.with_extension("json.tmp")).unwrap(),
            bundle.additional_members.last().unwrap().bytes_utf8
        );
        assert!(crate::transition::recover_pending_source_bundles()
            .unwrap_err()
            .message
            .contains("projection.repair-required"));
        assert_eq!(
            crate::transition::recover_pending_source_bundles().unwrap(),
            1
        );
        assert!(!journal.exists());
        assert!(!lag.exists());
        clear_patch_test_env(&root);
    }

    #[test]
    fn projection_lag_crash_after_lag_removal_before_journal_cleanup_converges() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("lag-remove-before-journal-cleanup");
        let (_target, workflow, proposal) = create_prepared_pending_workflow(&root, "pwd");
        let intent_id = "intent-lag-remove-before-journal-cleanup";
        std::env::set_var("RPOTATO_TEST_APPROVAL_PROJECTION_FAULT", "converge");
        approve_report_for_intent(
            &proposal.proposal_id,
            &proposal.approval_token,
            false,
            None,
            intent_id,
        )
        .unwrap_err();
        std::env::remove_var("RPOTATO_TEST_APPROVAL_PROJECTION_FAULT");
        let journal = paths::project_transition_journal_file(&workflow.project_id, intent_id);
        let bundle =
            crate::transition::parse_prepared_source_bundle(&fs::read_to_string(&journal).unwrap())
                .unwrap();
        let lag = crate::transition::projection_lag_path(&bundle).unwrap();
        assert!(lag.exists());
        std::env::set_var("RPOTATO_TEST_PROJECTION_LAG_FAULT", "journal-remove");

        let interrupted = crate::transition::recover_pending_source_bundles().unwrap_err();
        std::env::remove_var("RPOTATO_TEST_PROJECTION_LAG_FAULT");

        assert!(interrupted.message.contains("journal-remove"));
        assert!(journal.exists());
        assert!(!lag.exists());
        assert_eq!(
            crate::transition::recover_pending_source_bundles().unwrap(),
            1
        );
        assert!(!journal.exists());
        assert_eq!(
            crate::transition::recover_pending_source_bundles().unwrap(),
            0
        );
        clear_patch_test_env(&root);
    }

    #[test]
    fn projection_success_receipt_requires_lag_and_journal_parent_fsyncs() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        for (case, projection_fault, lag_fault) in [
            ("lag-parent", true, "parent-fsync"),
            ("journal-parent", false, "journal-parent-fsync"),
        ] {
            let root = patch_test_root(&format!("success-receipt-fsync-{case}"));
            let (_target, workflow, proposal) = create_prepared_pending_workflow(&root, "pwd");
            let intent_id = format!("intent-success-receipt-fsync-{case}");
            if projection_fault {
                std::env::set_var("RPOTATO_TEST_APPROVAL_PROJECTION_FAULT", "converge");
            }
            std::env::set_var("RPOTATO_TEST_PROJECTION_LAG_FAULT", lag_fault);

            let error = approve_report_for_intent(
                &proposal.proposal_id,
                &proposal.approval_token,
                false,
                None,
                &intent_id,
            )
            .unwrap_err();
            std::env::remove_var("RPOTATO_TEST_APPROVAL_PROJECTION_FAULT");
            std::env::remove_var("RPOTATO_TEST_PROJECTION_LAG_FAULT");

            let journal = paths::project_transition_journal_file(&workflow.project_id, &intent_id);
            assert!(error.message.contains(lag_fault), "case: {case}");
            assert!(journal.exists(), "case: {case}");
            assert_eq!(
                crate::transition::recover_pending_source_bundles().unwrap(),
                1
            );
            let retry = approve_report_for_intent(
                &proposal.proposal_id,
                &proposal.approval_token,
                false,
                None,
                &intent_id,
            )
            .unwrap();
            assert!(retry.contains("status: refresh-only"), "case: {case}");
            clear_patch_test_env(&root);
        }
    }

    #[test]
    fn projection_lag_journal_cleanup_state_matrix_is_closed() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("prepared-projection-repair");
        let (_target, mut workflow, proposal) = create_pending_workflow(&root, "pwd");
        let mut skill = crate::skill::SkillRuntimeState::new("small-patch", "explicit").unwrap();
        for skill_state in [
            crate::skill::SkillState::ContextReady,
            crate::skill::SkillState::ModelRequested,
            crate::skill::SkillState::ActionRecorded,
            crate::skill::SkillState::AwaitingApproval,
        ] {
            skill.transition(skill_state).unwrap();
        }
        skill.store_in_workflow(&mut workflow);
        state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();
        let intent_id = "intent-prepared-projection-repair";
        std::env::set_var("RPOTATO_TEST_APPROVAL_PROJECTION_FAULT", "converge");

        let error = approve_report_for_intent(
            &proposal.proposal_id,
            &proposal.approval_token,
            false,
            None,
            intent_id,
        )
        .unwrap_err();
        std::env::remove_var("RPOTATO_TEST_APPROVAL_PROJECTION_FAULT");

        assert!(error.message.contains("projection.repair-required"));
        let journal = paths::project_transition_journal_file(&workflow.project_id, intent_id);
        let bundle =
            crate::transition::parse_prepared_source_bundle(&fs::read_to_string(&journal).unwrap())
                .unwrap();
        let final_event_id = &bundle.semantic_events[9].event_id;
        let lag = paths::projection_lag_file(intent_id, final_event_id);
        let lag_member = bundle.additional_members.last().unwrap();
        assert_eq!(fs::read_to_string(&lag).unwrap(), lag_member.bytes_utf8);
        assert!(journal.exists());
        let workflow_pointer = paths::project_workflow_file(&workflow.workflow_id);
        let workflow_before = fs::read_to_string(&workflow_pointer).unwrap();
        let current_before = fs::read_to_string(paths::current_state_file()).unwrap();
        let events_before = ledger::read_runtime_events().unwrap();

        fs::remove_file(&lag).unwrap();
        std::env::set_var("RPOTATO_TEST_APPROVAL_PROJECTION_FAULT", "converge");
        let interrupted_repair = crate::transition::recover_pending_source_bundles().unwrap_err();
        std::env::remove_var("RPOTATO_TEST_APPROVAL_PROJECTION_FAULT");
        assert!(interrupted_repair
            .message
            .contains("projection.repair-required"));
        assert!(journal.exists());
        assert_eq!(fs::read_to_string(&lag).unwrap(), lag_member.bytes_utf8);
        assert_eq!(
            fs::read_to_string(&workflow_pointer).unwrap(),
            workflow_before
        );
        assert_eq!(
            fs::read_to_string(paths::current_state_file()).unwrap(),
            current_before
        );
        assert_eq!(ledger::read_runtime_events().unwrap(), events_before);

        assert_eq!(
            crate::transition::recover_pending_source_bundles().unwrap(),
            1
        );
        assert!(!journal.exists());
        assert!(!lag.exists());
        assert_eq!(
            fs::read_to_string(&workflow_pointer).unwrap(),
            workflow_before
        );
        assert_eq!(
            fs::read_to_string(paths::current_state_file()).unwrap(),
            current_before
        );
        assert_eq!(ledger::read_runtime_events().unwrap(), events_before);

        fs::create_dir_all(lag.parent().unwrap()).unwrap();
        fs::write(&lag, lag_member.bytes_utf8.as_bytes()).unwrap();
        let orphan = crate::transition::recover_pending_source_bundles().unwrap_err();
        assert!(orphan
            .message
            .contains("orphan 또는 ambiguous projection lag"));
        assert_eq!(fs::read_to_string(&lag).unwrap(), lag_member.bytes_utf8);
        fs::remove_file(&lag).unwrap();
        clear_patch_test_env(&root);
    }

    #[test]
    fn projection_lag_reference_and_member_mutation_matrix_fails_closed() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("projection-lag-mutation-matrix");
        let (_target, workflow, proposal) = create_prepared_pending_workflow(&root, "pwd");
        let intent_id = "intent-projection-lag-mutation-matrix";
        std::env::set_var("RPOTATO_TEST_APPROVAL_PROJECTION_FAULT", "converge");
        approve_report_for_intent(
            &proposal.proposal_id,
            &proposal.approval_token,
            false,
            None,
            intent_id,
        )
        .unwrap_err();
        std::env::remove_var("RPOTATO_TEST_APPROVAL_PROJECTION_FAULT");
        let journal = paths::project_transition_journal_file(&workflow.project_id, intent_id);
        let body = fs::read_to_string(&journal).unwrap();
        let bundle = crate::transition::parse_prepared_source_bundle(&body).unwrap();
        let lag_member = bundle.additional_members.last().unwrap();
        let event_id = lag_member.binding.event_id.as_deref().unwrap();
        let mutations = [
            body.replacen("\"member_index\":10", "\"member_index\":9", 1),
            body.replacen(
                "project-session-ledger",
                "project-session-ledger-mutated",
                1,
            ),
            body.replacen(event_id, "event-mutated", 1),
            body.replacen(&lag_member.path, "state/projection-lag/wrong.json", 1),
            body.replacen(
                &lag_member.binding.artifact_id.clone().unwrap(),
                "projection-lag-deadbeef",
                1,
            ),
        ];
        for (index, mutation) in mutations.iter().enumerate() {
            assert_ne!(mutation, &body, "mutation {index} changed no bytes");
            assert!(
                crate::transition::parse_prepared_source_bundle(mutation).is_err(),
                "mutation {index}"
            );
        }
        clear_patch_test_env(&root);
    }

    #[test]
    fn projection_lag_restart_validates_reference_member_installed_bytes_and_head() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("projection-lag-restart-validation");
        let (_target, workflow, proposal) = create_prepared_pending_workflow(&root, "pwd");
        let intent_id = "intent-projection-lag-restart-validation";
        std::env::set_var("RPOTATO_TEST_APPROVAL_PROJECTION_FAULT", "converge");
        approve_report_for_intent(
            &proposal.proposal_id,
            &proposal.approval_token,
            false,
            None,
            intent_id,
        )
        .unwrap_err();
        std::env::remove_var("RPOTATO_TEST_APPROVAL_PROJECTION_FAULT");
        let journal = paths::project_transition_journal_file(&workflow.project_id, intent_id);
        let bundle =
            crate::transition::parse_prepared_source_bundle(&fs::read_to_string(&journal).unwrap())
                .unwrap();
        let lag = crate::transition::projection_lag_path(&bundle).unwrap();
        let current_before = fs::read(paths::current_state_file()).unwrap();
        let workflow_before =
            fs::read(paths::project_workflow_file(&workflow.workflow_id)).unwrap();
        let events_before = ledger::read_runtime_events().unwrap();
        let installed = fs::read_to_string(&lag).unwrap();
        fs::write(
            &lag,
            installed.replacen(
                "project-session-ledger",
                "project-session-ledger-mutated",
                1,
            ),
        )
        .unwrap();

        let error = crate::transition::recover_pending_source_bundles().unwrap_err();

        assert!(error.message.contains("projection lag"));
        assert_eq!(
            fs::read(paths::current_state_file()).unwrap(),
            current_before
        );
        assert_eq!(
            fs::read(paths::project_workflow_file(&workflow.workflow_id)).unwrap(),
            workflow_before
        );
        assert_eq!(ledger::read_runtime_events().unwrap(), events_before);
        assert!(journal.exists());
        clear_patch_test_env(&root);
    }

    #[test]
    fn projection_lag_orphan_without_journal_blocks() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("projection-lag-orphan-without-journal");
        set_patch_test_env(&root);
        fs::create_dir_all(root.join("project")).unwrap();
        state::initialize().unwrap();
        let lag = paths::projection_lag_file("intent-orphan", "event-orphan");
        fs::create_dir_all(lag.parent().unwrap()).unwrap();
        fs::write(&lag, b"{}" as &[u8]).unwrap();

        let error = crate::transition::recover_pending_source_bundles().unwrap_err();

        assert!(error
            .message
            .contains("orphan 또는 ambiguous projection lag"));
        assert_eq!(fs::read(&lag).unwrap(), b"{}" as &[u8]);
        clear_patch_test_env(&root);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn canonical_non_utf8_source_path_fails_before_any_effect() {
        use std::os::unix::ffi::OsStringExt;
        use std::os::unix::fs::symlink;

        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("canonical-non-utf8-source");
        let project = root.join("project");
        fs::create_dir_all(&project).unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        state::initialize().unwrap();
        let non_utf8 = project.join(std::ffi::OsString::from_vec(
            b"source-non-utf8-\xff.rs".to_vec(),
        ));
        fs::write(&non_utf8, b"pub const VALUE: i32 = 1;\n").unwrap();
        symlink(&non_utf8, project.join("source-link.rs")).unwrap();
        let current_before = fs::read(paths::current_state_file()).unwrap();
        let ledger_before = fs::read(paths::runtime_ledger_file()).unwrap();
        let journal_before = fs::read_dir(paths::project_transition_journal_dir(
            &ledger::validated_current_identity().unwrap().project_id,
        ))
        .unwrap()
        .count();

        let error = resolve_target_for("patch approve", "source-link.rs").unwrap_err();

        assert!(error
            .message
            .contains("canonical project-relative path가 UTF-8이 아닙니다"));
        assert_eq!(
            fs::read(paths::current_state_file()).unwrap(),
            current_before
        );
        assert_eq!(
            fs::read(paths::runtime_ledger_file()).unwrap(),
            ledger_before
        );
        assert_eq!(
            fs::read_dir(paths::project_transition_journal_dir(
                &ledger::validated_current_identity().unwrap().project_id,
            ))
            .unwrap()
            .count(),
            journal_before
        );
        clear_patch_test_env(&root);
    }

    #[test]
    fn prepared_bundle_member_tamper_blocks_recovery_before_effects() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("prepared-member-tamper");
        let (target, mut workflow, proposal) = create_pending_workflow(&root, "pwd");
        let mut skill = crate::skill::SkillRuntimeState::new("small-patch", "explicit").unwrap();
        for skill_state in [
            crate::skill::SkillState::ContextReady,
            crate::skill::SkillState::ModelRequested,
            crate::skill::SkillState::ActionRecorded,
            crate::skill::SkillState::AwaitingApproval,
        ] {
            skill.transition(skill_state).unwrap();
        }
        skill.store_in_workflow(&mut workflow);
        state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();
        let intent_id = "intent-prepared-member-tamper";
        std::env::set_var("RPOTATO_TEST_APPROVAL_TRANSACTION_FAULT", "T1");
        approve_report_for_intent(
            &proposal.proposal_id,
            &proposal.approval_token,
            false,
            None,
            intent_id,
        )
        .unwrap_err();
        std::env::remove_var("RPOTATO_TEST_APPROVAL_TRANSACTION_FAULT");
        let journal = paths::project_transition_journal_file(&workflow.project_id, intent_id);
        let mut bundle =
            crate::transition::parse_prepared_source_bundle(&fs::read_to_string(&journal).unwrap())
                .unwrap();
        bundle.additional_members[2].bytes_utf8 = bundle.additional_members[2].bytes_utf8.replacen(
            "\"phase\": \"approved\"",
            "\"phase\": \"tampered\"",
            1,
        );
        fs::write(
            &journal,
            crate::transition::render_prepared_source_bundle(&bundle).unwrap(),
        )
        .unwrap();
        let source_before = fs::read_to_string(&target).unwrap();
        let workflow_pointer = paths::project_workflow_file(&workflow.workflow_id);
        let workflow_before = fs::read(&workflow_pointer).unwrap();
        let current_before = fs::read(paths::current_state_file()).unwrap();
        let events_before = ledger::read_runtime_events().unwrap();

        let error = crate::transition::recover_pending_source_bundles().unwrap_err();

        assert!(error.message.contains("workflow") || error.message.contains("corrupt"));
        assert!(journal.exists());
        assert_eq!(fs::read_to_string(&target).unwrap(), source_before);
        assert_eq!(fs::read(&workflow_pointer).unwrap(), workflow_before);
        assert_eq!(
            fs::read(paths::current_state_file()).unwrap(),
            current_before
        );
        assert_eq!(ledger::read_runtime_events().unwrap(), events_before);
        clear_patch_test_env(&root);
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

    #[cfg(unix)]
    #[test]
    fn verification_approval_commits_prepared_audit_before_command() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("verification-prepared-audit");
        let (_target, workflow, proposal) = create_pending_workflow(&root, "pwd");
        let approval =
            approve_report(&proposal.proposal_id, &proposal.approval_token, false, None).unwrap();
        let verify_token = verification_token(&approval);
        let before = ledger::read_runtime_events().unwrap().len();

        std::env::set_var(
            "RPOTATO_TEST_VERIFICATION_FAULT",
            "after-started-checkpoint",
        );
        verify_report(&proposal.proposal_id, &verify_token).unwrap_err();
        std::env::remove_var("RPOTATO_TEST_VERIFICATION_FAULT");

        let started = state::load_workflow(&workflow.workflow_id).unwrap();
        let events = ledger::read_runtime_events().unwrap();
        let committed = &events[before..];
        assert_eq!(started.phase, "verification-started");
        assert_eq!(started.verification_approval_state, "approved");
        assert_eq!(committed.len(), 3);
        assert_eq!(
            committed
                .iter()
                .map(|event| event.event_type.as_str())
                .collect::<Vec<_>>(),
            [
                "runtime.intent.accepted",
                "workflow.checkpoint",
                "patch.verification.approved",
            ]
        );
        assert!(committed[0]
            .details
            .contains("intent_kind=approve-verification"));
        assert!(committed[2].details.contains("gate=verification-command"));
        assert!(!paths::project_transition_journal_file(
            &workflow.project_id,
            &format!("intent-verify-{}", proposal.proposal_id)
        )
        .exists());
        clear_patch_test_env(&root);
    }

    #[cfg(unix)]
    #[test]
    fn prepared_verification_approval_faults_recover_without_running_command() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        for stage in ["V1", "V2", "V3-before-pointer", "V3", "V4", "V5", "V6"] {
            let root = patch_test_root(&format!("verification-recover-{stage}"));
            let (target, workflow, proposal) = create_pending_workflow(&root, "pwd");
            let approval =
                approve_report(&proposal.proposal_id, &proposal.approval_token, false, None)
                    .unwrap();
            let verify_token = verification_token(&approval);
            let before_events = ledger::read_runtime_events().unwrap().len();
            let before_current = state::current_state_lease_view().unwrap().revision;
            let intent_id = format!("intent-verify-{}", proposal.proposal_id);
            std::env::set_var("RPOTATO_TEST_VERIFICATION_APPROVAL_FAULT", stage);

            let error = verify_report(&proposal.proposal_id, &verify_token).unwrap_err();
            std::env::remove_var("RPOTATO_TEST_VERIFICATION_APPROVAL_FAULT");
            assert!(error.message.contains(stage), "stage: {stage}");
            assert!(
                paths::project_transition_journal_file(&workflow.project_id, &intent_id).exists()
            );

            assert_eq!(
                crate::transition::recover_pending_source_bundles().unwrap(),
                1,
                "stage: {stage}"
            );
            let recovered = state::load_workflow(&workflow.workflow_id).unwrap();
            let current = state::current_state_lease_view().unwrap();
            let events = ledger::read_runtime_events().unwrap();
            assert_eq!(recovered.phase, "verification-started", "stage: {stage}");
            assert_eq!(
                recovered.verification_approval_state, "approved",
                "stage: {stage}"
            );
            assert!(recovered.evidence_id.is_empty(), "stage: {stage}");
            assert_eq!(current.revision, before_current + 1, "stage: {stage}");
            assert_eq!(events.len(), before_events + 3, "stage: {stage}");
            assert_eq!(
                events[before_events..]
                    .iter()
                    .map(|event| event.event_type.as_str())
                    .collect::<Vec<_>>(),
                [
                    "runtime.intent.accepted",
                    "workflow.checkpoint",
                    "patch.verification.approved",
                ],
                "stage: {stage}"
            );
            assert_eq!(
                fs::read_to_string(&target).unwrap(),
                "pub const X: i32 = 2;\n"
            );
            assert!(
                !paths::project_transition_journal_file(&workflow.project_id, &intent_id).exists()
            );
            assert_eq!(
                crate::transition::recover_pending_source_bundles().unwrap(),
                0
            );
            assert_eq!(ledger::read_runtime_events().unwrap(), events);
            clear_patch_test_env(&root);
        }
    }

    #[cfg(unix)]
    #[test]
    fn intermediate_approval_phases_cannot_resume_without_prepared_journal() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();

        let approved_root = patch_test_root("resume-approved-without-journal");
        let (approved_target, mut approved, _proposal) =
            create_pending_workflow(&approved_root, "pwd");
        approved.phase = "approved".to_string();
        approved.approval_state = "approved".to_string();
        approved = state::checkpoint_workflow(approved.clone(), approved.revision).unwrap();
        let approved_error = resume_workflow_report(&approved.workflow_id).unwrap_err();
        assert!(approved_error
            .message
            .contains("exact E0..E9 prepared journal"));
        assert_eq!(
            fs::read_to_string(&approved_target).unwrap(),
            "pub const X: i32 = 1;\n"
        );
        clear_patch_test_env(&approved_root);

        let verification_root = patch_test_root("resume-verification-without-journal");
        let (verification_target, workflow, proposal) =
            create_pending_workflow(&verification_root, "pwd");
        approve_report(&proposal.proposal_id, &proposal.approval_token, false, None).unwrap();
        let mut verification = state::load_workflow(&workflow.workflow_id).unwrap();
        verification.phase = "verification-approved".to_string();
        verification.verification_approval_state = "approved".to_string();
        verification =
            state::checkpoint_workflow(verification.clone(), verification.revision).unwrap();
        let verification_error = resume_workflow_report(&verification.workflow_id).unwrap_err();
        assert!(verification_error
            .message
            .contains("prepared verification journal"));
        assert_eq!(
            fs::read_to_string(&verification_target).unwrap(),
            "pub const X: i32 = 2;\n"
        );
        clear_patch_test_env(&verification_root);
    }

    #[test]
    fn proposal_summary_reads_preview_record() {
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

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].proposal_id, proposal_id);
        assert_eq!(summaries[0].status, "pending-approval");
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
        let (_target, workflow, proposal) = create_pending_workflow(&root, "pwd");
        let second_token = proposal.approval_token.clone();
        let proposal_id = proposal.proposal_id;
        let record = fs::read_to_string(
            paths::project_patch_proposals_dir().join(format!("{proposal_id}.txt")),
        )
        .unwrap();
        let detail = proposal_detail_for_workflow_bounded(
            &workflow,
            &proposal_id,
            MAX_PROPOSAL_RECORD_BYTES,
        )
        .unwrap();

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
    fn deny_pending_patch_is_idempotent_and_returns_stored_receipt_first() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("deny-patch-idempotent");
        let (target, workflow, _proposal) = create_pending_workflow(&root, "pwd");

        let first = deny_pending_gate(&workflow.workflow_id, "intent-outcome-0001").unwrap();
        let ledger_after_first = fs::read_to_string(paths::runtime_ledger_file()).unwrap();
        let retry = deny_pending_gate(&workflow.workflow_id, "intent-outcome-0001").unwrap();
        let ledger_after_retry = fs::read_to_string(paths::runtime_ledger_file()).unwrap();
        let cancelled = state::load_workflow(&workflow.workflow_id).unwrap();
        let source = fs::read_to_string(&target).unwrap();
        clear_patch_test_env(&root);

        assert_eq!(first.status, TuiOutcomeStatus::Succeeded);
        assert_eq!(first.code, TuiOutcomeCode::DenyPatchAccepted);
        assert_eq!(first.effect, TuiEffect::Committed);
        assert_eq!(first.safe_message, retry.safe_message);
        assert_eq!(ledger_after_first, ledger_after_retry);
        assert_eq!(cancelled.phase, "cancelled");
        assert_eq!(cancelled.failure_reason, "user-denied-patch");
        assert_eq!(cancelled.approval_state, "denied");
        assert_eq!(cancelled.verification_approval_state, "not-issued");
        assert_eq!(source, "pub const X: i32 = 1;\n");
    }

    #[test]
    fn denial_retry_requires_exact_intent_field_not_substring_match() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("deny-exact-intent-retry");
        let (_target, workflow, _proposal) = create_pending_workflow(&root, "pwd");

        deny_pending_gate(&workflow.workflow_id, "intent-deny-10").unwrap();
        let events_after_commit = ledger::read_runtime_events().unwrap();
        let conflict = deny_pending_gate(&workflow.workflow_id, "intent-deny-1").unwrap();

        assert_eq!(conflict.status, TuiOutcomeStatus::Blocked);
        assert_eq!(conflict.code, TuiOutcomeCode::DenyBlockedTerminalState);
        assert_eq!(conflict.effect, TuiEffect::NotDispatched);
        assert_eq!(ledger::read_runtime_events().unwrap(), events_after_commit);
        clear_patch_test_env(&root);
    }

    #[test]
    fn tui_workflow_resume_revalidates_lease_and_persists_exact_intent_receipt() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("tui-resume-transaction");
        let (_target, workflow, _proposal) = create_pending_workflow(&root, "pwd");
        let lease = crate::runtime::tui_selection_lease(&workflow.workflow_id).unwrap();
        let intent_id = "intent-tui-resume-exact";

        resume_workflow_for_tui(&workflow.workflow_id, intent_id, &lease).unwrap();
        let events_after_commit = ledger::read_runtime_events().unwrap();
        assert!(ledger::event_details_match(
            "workflow.resume.accepted",
            &[
                ("intent_id", intent_id),
                ("workflow_id", workflow.workflow_id.as_str())
            ],
        )
        .unwrap());

        resume_workflow_for_tui(&workflow.workflow_id, intent_id, &lease).unwrap();
        assert_eq!(ledger::read_runtime_events().unwrap(), events_after_commit);

        let error =
            resume_workflow_for_tui(&workflow.workflow_id, "intent-tui-resume-stale", &lease)
                .unwrap_err();
        assert!(is_stale_selection_error(&error));
        assert_eq!(ledger::read_runtime_events().unwrap(), events_after_commit);
        clear_patch_test_env(&root);
    }

    #[test]
    fn tui_approval_rejects_current_lease_selected_for_a_different_object() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("tui-approval-selected-object");
        let (target, workflow, proposal) = create_pending_workflow(&root, "pwd");
        let wrong_lease = crate::runtime::tui_selection_lease("workflow-unrelated").unwrap();
        let before_events = ledger::read_runtime_events().unwrap();
        let before_workflow = state::load_workflow(&workflow.workflow_id).unwrap();

        let error = match approve_for_tui(
            &proposal.proposal_id,
            &proposal.approval_token,
            "intent-tui-wrong-selected-object",
            &wrong_lease,
        ) {
            Ok(_) => panic!("wrong selected object approved a proposal"),
            Err(error) => error,
        };

        assert!(is_stale_selection_error(&error));
        assert_eq!(ledger::read_runtime_events().unwrap(), before_events);
        assert_eq!(
            state::load_workflow(&workflow.workflow_id).unwrap(),
            before_workflow
        );
        assert_eq!(
            fs::read_to_string(target).unwrap(),
            "pub const X: i32 = 1;\n"
        );
        clear_patch_test_env(&root);
    }

    #[test]
    fn resume_entrypoints_block_tampered_and_oversized_pending_approval_proposals() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        for case in ["tampered", "oversized"] {
            let root = patch_test_root(&format!("tui-resume-proposal-{case}"));
            let (_target, workflow, proposal) = create_pending_workflow(&root, "pwd");
            let lease = crate::runtime::tui_selection_lease(&workflow.workflow_id).unwrap();
            let path =
                paths::project_patch_proposals_dir().join(format!("{}.txt", proposal.proposal_id));
            let original = fs::read_to_string(&path).unwrap();
            if case == "tampered" {
                fs::write(
                    &path,
                    original.replacen(
                        &format!("workflow_id={}", workflow.workflow_id),
                        "workflow_id=workflow-unrelated",
                        1,
                    ),
                )
                .unwrap();
            } else {
                let mut oversized = original.into_bytes();
                oversized.resize(MAX_PROPOSAL_RECORD_BYTES + 1, b'x');
                fs::write(&path, oversized).unwrap();
            }
            let before_events = ledger::read_runtime_events().unwrap();
            let direct_error = resume_workflow_report(&workflow.workflow_id).unwrap_err();
            assert!(
                direct_error.message.contains("byte budget 초과")
                    || direct_error.message.contains("binding이 일치하지 않습니다")
            );
            assert_eq!(ledger::read_runtime_events().unwrap(), before_events);
            let error = resume_workflow_for_tui(
                &workflow.workflow_id,
                &format!("intent-tui-resume-{case}"),
                &lease,
            )
            .unwrap_err();
            assert!(!is_stale_selection_error(&error) || case == "tampered");
            assert_eq!(ledger::read_runtime_events().unwrap(), before_events);
            clear_patch_test_env(&root);
        }
    }

    #[test]
    fn tui_resume_revalidates_proposal_during_pending_verification_approval() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("tui-resume-verification-proposal-binding");
        let (_target, workflow, proposal) = create_pending_workflow(&root, "pwd");
        approve_report(&proposal.proposal_id, &proposal.approval_token, false, None).unwrap();
        let current = state::load_workflow(&workflow.workflow_id).unwrap();
        assert_eq!(current.phase, "pending-verification-approval");
        let lease = crate::runtime::tui_selection_lease(&workflow.workflow_id).unwrap();
        let path =
            paths::project_patch_proposals_dir().join(format!("{}.txt", proposal.proposal_id));
        let tampered = fs::read_to_string(&path).unwrap().replacen(
            &format!("workflow_id={}", workflow.workflow_id),
            "workflow_id=workflow-unrelated",
            1,
        );
        fs::write(&path, tampered).unwrap();
        let before_events = ledger::read_runtime_events().unwrap();

        assert!(resume_workflow_for_tui(
            &workflow.workflow_id,
            "intent-tui-resume-verification-tamper",
            &lease,
        )
        .is_err());
        assert_eq!(ledger::read_runtime_events().unwrap(), before_events);
        clear_patch_test_env(&root);
    }

    #[test]
    fn terminal_denial_crash_matrix_recovers_one_exact_commit() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        for point in [
            "A1-after-journal",
            "A2-after-intent",
            "A3-after-source",
            "A4-after-snapshot",
            "A5-after-pointer",
            "A6-after-ledger",
            "A7-after-current",
            "A8-after-projection",
        ] {
            let root = patch_test_root(&format!("terminal-denial-{point}"));
            let (target, workflow, _proposal) = create_pending_workflow(&root, "pwd");
            let before_events = ledger::read_runtime_events().unwrap().len();
            let before_current = state::current_state_lease_view().unwrap().revision;
            let before_workflow = workflow.revision;
            std::env::set_var("RPOTATO_TEST_TERMINAL_ACTION_FAULT", point);
            let error = match deny_pending_gate(&workflow.workflow_id, "intent-terminal-crash") {
                Ok(_) => panic!("fault must interrupt terminal transaction"),
                Err(error) => error,
            };
            assert!(error.message.contains(point));
            std::env::remove_var("RPOTATO_TEST_TERMINAL_ACTION_FAULT");

            assert_eq!(
                crate::transition::recover_pending_source_bundles().unwrap(),
                1
            );
            let terminal = state::load_workflow(&workflow.workflow_id).unwrap();
            assert_eq!(terminal.phase, "cancelled", "point: {point}");
            assert_eq!(terminal.failure_reason, "user-denied-patch");
            assert_eq!(terminal.revision, before_workflow + 1);
            assert_eq!(
                state::current_state_lease_view().unwrap().revision,
                before_current + 1
            );
            assert_eq!(
                ledger::read_runtime_events().unwrap().len(),
                before_events + 3
            );
            assert_eq!(
                fs::read_to_string(&target).unwrap(),
                "pub const X: i32 = 1;\n"
            );
            let after_events = ledger::read_runtime_events().unwrap();
            let after_current = fs::read(paths::current_state_file()).unwrap();
            assert_eq!(
                crate::transition::recover_pending_source_bundles().unwrap(),
                0
            );
            assert_eq!(ledger::read_runtime_events().unwrap(), after_events);
            assert_eq!(
                fs::read(paths::current_state_file()).unwrap(),
                after_current
            );
            clear_patch_test_env(&root);
        }
    }

    #[test]
    fn deny_pending_verification_rolls_back_exact_source_once() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("deny-verification");
        let (target, workflow, proposal) = create_pending_workflow(&root, "pwd");
        approve_report(&proposal.proposal_id, &proposal.approval_token, false, None).unwrap();

        let outcome = deny_pending_gate(&workflow.workflow_id, "intent-outcome-0001").unwrap();
        let cancelled = state::load_workflow(&workflow.workflow_id).unwrap();
        let source = fs::read_to_string(&target).unwrap();
        clear_patch_test_env(&root);

        assert_eq!(outcome.status, TuiOutcomeStatus::Succeeded);
        assert_eq!(outcome.code, TuiOutcomeCode::DenyVerificationRolledBack);
        assert_eq!(outcome.effect, TuiEffect::RolledBack);
        assert_eq!(cancelled.phase, "cancelled");
        assert_eq!(source, "pub const X: i32 = 1;\n");
    }

    #[test]
    fn deny_non_pending_and_terminal_phases_do_not_mutate_workflow() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("deny-phase-blocks");
        let (_target, mut workflow, _proposal) = create_pending_workflow(&root, "pwd");
        workflow.phase = "approved".to_string();
        workflow.approval_state = "approved".to_string();
        workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();
        let approved_revision = workflow.revision;

        let not_pending = deny_pending_gate(&workflow.workflow_id, "intent-outcome-0001").unwrap();
        let after_not_pending = state::load_workflow(&workflow.workflow_id).unwrap();
        cancel_workflow_report(&workflow.workflow_id).unwrap();
        let terminal_before = state::load_workflow(&workflow.workflow_id).unwrap();
        let terminal = deny_pending_gate(&workflow.workflow_id, "intent-outcome-0002").unwrap();
        let terminal_after = state::load_workflow(&workflow.workflow_id).unwrap();
        clear_patch_test_env(&root);

        assert_eq!(not_pending.code, TuiOutcomeCode::DenyBlockedNotPending);
        assert_eq!(not_pending.effect, TuiEffect::NotDispatched);
        assert_eq!(after_not_pending.revision, approved_revision);
        assert_eq!(terminal.code, TuiOutcomeCode::DenyBlockedTerminalState);
        assert_eq!(terminal.effect, TuiEffect::NotDispatched);
        assert_eq!(terminal_before, terminal_after);
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
        let record = load_proposal_record(
            &proposal.proposal_id,
            &paths::project_patch_proposals_dir().join(format!("{}.txt", proposal.proposal_id)),
        )
        .unwrap();
        let rollback_path = rollback_path_for_record(&record).unwrap();
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
    fn source_replace_fault_windows_recover_committed_prepared_bytes() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        for point in ["after-guard", "after-install"] {
            let root = patch_test_root(&format!("source-fault-{point}"));
            let (target, _workflow, proposal) = create_pending_workflow(&root, "pwd");
            std::env::set_var("RPOTATO_TEST_SOURCE_REPLACE_FAULT", point);
            let error =
                approve_report(&proposal.proposal_id, &proposal.approval_token, false, None)
                    .unwrap_err();
            std::env::remove_var("RPOTATO_TEST_SOURCE_REPLACE_FAULT");
            let repair_required = crate::transition::recover_pending_source_bundles().unwrap_err();
            assert!(
                repair_required
                    .message
                    .contains("projection.repair-required"),
                "point: {point}, error: {}",
                repair_required.message
            );
            assert_eq!(
                crate::transition::recover_pending_source_bundles().unwrap(),
                1
            );
            let source = fs::read_to_string(&target).unwrap();
            clear_patch_test_env(&root);
            assert!(matches!(error.code, 1 | 3), "point: {point}");
            assert_eq!(source, "pub const X: i32 = 2;\n", "point: {point}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn source_recovery_rejects_parent_symlink_replacement_before_any_event() {
        use std::os::unix::fs::symlink;

        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("source-parent-symlink-race");
        let (target, workflow, proposal) = create_pending_workflow(&root, "pwd");
        let before_events = ledger::read_runtime_events().unwrap();
        std::env::set_var("RPOTATO_TEST_APPROVAL_TRANSACTION_FAULT", "T1");
        approve_report(&proposal.proposal_id, &proposal.approval_token, false, None).unwrap_err();
        std::env::remove_var("RPOTATO_TEST_APPROVAL_TRANSACTION_FAULT");

        let original_parent = target.parent().unwrap().with_file_name("src-original");
        fs::rename(target.parent().unwrap(), &original_parent).unwrap();
        let outside = root.join("outside");
        fs::create_dir_all(&outside).unwrap();
        let outside_target = outside.join("lib.rs");
        fs::write(&outside_target, "outside sentinel\n").unwrap();
        symlink(&outside, target.parent().unwrap()).unwrap();

        let error = crate::transition::recover_pending_source_bundles().unwrap_err();

        assert!(error.message.contains("parent traversal"));
        assert_eq!(
            fs::read_to_string(&outside_target).unwrap(),
            "outside sentinel\n"
        );
        assert_eq!(
            fs::read_to_string(original_parent.join("lib.rs")).unwrap(),
            "pub const X: i32 = 1;\n"
        );
        assert_eq!(ledger::read_runtime_events().unwrap(), before_events);
        assert!(paths::project_transition_journal_file(
            &workflow.project_id,
            &format!("intent-approve-{}", proposal.proposal_id),
        )
        .exists());
        clear_patch_test_env(&root);
    }

    #[cfg(unix)]
    #[test]
    fn source_recovery_rejects_rollback_parent_symlink_before_any_event() {
        use std::os::unix::fs::symlink;

        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = patch_test_root("rollback-parent-symlink-race");
        let (target, workflow, proposal) = create_pending_workflow(&root, "pwd");
        let record = load_proposal_record(
            &proposal.proposal_id,
            &paths::project_patch_proposals_dir().join(format!("{}.txt", proposal.proposal_id)),
        )
        .unwrap();
        let rollback = rollback_path_for_record(&record).unwrap();
        let before_events = ledger::read_runtime_events().unwrap();
        std::env::set_var("RPOTATO_TEST_APPROVAL_TRANSACTION_FAULT", "T1");
        approve_report(&proposal.proposal_id, &proposal.approval_token, false, None).unwrap_err();
        std::env::remove_var("RPOTATO_TEST_APPROVAL_TRANSACTION_FAULT");

        let outside = root.join("outside-rollback");
        fs::create_dir_all(&outside).unwrap();
        let outside_sentinel = outside.join("sentinel.txt");
        fs::write(&outside_sentinel, "outside sentinel\n").unwrap();
        fs::create_dir_all(rollback.parent().unwrap().parent().unwrap()).unwrap();
        symlink(&outside, rollback.parent().unwrap()).unwrap();

        let error = crate::transition::recover_pending_source_bundles().unwrap_err();

        assert!(error.message.contains("rollback parent traversal"));
        assert_eq!(
            fs::read_to_string(&outside_sentinel).unwrap(),
            "outside sentinel\n"
        );
        assert!(!outside.join(rollback.file_name().unwrap()).exists());
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "pub const X: i32 = 1;\n"
        );
        assert_eq!(ledger::read_runtime_events().unwrap(), before_events);
        assert!(paths::project_transition_journal_file(
            &workflow.project_id,
            &format!("intent-approve-{}", proposal.proposal_id),
        )
        .exists());
        clear_patch_test_env(&root);
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
        let rollback_path = rollback_path_for_record(&record).unwrap();
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
            let (target, workflow, proposal) = create_pending_workflow(&root, "cargo test");

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

            clear_patch_test_env(&root);

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
        let mut skill = crate::skill::SkillRuntimeState::new("small-patch", "explicit").unwrap();
        for state in [
            crate::skill::SkillState::ContextReady,
            crate::skill::SkillState::ModelRequested,
            crate::skill::SkillState::ActionRecorded,
            crate::skill::SkillState::AwaitingApproval,
        ] {
            skill.transition(state).unwrap();
        }
        for hook in [
            "session_start",
            "user_request_received",
            "pre_context_pack",
            "post_context_pack",
            "pre_model_request",
            "post_model_response",
            "pre_action_parse",
            "post_action_parse",
            "pre_tool_call",
            "post_tool_result",
        ] {
            skill.record_hook(hook).unwrap();
        }
        skill.record_evidence("diff_review");
        skill.store_in_workflow(&mut workflow);
        workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();
        (target, workflow, proposal)
    }

    fn create_prepared_pending_workflow(
        root: &Path,
        verification: &str,
    ) -> (PathBuf, state::WorkflowRecord, WorkflowProposal) {
        let (target, mut workflow, proposal) = create_pending_workflow(root, verification);
        let mut skill = crate::skill::SkillRuntimeState::new("small-patch", "explicit").unwrap();
        for skill_state in [
            crate::skill::SkillState::ContextReady,
            crate::skill::SkillState::ModelRequested,
            crate::skill::SkillState::ActionRecorded,
            crate::skill::SkillState::AwaitingApproval,
        ] {
            skill.transition(skill_state).unwrap();
        }
        skill.store_in_workflow(&mut workflow);
        state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();
        (target, workflow, proposal)
    }
}
