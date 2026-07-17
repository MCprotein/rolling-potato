use super::*;

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
pub(super) fn summary_from_path(path: &Path) -> Result<PatchProposalSummary, AppError> {
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

pub(super) fn rollback_path_for_record(record: &ProposalRecord) -> Result<PathBuf, AppError> {
    let target = resolve_target_for("patch rollback path", &record.relative_path)?;
    let legacy = transition::source_install_rollback_path(
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

pub(super) fn validate_applied_proposal(record: &ProposalRecord) -> Result<ApplyResult, AppError> {
    let target = resolve_target_for("patch verification", &record.relative_path)?;
    let current = fs::read(&target.absolute_path).map_err(|err| {
        AppError::blocked(format!(
            "patch verification source reread 실패: {} ({err})",
            target.relative_path
        ))
    })?;
    let current_sha256 = sha256_bytes(&current);
    application_domain::validate_applied_source(
        &target.relative_path,
        &current_sha256,
        &record.proposed_sha256,
    )?;
    let rollback_path = rollback_path_for_record(record)?;
    let rollback = fs::read(&rollback_path).map_err(|err| {
        AppError::blocked(format!(
            "patch verification 차단\n- 이유: rollback record를 읽지 못했습니다.\n- path: {}\n- error: {err}",
            rollback_path.display()
        ))
    })?;
    application_domain::validate_applied_rollback(
        &sha256_bytes(&rollback),
        &record.original_sha256,
    )?;
    Ok(ApplyResult {
        relative_path: target.relative_path,
        original_sha256: record.original_sha256.clone(),
        applied_sha256: current_sha256,
        rollback_path,
    })
}

pub(super) fn load_proposal_record(
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
        RecordParse::Canonical(record) => Ok(*record),
        RecordParse::LegacyMigration { scrubbed } => {
            state::atomic_replace_bytes(proposal_path, scrubbed.as_bytes())?;
            Err(AppError::blocked(
                "legacy proposal migration 완료\n- plaintext token을 hash-only로 atomic scrub했습니다.\n- 동작: 기존 binding은 폐기하고 canonical workflow preview를 다시 생성하세요.",
            ))
        }
    }
}
pub(super) fn validate_token_hash(
    expected_hash: &str,
    token: &str,
    record: &ProposalRecord,
) -> Result<(), AppError> {
    if approval_domain::matches_hash(expected_hash, token) {
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

pub(super) fn dry_run_approval_report(
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
