use super::*;

pub(super) fn required_u128(object: &CanonicalObject, key: &str) -> Result<u128, AppError> {
    strict_json::canonical_u128(object, key, "prepared source bundle")
}

pub(super) fn render_optional_string(value: Option<&str>) -> String {
    value
        .map(|value| {
            format!(
                "\"{}\"",
                crate::app::workflow_adapter::ledger::json_string(value)
            )
        })
        .unwrap_or_else(|| "null".to_string())
}

pub(super) fn render_path(path: &PreparedPath) -> String {
    format!(
        "{{\"namespace\":\"{}\",\"path\":\"{}\",\"parent\":\"{}\",\"basename\":\"{}\",\"expected_type\":\"{}\",\"expected_identity\":{}}}",
        crate::app::workflow_adapter::ledger::json_string(&path.namespace),
        crate::app::workflow_adapter::ledger::json_string(&path.path),
        crate::app::workflow_adapter::ledger::json_string(&path.parent),
        crate::app::workflow_adapter::ledger::json_string(&path.basename),
        crate::app::workflow_adapter::ledger::json_string(&path.expected_type),
        path.expected_identity
            .as_ref()
            .map(|value| format!("\"{}\"", crate::app::workflow_adapter::ledger::json_string(value)))
            .unwrap_or_else(|| "null".to_string())
    )
}

pub(super) fn render_blob(blob: &PreparedBlob) -> String {
    format!(
        "{{\"blob_id\":\"{}\",\"member_path\":\"{}\",\"sha256\":\"{}\",\"byte_length\":{}}}",
        crate::app::workflow_adapter::ledger::json_string(&blob.blob_id),
        crate::app::workflow_adapter::ledger::json_string(&blob.member_path),
        blob.sha256,
        blob.byte_length
    )
}

pub(super) fn render_permissions(value: &SourcePermissions) -> String {
    format!(
        "{{\"before_readonly\":{},\"install_readonly\":{},\"before_mode\":{},\"install_mode\":{}}}",
        value.before_readonly, value.install_readonly, value.before_mode, value.install_mode
    )
}

pub(super) fn render_ownership(value: &SourceOwnership) -> String {
    format!(
        "{{\"before_owner\":\"{}\",\"install_owner\":\"{}\"}}",
        crate::app::workflow_adapter::ledger::json_string(&value.before_owner),
        crate::app::workflow_adapter::ledger::json_string(&value.install_owner)
    )
}

pub(super) fn render_unix_metadata(value: &UnixSourceMetadata) -> String {
    format!(
        "{{\"before_mode\":{},\"install_mode\":{},\"before_uid\":{},\"before_gid\":{},\"install_uid\":{},\"install_gid\":{},\"before_dev\":{},\"before_ino\":{}}}",
        value.before_mode,
        value.install_mode,
        value.before_uid,
        value.before_gid,
        value.install_uid,
        value.install_gid,
        value.before_dev,
        value.before_ino
    )
}

pub(super) fn required_object<'a>(
    object: &'a CanonicalObject,
    key: &str,
) -> Result<&'a CanonicalObject, AppError> {
    match object.get(key) {
        Some(CanonicalValue::Object(value)) => Ok(value),
        _ => Err(AppError::blocked(format!(
            "source_install_v1 object 누락: {key}"
        ))),
    }
}

pub(super) fn required_string(object: &CanonicalObject, key: &str) -> Result<String, AppError> {
    match object.get(key) {
        Some(CanonicalValue::String(value)) => Ok(value.clone()),
        _ => Err(AppError::blocked(format!(
            "source_install_v1 string 누락: {key}"
        ))),
    }
}

pub(super) fn optional_string(
    object: &CanonicalObject,
    key: &str,
) -> Result<Option<String>, AppError> {
    match object.get(key) {
        Some(CanonicalValue::String(value)) => Ok(Some(value.clone())),
        Some(CanonicalValue::Null) => Ok(None),
        _ => Err(AppError::blocked(format!(
            "source_install_v1 nullable string 손상: {key}"
        ))),
    }
}

