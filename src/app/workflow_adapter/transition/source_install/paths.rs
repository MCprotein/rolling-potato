use super::super::*;

pub(crate) fn source_identity_v1(
    dev: u64,
    ino: u64,
    content_sha256: &str,
) -> Result<String, AppError> {
    let content_hash = decode_sha256(content_sha256)?;
    let mut identity = b"rpotato.source-identity/v1".to_vec();
    append_tlv(&mut identity, 0x01, b"unix")?;
    append_tlv(&mut identity, 0x10, &dev.to_be_bytes())?;
    append_tlv(&mut identity, 0x11, &ino.to_be_bytes())?;
    append_tlv(&mut identity, 0x20, &content_hash)?;
    Ok(sha256_bytes(&identity))
}

pub(crate) fn resolve_prepared_project_path(path: &PreparedPath) -> Result<PathBuf, AppError> {
    validate_prepared_path(path, path.expected_type == "file")?;
    let root = paths::project_root()
        .canonicalize()
        .map_err(|err| AppError::blocked(format!("project root canonicalize 실패: {err}")))?;
    let relative = Path::new(&path.path);
    if relative.is_absolute() {
        return Err(AppError::blocked("prepared project path absolute 차단"));
    }
    Ok(root.join(relative))
}

pub(crate) fn source_install_rollback_path(
    intent_id: &str,
    proposal_id: &str,
    target: &Path,
    before_sha256: &str,
    proposed_sha256: &str,
) -> Result<PathBuf, AppError> {
    validate_ascii_id(intent_id, "intent")?;
    validate_ascii_id(proposal_id, "proposal")?;
    if !is_sha256(before_sha256) || !is_sha256(proposed_sha256) {
        return Err(AppError::blocked("source rollback hash 형식 불일치"));
    }
    let root = paths::project_root()
        .canonicalize()
        .map_err(|err| AppError::blocked(format!("project root canonicalize 실패: {err}")))?;
    let target = target
        .canonicalize()
        .map_err(|err| AppError::blocked(format!("source target canonicalize 실패: {err}")))?;
    let target = stored_project_path(&root, &target)?;
    let source_key = source_key_v1(intent_id, &target, before_sha256, proposed_sha256);
    Ok(root.join(format!(
        ".rpotato/patches/{proposal_id}/{intent_id}-{source_key}.rollback"
    )))
}
