use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

#[cfg(windows)]
use crate::adapters::filesystem::windows_replace;
use crate::adapters::filesystem::{layout as paths, lease};
use crate::foundation::error::AppError;
use crate::foundation::serialization as strict_json;
use crate::foundation::serialization::{CanonicalObject, CanonicalValue};
use crate::runtime_core::workflow::domain::transition::{
    is_state_transition_intent_kind, is_terminal_action_intent_kind,
};
pub(crate) use crate::runtime_core::workflow::domain::transition::{
    CurrentStateIntent, PreparedBlob, PreparedBundleContext, PreparedEventChain, PreparedMember,
    PreparedMemberBinding, PreparedMemberKind, PreparedPath, PreparedSourceBundle, SourceInstallV1,
    SourceOwnership, SourcePermissions, UnixSourceMetadata,
};

mod bundle_codec;
mod bundle_preparation;
mod bundle_validation;
mod journal;
mod source_install;
use bundle_codec::{
    parse_additional_members, parse_event_chain_plan, parse_projection_lag_reference,
    parse_semantic_events, parse_source_members, prepared_member_order, render_event_chain_plan,
    render_semantic_event, render_semantic_events, render_source_members,
    PreparedMemberParseContext,
};
pub(crate) use bundle_preparation::{
    bind_additional_members, bind_planned_events, install_projection_lag, planned_events,
    prepare_projection_lag_member, prepare_source_bundle, prepare_source_bundle_with_context,
    prepare_state_transition_bundle, prepare_terminal_action_bundle_with_context,
    prepare_workflow_bundle_with_context, projection_lag_path, remove_projection_lag,
};
use bundle_validation::{validate_event_chain, validate_prepared_source_bundle};
#[cfg(test)]
pub(crate) use journal::render_prepared_source_bundle;
pub(crate) use journal::{
    commit_prepared_source_bundle, parse_prepared_source_bundle, projection_lag_status_read_only,
    recover_pending_source_bundles, remove_committed_source_bundle,
    validate_committed_bundle_cleanup_authority, ProjectionLagReadStatus, TransitionGuard,
};
use journal::{projection_lag_fault, restore_removed_file};
#[cfg(test)]
use journal::{
    read_regular_utf8_bounded, recover_pending_bundles_under_guard, recovery_work_may_exist,
};
pub(crate) use source_install::{
    parse_source_install_v1, prepare_source_install_v1, render_source_install_v1,
    resolve_prepared_project_path, source_identity_v1, source_install_rollback_path,
    validate_source_install_v1,
};

pub(crate) const MAX_SOURCE_BLOB_BYTES: usize = 262_144;
const MAX_PREPARED_EVENT_BYTES: usize = 16_384;
const MAX_PREPARED_EVENTS_BYTES: usize = 163_840;
const MAX_SOURCE_INSTALL_BYTES: usize = 32_768;
const MAX_PREPARED_BUNDLE_BYTES: usize = 1_048_576;
const MAX_RECOVERY_JOURNAL_ENTRIES: usize = 4;
const MAX_RECOVERY_JOURNAL_BYTES: usize = 2 * MAX_PREPARED_BUNDLE_BYTES + 64 * 1024;
const MAX_RECOVERY_PROJECT_ENTRIES: usize = 128;
const MAX_PROJECTION_LAG_ENTRIES: usize = 4;
const MAX_PROJECTION_LAG_BYTES: usize = 256 * 1024;

fn enforce_byte_limit(length: usize, limit: usize, message: &'static str) -> Result<(), AppError> {
    if length > limit {
        return Err(AppError::blocked(message));
    }
    Ok(())
}

fn checked_add_bytes(
    current: usize,
    additional: usize,
    limit: usize,
    overflow_message: &'static str,
    limit_message: &'static str,
) -> Result<usize, AppError> {
    let total = current
        .checked_add(additional)
        .ok_or_else(|| AppError::blocked(overflow_message))?;
    enforce_byte_limit(total, limit, limit_message)?;
    Ok(total)
}

