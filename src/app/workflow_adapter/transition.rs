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
mod canonical;
mod contracts;
mod journal;
mod source_install;
mod source_support;
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
pub(in crate::app::workflow_adapter::transition) use canonical::*;
pub(in crate::app::workflow_adapter::transition) use contracts::{
    checked_add_bytes, enforce_byte_limit, BEFORE_BINDING_KEYS, BINDING_KEYS, BLOB_KEYS,
    EVENT_CHAIN_PLAN_KEYS, MAX_PREPARED_BUNDLE_BYTES, MAX_PREPARED_EVENTS_BYTES,
    MAX_PREPARED_EVENT_BYTES, MAX_PROJECTION_LAG_BYTES, MAX_PROJECTION_LAG_ENTRIES,
    MAX_RECOVERY_JOURNAL_BYTES, MAX_RECOVERY_JOURNAL_ENTRIES, MAX_RECOVERY_PROJECT_ENTRIES,
    MAX_SOURCE_INSTALL_BYTES, MEMBER_KEYS, MEMBER_PERMISSION_KEYS, OWNERSHIP_KEYS, OWNER_KEYS,
    PATH_KEYS, PERMISSION_KEYS, PREPARED_BUNDLE_KEYS, PROJECTION_LAG_KEYS,
    PROJECTION_LAG_REFERENCE_KEYS, SEMANTIC_EVENT_KEYS, SOURCE_INSTALL_KEYS, UNIX_METADATA_KEYS,
};
pub(crate) use contracts::{MAX_SOURCE_BLOB_BYTES, SOURCE_INSTALL_OPERATIONS};
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
pub(in crate::app::workflow_adapter::transition) use source_support::*;

#[cfg(test)]
#[path = "transition/tests/mod.rs"]
mod tests;
