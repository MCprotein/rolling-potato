use super::*;

#[cfg(unix)]
pub(crate) fn prepare_source_install_v1(
    intent_id: &str,
    proposal_id: &str,
    target: &Path,
    before: &[u8],
    proposed: &[u8],
) -> Result<SourceInstallV1, AppError> {
    validate_ascii_id(intent_id, "intent")?;
    validate_ascii_id(proposal_id, "proposal")?;
    let canonical_root = paths::project_root()
        .canonicalize()
        .map_err(|err| AppError::blocked(format!("project root canonicalize 실패: {err}")))?;
    let canonical_target = target
        .canonicalize()
        .map_err(|err| AppError::blocked(format!("source target canonicalize 실패: {err}")))?;
    let target_parent = canonical_target
        .parent()
        .ok_or_else(|| AppError::blocked("source target parent 누락"))?;
    let canonical_parent = target_parent
        .canonicalize()
        .map_err(|err| AppError::blocked(format!("source parent canonicalize 실패: {err}")))?;
    if canonical_target.parent() != Some(canonical_parent.as_path()) {
        return Err(AppError::blocked(
            "source target/parent canonical binding 불일치",
        ));
    }
    let metadata = fs::symlink_metadata(&canonical_target)
        .map_err(|err| AppError::blocked(format!("source target metadata 실패: {err}")))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(AppError::blocked("source target type 불일치"));
    }
    let parent_metadata = fs::metadata(&canonical_parent)
        .map_err(|err| AppError::blocked(format!("source parent metadata 실패: {err}")))?;
    use std::os::unix::fs::MetadataExt;
    if metadata.dev() != parent_metadata.dev() {
        return Err(AppError::blocked("source target/parent filesystem 불일치"));
    }
    if !process_can_preserve_ownership(metadata.uid(), metadata.gid()) {
        return Err(AppError::blocked(format!(
            "source ownership 보존 차단\n- code: source-install.ownership-unsupported\n- target uid: {}\n- target gid: {}\n- 동작: journal/temp/guard/rollback/target 변경 없음",
            metadata.uid(),
            metadata.gid()
        )));
    }
    let actual = fs::read(&canonical_target)
        .map_err(|err| AppError::blocked(format!("source target read 실패: {err}")))?;
    if actual != before {
        return Err(AppError::blocked(
            "source before bytes가 canonical target과 다릅니다.",
        ));
    }
    let target_path = stored_project_path(&canonical_root, &canonical_target)?;
    let parent_path = stored_project_path(&canonical_root, &canonical_parent)?;
    let basename = canonical_target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::blocked("source target basename UTF-8 검증 실패"))?
        .to_string();
    validate_basename(&basename)?;
    let before_sha256 = sha256_bytes(before);
    let proposed_sha256 = sha256_bytes(proposed);
    let before_byte_length = checked_len(before, "before blob")?;
    let proposed_byte_length = checked_len(proposed, "proposed blob")?;
    let source_key = source_key_v1(intent_id, &target_path, &before_sha256, &proposed_sha256);
    let rollback_path = format!(".rpotato/patches/{proposal_id}/{intent_id}-{source_key}.rollback");
    let install_basename = format!(".{basename}.rpotato-install-{intent_id}-{proposed_sha256}.tmp");
    let guard_basename = format!(".{basename}.rpotato-guard-{intent_id}-{before_sha256}");
    validate_basename(&install_basename)?;
    validate_basename(&guard_basename)?;
    let install_path = join_stored(&parent_path, &install_basename);
    let guard_path = join_stored(&parent_path, &guard_basename);
    let install_absolute = canonical_parent.join(&install_basename);
    let guard_absolute = canonical_parent.join(&guard_basename);
    for path in [&install_absolute, &guard_absolute] {
        match fs::symlink_metadata(path) {
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Ok(_) => {
                return Err(AppError::blocked(
                    "source create-new sibling이 이미 존재합니다.",
                ))
            }
            Err(err) => {
                return Err(AppError::blocked(format!(
                    "source create-new sibling preflight 실패: {err}"
                )))
            }
        }
    }
    let rollback_parent = format!(".rpotato/patches/{proposal_id}");
    let rollback_basename = format!("{intent_id}-{source_key}.rollback");
    let mode = metadata.mode();
    let readonly = metadata.permissions().readonly();
    let owner = format!("uid:{}:gid:{}", metadata.uid(), metadata.gid());
    let expected_identity = source_identity_v1(metadata.dev(), metadata.ino(), &before_sha256)?;
    let plan = SourceInstallV1 {
        schema_version: 1,
        source_key,
        target: PreparedPath {
            namespace: "project".to_string(),
            path: target_path,
            parent: parent_path.clone(),
            basename,
            expected_type: "file".to_string(),
            expected_identity: Some(expected_identity),
        },
        before_blob: PreparedBlob {
            blob_id: format!("source-before-{before_sha256}"),
            member_path: format!(".rpotato/transition-blobs/{before_sha256}.before"),
            sha256: before_sha256.clone(),
            byte_length: before_byte_length,
        },
        proposed_blob: PreparedBlob {
            blob_id: format!("source-proposed-{proposed_sha256}"),
            member_path: format!(".rpotato/transition-blobs/{proposed_sha256}.proposed"),
            sha256: proposed_sha256.clone(),
            byte_length: proposed_byte_length,
        },
        rollback_final: PreparedPath {
            namespace: "project".to_string(),
            path: rollback_path,
            parent: rollback_parent,
            basename: rollback_basename,
            expected_type: "absent".to_string(),
            expected_identity: None,
        },
        install_temp: PreparedPath {
            namespace: "project".to_string(),
            path: install_path,
            parent: parent_path.clone(),
            basename: install_basename,
            expected_type: "absent".to_string(),
            expected_identity: None,
        },
        guard_path: PreparedPath {
            namespace: "project".to_string(),
            path: guard_path,
            parent: parent_path,
            basename: guard_basename,
            expected_type: "absent".to_string(),
            expected_identity: None,
        },
        before_sha256,
        before_byte_length,
        proposed_sha256,
        proposed_byte_length,
        permissions: SourcePermissions {
            before_readonly: readonly,
            install_readonly: readonly,
            before_mode: mode,
            install_mode: mode,
        },
        ownership: SourceOwnership {
            before_owner: owner.clone(),
            install_owner: owner,
        },
        platform: "unix".to_string(),
        unix_metadata: UnixSourceMetadata {
            before_mode: mode,
            install_mode: mode,
            before_uid: metadata.uid(),
            before_gid: metadata.gid(),
            install_uid: metadata.uid(),
            install_gid: metadata.gid(),
            before_dev: metadata.dev(),
            before_ino: metadata.ino(),
        },
        operations: SOURCE_INSTALL_OPERATIONS
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
    };
    validate_source_install_v1(&plan)?;
    crate::app::workflow_adapter::state::validate_source_install_initial_admission(&plan)?;
    Ok(plan)
}