pub(crate) const SOURCE_INSTALL_OPERATIONS: [&str; 19] = [
    "validate-target",
    "install-rollback-create-new",
    "fsync-rollback",
    "fsync-rollback-parent",
    "create-install-temp-new",
    "write-proposed",
    "apply-install-metadata",
    "fsync-install-temp",
    "hard-link-target-to-guard-create-new",
    "validate-guard-before-unlink",
    "fsync-target-parent",
    "unlink-target",
    "revalidate-guard-after-unlink",
    "hard-link-install-temp-to-target-create-new",
    "fsync-target-parent",
    "validate-installed-target",
    "remove-install-temp",
    "remove-guard",
    "fsync-target-parent",
];

const SOURCE_INSTALL_KEYS: &[&str] = &[
    "schema_version",
    "source_key",
    "target",
    "before_blob",
    "proposed_blob",
    "rollback_final",
    "install_temp",
    "guard_path",
    "before_sha256",
    "before_byte_length",
    "proposed_sha256",
    "proposed_byte_length",
    "permissions",
    "ownership",
    "platform",
    "unix_metadata",
    "operations",
];
const PATH_KEYS: &[&str] = &[
    "namespace",
    "path",
    "parent",
    "basename",
    "expected_type",
    "expected_identity",
];
const BLOB_KEYS: &[&str] = &["blob_id", "member_path", "sha256", "byte_length"];
const PERMISSION_KEYS: &[&str] = &[
    "before_readonly",
    "install_readonly",
    "before_mode",
    "install_mode",
];
const OWNERSHIP_KEYS: &[&str] = &["before_owner", "install_owner"];
const UNIX_METADATA_KEYS: &[&str] = &[
    "before_mode",
    "install_mode",
    "before_uid",
    "before_gid",
    "install_uid",
    "install_gid",
    "before_dev",
    "before_ino",
];
const PREPARED_BUNDLE_KEYS: &[&str] = &[
    "schema_version",
    "intent_id",
    "intent_kind",
    "project_id",
    "session_id",
    "workflow_id",
    "prepared_at_ms",
    "before_binding",
    "members",
    "semantic_events",
    "event_chain_plan",
    "source_install_v1",
    "projection_lag_v1",
];
const BEFORE_BINDING_KEYS: &[&str] = &[
    "current_revision",
    "current_artifact_hash",
    "ledger_count",
    "ledger_event_id",
    "ledger_hash",
];
const MEMBER_KEYS: &[&str] = &[
    "member_kind",
    "path",
    "schema_version",
    "owner",
    "binding",
    "prepared_at_ms",
    "bytes_utf8",
    "byte_length",
    "sha256",
    "expected_type",
    "expected_identity",
    "permissions",
    "ownership",
];
const OWNER_KEYS: &[&str] = &["project_id", "session_id", "workflow_id", "intent_id"];
const BINDING_KEYS: &[&str] = &["artifact_id", "causal_id", "source_key", "event_id"];
const MEMBER_PERMISSION_KEYS: &[&str] = &["readonly", "mode"];
const SEMANTIC_EVENT_KEYS: &[&str] = &[
    "schema_version",
    "event_id",
    "ts_ms",
    "event_type",
    "project_id",
    "session_id",
    "summary",
    "details",
];
const EVENT_CHAIN_PLAN_KEYS: &[&str] =
    &["event_id", "ordinal", "previous_event_hash", "event_hash"];
const PROJECTION_LAG_REFERENCE_KEYS: &[&str] = &["member_kind", "member_index"];
const PROJECTION_LAG_KEYS: &[&str] = &[
    "schema_version",
    "intent_id",
    "event_id",
    "event_ordinal",
    "event_hash",
    "required_outputs",
    "required_event_ids",
];

fn required_u128(object: &CanonicalObject, key: &str) -> Result<u128, AppError> {
    strict_json::canonical_u128(object, key, "prepared source bundle")
}

fn render_optional_string(value: Option<&str>) -> String {
    value
        .map(|value| {
            format!(
                "\"{}\"",
                crate::app::workflow_adapter::ledger::json_string(value)
            )
        })
        .unwrap_or_else(|| "null".to_string())
}

