use super::super::*;

pub(super) struct ApprovalSourcePreflight {
    pub(super) relative_path: String,
    pub(super) before: String,
    pub(super) source_install: transition::SourceInstallV1,
}

pub(super) fn prepare_approval_source(
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
