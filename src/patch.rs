use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProposalRecord {
    proposal_id: String,
    approval_token: String,
    relative_path: String,
    original_sha256: String,
    proposed_sha256: String,
    proposed_content: String,
    proposal_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ApplyResult {
    relative_path: String,
    original_sha256: String,
    applied_sha256: String,
    rollback_path: PathBuf,
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
    let preview = build_preview(path, find, replace)?;
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
        "patch preview\n- status: diff-ready\n- path: {}\n- proposal id: {}\n- replacements: {}\n- original sha256: {}\n- proposed sha256: {}\n- approval required: yes\n- approval token: {}\n- approval command: rpotato patch approve {} --token {}\n- dry-run command: rpotato patch approve {} --token {} --dry-run\n- proposal record: {}\n- write gate: diff-before-write\n- ledger event: {}\n- boundary: 대상 파일은 수정하지 않았습니다. approve에 --dry-run을 붙이면 gate만 확인하고, dry-run 없이 실행하면 승인된 patch를 적용합니다.\n- diff:\n{}",
        preview.relative_path,
        preview.proposal_id,
        preview.replacements,
        preview.original_sha256,
        preview.proposed_sha256,
        preview.approval_token,
        preview.proposal_id,
        preview.approval_token,
        preview.proposal_id,
        preview.approval_token,
        preview.proposal_path.display(),
        event_id,
        preview.diff
    ))
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
    validate_approval_token(&record, token)?;

    let verification_plan = verify_command.map(build_verification_plan).transpose()?;

    if dry_run {
        return dry_run_approval_report(&record, verify_command);
    }

    let apply = apply_proposal(&record)?;
    let verification = if let Some(plan) = verification_plan.as_ref() {
        let verification = run_verification(plan);
        if !verification.passed() {
            let rollback_status = restore_from_rollback(&record, &apply.rollback_path);
            let event_id = state::record_event(
                "patch.verification.failed_rolled_back",
                "patch verification failed and rollback was attempted",
                &format!(
                    "proposal_id={} path={} command={} exit_code={} rollback={}",
                    record.proposal_id,
                    record.relative_path,
                    ledger::redact_text(&verification.command),
                    verification.exit_code,
                    rollback_status
                ),
            )?;
            return Err(AppError::blocked(format!(
                "patch approve\n- status: verification-failed-rolled-back\n- proposal id: {}\n- path: {}\n- approval token: accepted\n- original sha256: {}\n- attempted sha256: {}\n- rollback record: {}\n- rollback status: {}\n- verification command: {}\n- verification exit code: {}\n- verification stdout: {}\n- verification stderr: {}\n- ledger event: {}\n- boundary: patch 적용 후 verification command가 실패해 rollback을 시도했고, 성공으로 보고하지 않습니다.",
                record.proposal_id,
                record.relative_path,
                apply.original_sha256,
                apply.applied_sha256,
                apply.rollback_path.display(),
                rollback_status,
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
        "patch.applied",
        "approved patch applied",
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
    let recorded_id = required_record_value(&contents, "proposal_id", proposal_path)?;
    if recorded_id != proposal_id {
        return Err(AppError::blocked(format!(
            "patch approve 차단\n- 이유: proposal id가 record와 일치하지 않습니다.\n- requested: {}\n- recorded: {}",
            proposal_id, recorded_id
        )));
    }
    let proposed_sha256 = required_record_value(&contents, "proposed_sha256", proposal_path)?;
    let proposed_content_hex =
        required_record_value(&contents, "proposed_content_hex", proposal_path).map_err(|_| {
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

    Ok(ProposalRecord {
        proposal_id: recorded_id,
        approval_token: required_record_value(&contents, "approval_token", proposal_path)?,
        relative_path: required_record_value(&contents, "path", proposal_path)?,
        original_sha256: required_record_value(&contents, "original_sha256", proposal_path)?,
        proposed_sha256,
        proposed_content,
        proposal_path: proposal_path.to_path_buf(),
    })
}

fn required_record_value(
    record: &str,
    key: &str,
    proposal_path: &Path,
) -> Result<String, AppError> {
    proposal_record_value(record, key).ok_or_else(|| {
        AppError::blocked(format!(
            "patch approve 차단\n- 이유: proposal record에 {} 값이 없습니다.\n- path: {}",
            key,
            proposal_path.display()
        ))
    })
}

fn validate_approval_token(record: &ProposalRecord, token: &str) -> Result<(), AppError> {
    if record.approval_token == token {
        return Ok(());
    }

    Err(AppError::blocked(format!(
        "patch approve 차단\n- 이유: approval token 불일치\n- proposal id: {}\n- approval prompt: 사용자 승인 필요",
        record.proposal_id
    )))
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
    if current_sha256 != record.original_sha256 {
        return Err(AppError::blocked(format!(
            "patch approve 차단\n- 이유: 대상 파일이 preview 이후 변경되었습니다.\n- path: {}\n- expected original sha256: {}\n- current sha256: {}\n- 동작: patch preview를 다시 생성하세요.",
            target.relative_path, record.original_sha256, current_sha256
        )));
    }

    let rollback_path = record
        .proposal_path
        .with_file_name(format!("{}.rollback", record.proposal_id));
    fs::write(&rollback_path, current.as_bytes()).map_err(|err| {
        AppError::runtime(format!(
            "patch rollback record를 쓰지 못했습니다: {} ({err})",
            rollback_path.display()
        ))
    })?;

    if let Err(err) = fs::write(&target.absolute_path, record.proposed_content.as_bytes()) {
        let rollback_status = restore_content(&target.absolute_path, &current);
        return Err(AppError::blocked(format!(
            "patch approve 실패\n- 이유: 대상 파일 쓰기에 실패했습니다.\n- path: {}\n- error: {}\n- rollback status: {}",
            target.relative_path, err, rollback_status
        )));
    }

    let applied = fs::read_to_string(&target.absolute_path).map_err(|err| {
        let rollback_status = restore_content(&target.absolute_path, &current);
        AppError::blocked(format!(
            "patch approve 실패\n- 이유: 적용 후 대상 파일을 읽지 못했습니다.\n- path: {}\n- error: {}\n- rollback status: {}",
            target.relative_path, err, rollback_status
        ))
    })?;
    let applied_sha256 = sha256_text(&applied);
    if applied_sha256 != record.proposed_sha256 {
        let rollback_status = restore_content(&target.absolute_path, &current);
        return Err(AppError::blocked(format!(
            "patch approve 실패\n- 이유: 적용 후 SHA-256이 proposal과 일치하지 않습니다.\n- path: {}\n- expected proposed sha256: {}\n- applied sha256: {}\n- rollback status: {}",
            target.relative_path, record.proposed_sha256, applied_sha256, rollback_status
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
    let decision = policy::classify_command(trimmed)?;
    if decision.decision != Decision::Allow {
        return Err(AppError::blocked(format!(
            "patch approve 차단\n- 이유: verification command policy가 allow가 아닙니다.\n- command: {}\n- decision: {}\n- class: {}\n- approval prompt: {}",
            ledger::redact_text(trimmed),
            read_decision_label(decision.decision),
            decision.command_class,
            decision.approval_prompt
        )));
    }
    let argv = split_simple_command(trimmed)?;
    Ok(VerificationPlan {
        command: trimmed.to_string(),
        argv,
    })
}

fn split_simple_command(command: &str) -> Result<Vec<String>, AppError> {
    if command.chars().any(|ch| {
        matches!(
            ch,
            ';' | '|' | '&' | '<' | '>' | '`' | '$' | '\n' | '\r' | '"' | '\''
        )
    }) {
        return Err(AppError::blocked(
            "patch approve 차단\n- 이유: verification command는 shell 없이 실행되는 단순 argv만 허용합니다.",
        ));
    }
    let argv = command
        .split_whitespace()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if argv.is_empty() {
        return Err(AppError::usage(
            "patch approve verification command는 비어 있을 수 없습니다.",
        ));
    }
    Ok(argv)
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

fn restore_from_rollback(record: &ProposalRecord, rollback_path: &Path) -> String {
    let target = match resolve_target_for("patch rollback", &record.relative_path) {
        Ok(target) => target,
        Err(err) => return format!("restore-failed: {}", err.message),
    };
    let original = match fs::read_to_string(rollback_path) {
        Ok(contents) => contents,
        Err(err) => return format!("restore-failed: rollback record read error: {err}"),
    };
    restore_content(&target.absolute_path, &original)
}

fn restore_content(target: &Path, contents: &str) -> String {
    match fs::write(target, contents.as_bytes()) {
        Ok(()) => "restored".to_string(),
        Err(err) => format!("restore-failed: {err}"),
    }
}

fn build_preview(path: &str, find: &str, replace: &str) -> Result<PatchPreview, AppError> {
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
    let proposal_id = format!(
        "patch-proposal-{}",
        &sha256_text(&format!(
            "{}\n{}\n{}",
            target.relative_path, original_sha256, proposed_sha256
        ))[..16]
    );
    let approval_token = sha256_text(&format!("{proposal_id}\n{diff}"))[..24].to_string();
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
    fs::write(
        &preview.proposal_path,
        format!(
            "record_version=2\nproposal_id={}\npath={}\napproval_token={}\noriginal_sha256={}\nproposed_sha256={}\nreplacements={}\ncontent_encoding=utf8-hex\nproposed_content_hex={}\n\n{}\n",
            preview.proposal_id,
            preview.relative_path,
            preview.approval_token,
            preview.original_sha256,
            preview.proposed_sha256,
            preview.replacements,
            encode_hex_text(&preview.proposed_content),
            preview.diff
        ),
    )
    .map_err(|err| {
        AppError::runtime(format!(
            "patch proposal record를 쓰지 못했습니다: {} ({err})",
            preview.proposal_path.display()
        ))
    })
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

fn proposal_record_value(record: &str, key: &str) -> Option<String> {
    record.lines().find_map(|line| {
        let (candidate, value) = line.split_once('=')?;
        if candidate == key {
            Some(value.to_string())
        } else {
            None
        }
    })
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
        assert!(report.contains("status: diff-ready"));
        assert!(report.contains("-    1"));
        assert!(report.contains("+    2"));
        assert!(report.contains("approval command: rpotato patch approve"));
    }

    #[test]
    fn approve_accepts_recorded_token_in_dry_run() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root =
            std::env::temp_dir().join(format!("rpotato-patch-approve-test-{}", std::process::id()));
        let project_root = root.join("project");
        fs::create_dir_all(project_root.join("src")).unwrap();
        fs::write(project_root.join("src/lib.rs"), "pub const X: i32 = 1;\n").unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let report = preview_report("src/lib.rs", "1", "2").unwrap();
        let proposal_id = report_value(&report, "proposal id").unwrap();
        let token = report_value(&report, "approval token").unwrap();
        let approval = approve_report(&proposal_id, &token, true, None).unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");

        assert!(approval.contains("status: gate-passed"));
        assert!(approval.contains("boundary: approval gate만 확인했습니다"));
    }

    #[test]
    fn approve_applies_recorded_patch() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root =
            std::env::temp_dir().join(format!("rpotato-patch-apply-test-{}", std::process::id()));
        let project_root = root.join("project");
        fs::create_dir_all(project_root.join("src")).unwrap();
        let target = project_root.join("src/lib.rs");
        fs::write(&target, "pub const X: i32 = 1;\n").unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let report = preview_report("src/lib.rs", "1", "2").unwrap();
        let proposal_id = report_value(&report, "proposal id").unwrap();
        let token = report_value(&report, "approval token").unwrap();
        let approval = approve_report(&proposal_id, &token, false, None).unwrap();
        let contents = fs::read_to_string(&target).unwrap();
        let rollback_path = project_root
            .join(".rpotato")
            .join("patch-proposals")
            .join(format!("{proposal_id}.rollback"));

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");

        assert_eq!(contents, "pub const X: i32 = 2;\n");
        assert!(rollback_path.exists());
        assert!(approval.contains("status: applied"));
        assert!(approval.contains("verification status: not-requested"));
    }

    #[test]
    fn approve_blocks_changed_target_before_apply() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-patch-changed-target-test-{}",
            std::process::id()
        ));
        let project_root = root.join("project");
        fs::create_dir_all(project_root.join("src")).unwrap();
        let target = project_root.join("src/lib.rs");
        fs::write(&target, "pub const X: i32 = 1;\n").unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let report = preview_report("src/lib.rs", "1", "2").unwrap();
        let proposal_id = report_value(&report, "proposal id").unwrap();
        let token = report_value(&report, "approval token").unwrap();
        fs::write(&target, "pub const X: i32 = 3;\n").unwrap();
        let err = approve_report(&proposal_id, &token, false, None).unwrap_err();
        let contents = fs::read_to_string(&target).unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");

        assert_eq!(err.code, 3);
        assert!(err.message.contains("preview 이후 변경"));
        assert_eq!(contents, "pub const X: i32 = 3;\n");
    }

    #[test]
    fn approve_blocks_disallowed_verification_before_apply() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-patch-verify-block-test-{}",
            std::process::id()
        ));
        let project_root = root.join("project");
        fs::create_dir_all(project_root.join("src")).unwrap();
        let target = project_root.join("src/lib.rs");
        fs::write(&target, "pub const X: i32 = 1;\n").unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let report = preview_report("src/lib.rs", "1", "2").unwrap();
        let proposal_id = report_value(&report, "proposal id").unwrap();
        let token = report_value(&report, "approval token").unwrap();
        let err = approve_report(&proposal_id, &token, false, Some("echo hi")).unwrap_err();
        let contents = fs::read_to_string(&target).unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");

        assert_eq!(err.code, 3);
        assert!(err.message.contains("verification command policy"));
        assert_eq!(contents, "pub const X: i32 = 1;\n");
    }

    #[cfg(unix)]
    #[test]
    fn approve_runs_allowed_verification_command() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-patch-verify-run-test-{}",
            std::process::id()
        ));
        let project_root = root.join("project");
        fs::create_dir_all(project_root.join("src")).unwrap();
        let target = project_root.join("src/lib.rs");
        fs::write(&target, "pub const X: i32 = 1;\n").unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let report = preview_report("src/lib.rs", "1", "2").unwrap();
        let proposal_id = report_value(&report, "proposal id").unwrap();
        let token = report_value(&report, "approval token").unwrap();
        let approval = approve_report(&proposal_id, &token, false, Some("pwd")).unwrap();
        let contents = fs::read_to_string(&target).unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");

        assert_eq!(contents, "pub const X: i32 = 2;\n");
        assert!(approval.contains("verification status: passed"));
        assert!(approval.contains("verification exit code: 0"));
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
}