#[cfg(not(windows))]
fn sync_parent(path: &Path) -> Result<(), AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::runtime("transition journal parent 누락"))?;
    let directory = fs::File::open(parent)
        .map_err(|err| AppError::runtime(format!("transition parent open 실패: {err}")))?;
    directory
        .sync_all()
        .map_err(|err| AppError::runtime(format!("transition parent fsync 실패: {err}")))
}

#[cfg(windows)]
fn sync_parent(_path: &Path) -> Result<(), AppError> {
    Ok(())
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn source_key_v1(intent_id: &str, target: &str, before: &str, proposed: &str) -> String {
    let mut bytes = b"source-install-v1".to_vec();
    for value in [intent_id, target, before, proposed] {
        bytes.push(0);
        bytes.extend_from_slice(value.as_bytes());
    }
    sha256_bytes(&bytes)
}

fn stored_project_path(root: &Path, path: &Path) -> Result<String, AppError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| AppError::blocked("source path가 canonical project root 밖입니다."))?;
    let value = relative
        .to_str()
        .ok_or_else(|| AppError::blocked("source project path가 UTF-8이 아닙니다."))?;
    validate_stored_path(value)?;
    Ok(value.replace(std::path::MAIN_SEPARATOR, "/"))
}

fn join_stored(parent: &str, basename: &str) -> String {
    if parent.is_empty() {
        basename.to_string()
    } else {
        format!("{parent}/{basename}")
    }
}

fn validate_prepared_path(path: &PreparedPath, target: bool) -> Result<(), AppError> {
    if path.namespace != "project" {
        return Err(AppError::blocked("prepared path namespace 불일치"));
    }
    validate_stored_path(&path.path)?;
    if !path.parent.is_empty() {
        validate_stored_path(&path.parent)?;
    }
    validate_basename(&path.basename)?;
    if join_stored(&path.parent, &path.basename) != path.path {
        return Err(AppError::blocked(
            "prepared path parent/basename binding 불일치",
        ));
    }
    if target {
        if path.expected_type != "file" || !path.expected_identity.as_deref().is_some_and(is_sha256)
        {
            return Err(AppError::blocked("prepared target type/identity 불일치"));
        }
    } else if path.expected_type != "absent" || path.expected_identity.is_some() {
        return Err(AppError::blocked(
            "prepared create-new type/identity 불일치",
        ));
    }
    Ok(())
}

fn validate_stored_path(value: &str) -> Result<(), AppError> {
    if value.is_empty()
        || value.starts_with('/')
        || value.contains('\\')
        || value.contains(['\0', '\r', '\n'])
        || value
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(AppError::blocked("stored project path 형식 불일치"));
    }
    Ok(())
}

fn validate_basename(value: &str) -> Result<(), AppError> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains(['/', '\\', '\0', '\r', '\n'])
    {
        return Err(AppError::blocked("prepared basename 형식 불일치"));
    }
    Ok(())
}

fn validate_ascii_id(value: &str, label: &str) -> Result<(), AppError> {
    if value.is_empty()
        || value.len() > 200
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(AppError::blocked(format!("{label} id 형식 불일치")));
    }
    Ok(())
}

fn checked_len(bytes: &[u8], label: &str) -> Result<u64, AppError> {
    u64::try_from(bytes.len()).map_err(|_| AppError::blocked(format!("{label} length 범위 초과")))
}

