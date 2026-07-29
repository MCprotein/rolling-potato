use super::*;

pub(crate) const MAX_SOURCE_BLOB_BYTES: usize = 262_144;
pub(super) const MAX_PREPARED_EVENT_BYTES: usize = 16_384;
pub(super) const MAX_PREPARED_EVENTS_BYTES: usize = 163_840;
pub(super) const MAX_SOURCE_INSTALL_BYTES: usize = 32_768;
pub(super) const MAX_PREPARED_BUNDLE_BYTES: usize = 1_048_576;
pub(super) const MAX_RECOVERY_JOURNAL_ENTRIES: usize = 4;
pub(super) const MAX_RECOVERY_JOURNAL_BYTES: usize = 2 * MAX_PREPARED_BUNDLE_BYTES + 64 * 1024;
pub(super) const MAX_RECOVERY_PROJECT_ENTRIES: usize = 128;
pub(super) const MAX_PROJECTION_LAG_ENTRIES: usize = 4;
pub(super) const MAX_PROJECTION_LAG_BYTES: usize = 256 * 1024;

pub(super) fn enforce_byte_limit(
    length: usize,
    limit: usize,
    message: &'static str,
) -> Result<(), AppError> {
    if length > limit {
        return Err(AppError::blocked(message));
    }
    Ok(())
}

pub(super) fn checked_add_bytes(
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

pub(super) const SOURCE_INSTALL_KEYS: &[&str] = &[
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
pub(super) const PATH_KEYS: &[&str] = &[
    "namespace",
    "path",
    "parent",
    "basename",
    "expected_type",
    "expected_identity",
];
pub(super) const BLOB_KEYS: &[&str] = &["blob_id", "member_path", "sha256", "byte_length"];
pub(super) const PERMISSION_KEYS: &[&str] = &[
    "before_readonly",
    "install_readonly",
    "before_mode",
    "install_mode",
];
pub(super) const OWNERSHIP_KEYS: &[&str] = &["before_owner", "install_owner"];
pub(super) const UNIX_METADATA_KEYS: &[&str] = &[
    "before_mode",
    "install_mode",
    "before_uid",
    "before_gid",
    "install_uid",
    "install_gid",
    "before_dev",
    "before_ino",
];
pub(super) const PREPARED_BUNDLE_KEYS: &[&str] = &[
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
pub(super) const BEFORE_BINDING_KEYS: &[&str] = &[
    "current_revision",
    "current_artifact_hash",
    "ledger_count",
    "ledger_event_id",
    "ledger_hash",
];
pub(super) const MEMBER_KEYS: &[&str] = &[
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
pub(super) const OWNER_KEYS: &[&str] = &["project_id", "session_id", "workflow_id", "intent_id"];
pub(super) const BINDING_KEYS: &[&str] = &["artifact_id", "causal_id", "source_key", "event_id"];
pub(super) const MEMBER_PERMISSION_KEYS: &[&str] = &["readonly", "mode"];
pub(super) const SEMANTIC_EVENT_KEYS: &[&str] = &[
    "schema_version",
    "event_id",
    "ts_ms",
    "event_type",
    "project_id",
    "session_id",
    "summary",
    "details",
];
pub(super) const EVENT_CHAIN_PLAN_KEYS: &[&str] =
    &["event_id", "ordinal", "previous_event_hash", "event_hash"];
pub(super) const PROJECTION_LAG_REFERENCE_KEYS: &[&str] = &["member_kind", "member_index"];
pub(super) const PROJECTION_LAG_KEYS: &[&str] = &[
    "schema_version",
    "intent_id",
    "event_id",
    "event_ordinal",
    "event_hash",
    "required_outputs",
    "required_event_ids",
];