#[cfg(unix)]
fn process_can_preserve_ownership(uid: u32, gid: u32) -> bool {
    unsafe extern "C" {
        fn geteuid() -> u32;
        fn getegid() -> u32;
        fn getgroups(size: i32, list: *mut u32) -> i32;
    }

    // SAFETY: these process identity queries have no pointer preconditions.
    let effective_uid = unsafe { geteuid() };
    if effective_uid == 0 {
        return true;
    }
    if uid != effective_uid {
        return false;
    }
    // SAFETY: getegid has no pointer preconditions.
    if gid == unsafe { getegid() } {
        return true;
    }
    // SAFETY: a null list with size zero requests the supplementary group count.
    let count = unsafe { getgroups(0, std::ptr::null_mut()) };
    if count <= 0 {
        return false;
    }
    let Ok(count_usize) = usize::try_from(count) else {
        return false;
    };
    let mut groups = vec![0_u32; count_usize];
    // SAFETY: `groups` has `count` writable gid slots.
    let written = unsafe { getgroups(count, groups.as_mut_ptr()) };
    written == count && groups.contains(&gid)
}

#[cfg(not(unix))]
pub(crate) fn prepare_source_install_v1(
    _intent_id: &str,
    _proposal_id: &str,
    _target: &Path,
    _before: &[u8],
    _proposed: &[u8],
) -> Result<SourceInstallV1, AppError> {
    Err(AppError::blocked(format!(
        "source install 차단\n- code: source-install.unsupported-platform\n- platform: {}\n- 지원 범위: v0.34.0 source installation은 Unix만 지원합니다.\n- 동작: journal/temp/guard/rollback/target 변경 없음",
        std::env::consts::OS
    )))
}