fn render_path(path: &PreparedPath) -> String {
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

fn render_blob(blob: &PreparedBlob) -> String {
    format!(
        "{{\"blob_id\":\"{}\",\"member_path\":\"{}\",\"sha256\":\"{}\",\"byte_length\":{}}}",
        crate::app::workflow_adapter::ledger::json_string(&blob.blob_id),
        crate::app::workflow_adapter::ledger::json_string(&blob.member_path),
        blob.sha256,
        blob.byte_length
    )
}

fn render_permissions(value: &SourcePermissions) -> String {
    format!(
        "{{\"before_readonly\":{},\"install_readonly\":{},\"before_mode\":{},\"install_mode\":{}}}",
        value.before_readonly, value.install_readonly, value.before_mode, value.install_mode
    )
}

fn render_ownership(value: &SourceOwnership) -> String {
    format!(
        "{{\"before_owner\":\"{}\",\"install_owner\":\"{}\"}}",
        crate::app::workflow_adapter::ledger::json_string(&value.before_owner),
        crate::app::workflow_adapter::ledger::json_string(&value.install_owner)
    )
}

fn render_unix_metadata(value: &UnixSourceMetadata) -> String {
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

fn required_object<'a>(
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

fn required_string(object: &CanonicalObject, key: &str) -> Result<String, AppError> {
    match object.get(key) {
        Some(CanonicalValue::String(value)) => Ok(value.clone()),
        _ => Err(AppError::blocked(format!(
            "source_install_v1 string 누락: {key}"
        ))),
    }
}

fn optional_string(object: &CanonicalObject, key: &str) -> Result<Option<String>, AppError> {
    match object.get(key) {
        Some(CanonicalValue::String(value)) => Ok(Some(value.clone())),
        Some(CanonicalValue::Null) => Ok(None),
        _ => Err(AppError::blocked(format!(
            "source_install_v1 nullable string 손상: {key}"
        ))),
    }
}

fn required_bool(object: &CanonicalObject, key: &str) -> Result<bool, AppError> {
    match object.get(key) {
        Some(CanonicalValue::Bool(value)) => Ok(*value),
        _ => Err(AppError::blocked(format!(
            "source_install_v1 bool 누락: {key}"
        ))),
    }
}

fn required_u32(object: &CanonicalObject, key: &str) -> Result<u32, AppError> {
    u32::try_from(strict_json::canonical_u64(
        object,
        key,
        "source_install_v1",
    )?)
    .map_err(|_| AppError::blocked(format!("source_install_v1 u32 overflow: {key}")))
}

fn required_string_array(object: &CanonicalObject, key: &str) -> Result<Vec<String>, AppError> {
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

fn require_keys(object: &CanonicalObject, expected: &[&str]) -> Result<(), AppError> {
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

fn parse_path(object: &CanonicalObject) -> Result<PreparedPath, AppError> {
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

fn parse_blob(object: &CanonicalObject) -> Result<PreparedBlob, AppError> {
    require_keys(object, BLOB_KEYS)?;
    Ok(PreparedBlob {
        blob_id: required_string(object, "blob_id")?,
        member_path: required_string(object, "member_path")?,
        sha256: required_string(object, "sha256")?,
        byte_length: strict_json::canonical_u64(object, "byte_length", "source_install_v1")?,
    })
}

fn parse_permissions(object: &CanonicalObject) -> Result<SourcePermissions, AppError> {
    require_keys(object, PERMISSION_KEYS)?;
    Ok(SourcePermissions {
        before_readonly: required_bool(object, "before_readonly")?,
        install_readonly: required_bool(object, "install_readonly")?,
        before_mode: required_u32(object, "before_mode")?,
        install_mode: required_u32(object, "install_mode")?,
    })
}

fn parse_ownership(object: &CanonicalObject) -> Result<SourceOwnership, AppError> {
    require_keys(object, OWNERSHIP_KEYS)?;
    Ok(SourceOwnership {
        before_owner: required_string(object, "before_owner")?,
        install_owner: required_string(object, "install_owner")?,
    })
}

fn parse_unix_metadata(object: &CanonicalObject) -> Result<UnixSourceMetadata, AppError> {
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

fn append_tlv(target: &mut Vec<u8>, tag: u8, value: &[u8]) -> Result<(), AppError> {
    let length = u16::try_from(value.len())
        .map_err(|_| AppError::blocked("source identity TLV length overflow"))?;
    target.push(tag);
    target.extend_from_slice(&length.to_be_bytes());
    target.extend_from_slice(value);
    Ok(())
}

fn decode_sha256(value: &str) -> Result<[u8; 32], AppError> {
    if !is_sha256(value) {
        return Err(AppError::blocked("SHA-256 raw decode 형식 불일치"));
    }
    let mut output = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        output[index] = (hex_nibble(pair[0]).expect("lowercase hash validated") << 4)
            | hex_nibble(pair[1]).expect("lowercase hash validated");
    }
    Ok(output)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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

#[cfg(test)]
#[path = "transition/tests/mod.rs"]
mod tests;