pub(super) fn required_bool(object: &CanonicalObject, key: &str) -> Result<bool, AppError> {
    match object.get(key) {
        Some(CanonicalValue::Bool(value)) => Ok(*value),
        _ => Err(AppError::blocked(format!(
            "source_install_v1 bool 누락: {key}"
        ))),
    }
}

pub(super) fn required_u32(object: &CanonicalObject, key: &str) -> Result<u32, AppError> {
    u32::try_from(strict_json::canonical_u64(
        object,
        key,
        "source_install_v1",
    )?)
    .map_err(|_| AppError::blocked(format!("source_install_v1 u32 overflow: {key}")))
}

pub(super) fn required_string_array(
    object: &CanonicalObject,
    key: &str,
) -> Result<Vec<String>, AppError> {
    let Some(CanonicalValue::Array(values)) = object.get(key) else {
        return Err(AppError::blocked(format!(
            "source_install_v1 array 누락: {key}"
        )));
    };
    values
        .iter()
        .map(|value| match value {
            CanonicalValue::String(value) => Ok(value.clone()),
            _ => Err(AppError::blocked(format!(
                "source_install_v1 string array 손상: {key}"
            ))),
        })
        .collect()
}

pub(super) fn require_keys(object: &CanonicalObject, expected: &[&str]) -> Result<(), AppError> {
    let actual = object
        .entries
        .iter()
        .map(|(key, _)| key.as_str())
        .collect::<Vec<_>>();
    if actual != expected {
        return Err(AppError::blocked(
            "source_install_v1 nested key order 불일치",
        ));
    }
    Ok(())
}

pub(super) fn parse_path(object: &CanonicalObject) -> Result<PreparedPath, AppError> {
    require_keys(object, PATH_KEYS)?;
    Ok(PreparedPath {
        namespace: required_string(object, "namespace")?,
        path: required_string(object, "path")?,
        parent: required_string(object, "parent")?,
        basename: required_string(object, "basename")?,
        expected_type: required_string(object, "expected_type")?,
        expected_identity: optional_string(object, "expected_identity")?,
    })
}

pub(super) fn parse_blob(object: &CanonicalObject) -> Result<PreparedBlob, AppError> {
    require_keys(object, BLOB_KEYS)?;
    Ok(PreparedBlob {
        blob_id: required_string(object, "blob_id")?,
        member_path: required_string(object, "member_path")?,
        sha256: required_string(object, "sha256")?,
        byte_length: strict_json::canonical_u64(object, "byte_length", "source_install_v1")?,
    })
}

pub(super) fn parse_permissions(object: &CanonicalObject) -> Result<SourcePermissions, AppError> {
    require_keys(object, PERMISSION_KEYS)?;
    Ok(SourcePermissions {
        before_readonly: required_bool(object, "before_readonly")?,
        install_readonly: required_bool(object, "install_readonly")?,
        before_mode: required_u32(object, "before_mode")?,
        install_mode: required_u32(object, "install_mode")?,
    })
}

pub(super) fn parse_ownership(object: &CanonicalObject) -> Result<SourceOwnership, AppError> {
    require_keys(object, OWNERSHIP_KEYS)?;
    Ok(SourceOwnership {
        before_owner: required_string(object, "before_owner")?,
        install_owner: required_string(object, "install_owner")?,
    })
}

pub(super) fn parse_unix_metadata(
    object: &CanonicalObject,
) -> Result<UnixSourceMetadata, AppError> {
    require_keys(object, UNIX_METADATA_KEYS)?;
    Ok(UnixSourceMetadata {
        before_mode: required_u32(object, "before_mode")?,
        install_mode: required_u32(object, "install_mode")?,
        before_uid: required_u32(object, "before_uid")?,
        before_gid: required_u32(object, "before_gid")?,
        install_uid: required_u32(object, "install_uid")?,
        install_gid: required_u32(object, "install_gid")?,
        before_dev: strict_json::canonical_u64(object, "before_dev", "source_install_v1")?,
        before_ino: strict_json::canonical_u64(object, "before_ino", "source_install_v1")?,
    })
}
