use super::super::*;
use super::paths::source_identity_v1;

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