pub(crate) fn validate_source_install_v1(plan: &SourceInstallV1) -> Result<(), AppError> {
    if plan.schema_version != 1 || plan.platform != "unix" {
        return Err(AppError::blocked(
            "source_install_v1 schema/platform 불일치",
        ));
    }
    if !is_sha256(&plan.source_key)
        || !is_sha256(&plan.before_sha256)
        || !is_sha256(&plan.proposed_sha256)
        || plan.before_blob.sha256 != plan.before_sha256
        || plan.proposed_blob.sha256 != plan.proposed_sha256
        || plan.before_blob.byte_length != plan.before_byte_length
        || plan.proposed_blob.byte_length != plan.proposed_byte_length
        || plan.before_byte_length > MAX_SOURCE_BLOB_BYTES as u64
        || plan.proposed_byte_length > MAX_SOURCE_BLOB_BYTES as u64
    {
        return Err(AppError::blocked(
            "source_install_v1 hash/blob binding 불일치",
        ));
    }
    validate_prepared_path(&plan.target, true)?;
    validate_prepared_path(&plan.rollback_final, false)?;
    validate_prepared_path(&plan.install_temp, false)?;
    validate_prepared_path(&plan.guard_path, false)?;
    if plan.target.parent != plan.install_temp.parent
        || plan.target.parent != plan.guard_path.parent
        || plan.install_temp.path == plan.guard_path.path
    {
        return Err(AppError::blocked(
            "source_install_v1 same-parent binding 불일치",
        ));
    }
    let expected_operations = SOURCE_INSTALL_OPERATIONS
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<_>>();
    if plan.operations != expected_operations {
        return Err(AppError::blocked(
            "source_install_v1 operation oracle 불일치",
        ));
    }
    if plan.permissions.before_readonly != plan.permissions.install_readonly
        || plan.permissions.before_mode != plan.permissions.install_mode
        || plan.unix_metadata.before_mode != plan.unix_metadata.install_mode
        || plan.unix_metadata.before_uid != plan.unix_metadata.install_uid
        || plan.unix_metadata.before_gid != plan.unix_metadata.install_gid
        || plan.permissions.before_mode != plan.unix_metadata.before_mode
        || plan.permissions.install_mode != plan.unix_metadata.install_mode
        || plan.ownership.before_owner
            != format!(
                "uid:{}:gid:{}",
                plan.unix_metadata.before_uid, plan.unix_metadata.before_gid
            )
        || plan.ownership.install_owner
            != format!(
                "uid:{}:gid:{}",
                plan.unix_metadata.install_uid, plan.unix_metadata.install_gid
            )
    {
        return Err(AppError::blocked(
            "source_install_v1 metadata binding 불일치",
        ));
    }
    let expected_identity = source_identity_v1(
        plan.unix_metadata.before_dev,
        plan.unix_metadata.before_ino,
        &plan.before_sha256,
    )?;
    if plan.target.expected_identity.as_deref() != Some(expected_identity.as_str()) {
        return Err(AppError::blocked(
            "source_install_v1 expected identity 불일치",
        ));
    }
    Ok(())
}

pub(crate) fn render_source_install_v1(plan: &SourceInstallV1) -> Result<String, AppError> {
    validate_source_install_v1(plan)?;
    let operations = plan
        .operations
        .iter()
        .map(|operation| {
            format!(
                "\"{}\"",
                crate::app::workflow_adapter::ledger::json_string(operation)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let body = format!(
        "{{\"schema_version\":{},\"source_key\":\"{}\",\"target\":{},\"before_blob\":{},\"proposed_blob\":{},\"rollback_final\":{},\"install_temp\":{},\"guard_path\":{},\"before_sha256\":\"{}\",\"before_byte_length\":{},\"proposed_sha256\":\"{}\",\"proposed_byte_length\":{},\"permissions\":{},\"ownership\":{},\"platform\":\"{}\",\"unix_metadata\":{},\"operations\":[{}]}}",
        plan.schema_version,
        crate::app::workflow_adapter::ledger::json_string(&plan.source_key),
        render_path(&plan.target),
        render_blob(&plan.before_blob),
        render_blob(&plan.proposed_blob),
        render_path(&plan.rollback_final),
        render_path(&plan.install_temp),
        render_path(&plan.guard_path),
        plan.before_sha256,
        plan.before_byte_length,
        plan.proposed_sha256,
        plan.proposed_byte_length,
        render_permissions(&plan.permissions),
        render_ownership(&plan.ownership),
        plan.platform,
        render_unix_metadata(&plan.unix_metadata),
        operations
    );
    enforce_byte_limit(
        body.len(),
        MAX_SOURCE_INSTALL_BYTES,
        "source_install_v1 byte limit 초과",
    )?;
    Ok(body)
}

pub(crate) fn parse_source_install_v1(body: &str) -> Result<SourceInstallV1, AppError> {
    let object =
        strict_json::parse_canonical_object(body, SOURCE_INSTALL_KEYS, "source_install_v1")?;
    let plan = SourceInstallV1 {
        schema_version: strict_json::canonical_u64(&object, "schema_version", "source_install_v1")?,
        source_key: required_string(&object, "source_key")?,
        target: parse_path(required_object(&object, "target")?)?,
        before_blob: parse_blob(required_object(&object, "before_blob")?)?,
        proposed_blob: parse_blob(required_object(&object, "proposed_blob")?)?,
        rollback_final: parse_path(required_object(&object, "rollback_final")?)?,
        install_temp: parse_path(required_object(&object, "install_temp")?)?,
        guard_path: parse_path(required_object(&object, "guard_path")?)?,
        before_sha256: required_string(&object, "before_sha256")?,
        before_byte_length: strict_json::canonical_u64(
            &object,
            "before_byte_length",
            "source_install_v1",
        )?,
        proposed_sha256: required_string(&object, "proposed_sha256")?,
        proposed_byte_length: strict_json::canonical_u64(
            &object,
            "proposed_byte_length",
            "source_install_v1",
        )?,
        permissions: parse_permissions(required_object(&object, "permissions")?)?,
        ownership: parse_ownership(required_object(&object, "ownership")?)?,
        platform: required_string(&object, "platform")?,
        unix_metadata: parse_unix_metadata(required_object(&object, "unix_metadata")?)?,
        operations: required_string_array(&object, "operations")?,
    };
    validate_source_install_v1(&plan)?;
    if render_source_install_v1(&plan)? != body {
        return Err(AppError::blocked(
            "source_install_v1 canonical re-render 불일치",
        ));
    }
    Ok(plan)
}

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
