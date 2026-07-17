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

mod bundle_preparation;
mod journal;
mod source_install;
pub(crate) use bundle_preparation::{
    bind_additional_members, bind_planned_events, install_projection_lag, planned_events,
    prepare_projection_lag_member, prepare_source_bundle, prepare_source_bundle_with_context,
    prepare_state_transition_bundle, prepare_terminal_action_bundle_with_context,
    prepare_workflow_bundle_with_context, projection_lag_path, remove_projection_lag,
};
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

fn validate_prepared_source_bundle(bundle: &PreparedSourceBundle) -> Result<(), AppError> {
    validate_ascii_id(&bundle.intent_id, "intent")?;
    validate_ascii_id(&bundle.project_id, "project")?;
    validate_ascii_id(&bundle.session_id, "session")?;
    if let Some(workflow_id) = bundle.workflow_id.as_deref() {
        validate_ascii_id(workflow_id, "workflow")?;
    }
    if !matches!(
        bundle.intent_kind.as_str(),
        "approve-patch" | "approve-verification"
    ) && !is_state_transition_intent_kind(&bundle.intent_kind)
        && !is_terminal_action_intent_kind(&bundle.intent_kind)
    {
        return Err(AppError::blocked("prepared bundle intent kind 불일치"));
    }
    let missing_current = bundle.current_revision == 0
        && bundle.current_artifact_hash == "missing"
        && matches!(
            bundle.intent_kind.as_str(),
            "bootstrap" | "repair-workflow-pointer" | "reconcile" | "start-session"
        );
    let preserved_invalid_current = bundle.current_revision == 0
        && is_sha256(&bundle.current_artifact_hash)
        && bundle.intent_kind == "reconcile";
    if (!missing_current
        && !preserved_invalid_current
        && (bundle.current_revision == 0 || !is_sha256(&bundle.current_artifact_hash)))
        || (bundle.ledger_binding.event_count == 0
            && (bundle.ledger_binding.event_id.is_some()
                || bundle.ledger_binding.event_hash != "root"))
        || (bundle.ledger_binding.event_count > 0
            && (bundle.ledger_binding.event_id.is_none()
                || !is_sha256(&bundle.ledger_binding.event_hash)))
    {
        return Err(AppError::blocked("prepared source bundle binding 불일치"));
    }
    match (
        bundle.intent_kind.as_str(),
        bundle.source_install.as_ref(),
        bundle.before_bytes.as_deref(),
        bundle.proposed_bytes.as_deref(),
    ) {
        ("approve-patch", Some(source), Some(before), Some(proposed)) => {
            validate_source_install_v1(source)?;
            if sha256_bytes(before.as_bytes()) != source.before_sha256
                || sha256_bytes(proposed.as_bytes()) != source.proposed_sha256
            {
                return Err(AppError::blocked(
                    "prepared source bundle hash binding 불일치",
                ));
            }
        }
        ("approve-verification", None, None, None) => {}
        (kind, Some(source), Some(before), Some(proposed))
            if is_terminal_action_intent_kind(kind) =>
        {
            validate_source_install_v1(source)?;
            if sha256_bytes(before.as_bytes()) != source.before_sha256
                || sha256_bytes(proposed.as_bytes()) != source.proposed_sha256
                || kind == "deny-patch"
            {
                return Err(AppError::blocked(
                    "prepared terminal source bundle hash/intent 불일치",
                ));
            }
        }
        (kind, None, None, None) if is_terminal_action_intent_kind(kind) => {
            if kind == "deny-verification" {
                return Err(AppError::blocked("prepared denial rollback source 누락"));
            }
        }
        (kind, None, None, None) if is_state_transition_intent_kind(kind) => {}
        _ => {
            return Err(AppError::blocked(
                "prepared bundle source nullability 불일치",
            ))
        }
    }
    validate_event_chain(bundle)?;
    validate_additional_members(bundle)?;
    Ok(())
}

fn validate_event_chain(bundle: &PreparedSourceBundle) -> Result<(), AppError> {
    if bundle.semantic_events.len() != bundle.event_chain_plan.len()
        || bundle.semantic_events.len() > 10
    {
        return Err(AppError::blocked(
            "prepared semantic event/chain cardinality 불일치",
        ));
    }
    let mut aggregate_event_bytes = 0_usize;
    for event in &bundle.semantic_events {
        let rendered = render_semantic_event(event);
        enforce_byte_limit(
            rendered.len(),
            MAX_PREPARED_EVENT_BYTES,
            "prepared semantic event byte limit 초과",
        )?;
        aggregate_event_bytes = checked_add_bytes(
            aggregate_event_bytes,
            rendered.len(),
            MAX_PREPARED_EVENTS_BYTES,
            "prepared semantic event byte count overflow",
            "prepared semantic events aggregate byte limit 초과",
        )?;
    }
    let mut previous = bundle.ledger_binding.event_hash.clone();
    let mut ids = std::collections::BTreeSet::new();
    for (index, (event, chain)) in bundle
        .semantic_events
        .iter()
        .zip(&bundle.event_chain_plan)
        .enumerate()
    {
        validate_ascii_id(&event.event_id, "event")?;
        if event.event_type.is_empty()
            || event.project_id != bundle.project_id
            || event.session_id != bundle.session_id
            || !ids.insert(event.event_id.as_str())
        {
            return Err(AppError::blocked(
                "prepared semantic event owner/id binding 불일치",
            ));
        }
        let expected_ordinal = bundle
            .ledger_binding
            .event_count
            .checked_add(
                u64::try_from(index + 1)
                    .map_err(|_| AppError::blocked("prepared event ordinal overflow"))?,
            )
            .ok_or_else(|| AppError::blocked("prepared event ordinal overflow"))?;
        let expected_hash =
            crate::app::workflow_adapter::ledger::planned_event_hash(event, &previous);
        if chain.event_id != event.event_id
            || chain.ordinal != expected_ordinal
            || chain.previous_event_hash != previous
            || chain.event_hash != expected_hash
            || !is_sha256(&chain.event_hash)
        {
            return Err(AppError::blocked(
                "prepared semantic event chain binding 불일치",
            ));
        }
        previous = chain.event_hash.clone();
    }
    Ok(())
}

fn validate_additional_members(bundle: &PreparedSourceBundle) -> Result<(), AppError> {
    if bundle.additional_members.is_empty() {
        if bundle.intent_kind != "approve-patch"
            || bundle.source_install.is_none()
            || bundle.projection_lag_member_index.is_some()
        {
            return Err(AppError::blocked(
                "prepared source-only bundle에는 projection lag reference가 없어야 합니다.",
            ));
        }
        return Ok(());
    }
    if bundle.intent_kind == "approve-verification" {
        return validate_verification_members(bundle);
    }
    if is_terminal_action_intent_kind(&bundle.intent_kind) {
        return validate_verification_members(bundle);
    }
    if is_state_transition_intent_kind(&bundle.intent_kind) {
        return validate_state_transition_members(bundle);
    }
    if bundle.intent_kind != "approve-patch" || bundle.source_install.is_none() {
        return Err(AppError::blocked(
            "prepared approval member intent binding 불일치",
        ));
    }
    if bundle.additional_members.len() != 8
        || bundle.semantic_events.len() != 10
        || bundle.workflow_id.is_none()
        || bundle.projection_lag_member_index != Some(10)
    {
        return Err(AppError::blocked(
            "prepared production approval exact-11 cardinality 불일치",
        ));
    }
    let expected_kinds = [
        PreparedMemberKind::ToolOutput,
        PreparedMemberKind::TranscriptV2,
        PreparedMemberKind::WorkflowSnapshot,
        PreparedMemberKind::WorkflowSnapshot,
        PreparedMemberKind::WorkflowPointer,
        PreparedMemberKind::WorkflowPointer,
        PreparedMemberKind::CurrentImage,
        PreparedMemberKind::ProjectionLag,
    ];
    let mut artifact_ids = std::collections::BTreeSet::new();
    let mut paths = std::collections::BTreeMap::<&str, Vec<&PreparedMember>>::new();
    for (index, member) in bundle.additional_members.iter().enumerate() {
        if member.kind != expected_kinds[index]
            || member.semantic_role_rank
                != match member.kind {
                    PreparedMemberKind::WorkflowSnapshot | PreparedMemberKind::WorkflowPointer => {
                        u8::try_from(index % 2)
                            .map_err(|_| AppError::blocked("prepared member role overflow"))?
                    }
                    _ => 0,
                }
            || member.binding.source_key.is_some()
        {
            return Err(AppError::blocked(
                "prepared production member kind/role/source binding 불일치",
            ));
        }
        let expected_schema = match member.kind {
            PreparedMemberKind::ToolOutput => 1,
            PreparedMemberKind::TranscriptV2 => 2,
            PreparedMemberKind::WorkflowSnapshot | PreparedMemberKind::WorkflowPointer => 4,
            PreparedMemberKind::CurrentImage => 2,
            PreparedMemberKind::ProjectionLag => 1,
        };
        if member.schema_version != expected_schema
            || member.bytes_utf8.is_empty()
            || member.expected_type == "content-addressed-reference"
            || member.readonly
            || member.mode != 0o600
            || member.ownership.is_some()
            || member
                .binding
                .artifact_id
                .as_deref()
                .is_none_or(|id| validate_ascii_id(id, "member artifact").is_err())
        {
            return Err(AppError::blocked(
                "prepared production member schema/bytes/binding 불일치",
            ));
        }
        validate_stored_path(&member.path)?;
        if member.kind == PreparedMemberKind::WorkflowPointer
            && member.path
                != format!(
                    ".rpotato/workflows/{}.json",
                    bundle.workflow_id.as_deref().expect("validated above")
                )
        {
            return Err(AppError::blocked(
                "prepared workflow pointer canonical path 불일치",
            ));
        }
        for (label, value) in [
            ("member causal", member.binding.causal_id.as_deref()),
            ("member event", member.binding.event_id.as_deref()),
        ] {
            if let Some(value) = value {
                validate_ascii_id(value, label)?;
            }
        }
        let limit = match member.kind {
            PreparedMemberKind::ToolOutput => 262_144,
            PreparedMemberKind::TranscriptV2 => 131_072,
            PreparedMemberKind::WorkflowSnapshot => 65_536,
            PreparedMemberKind::WorkflowPointer => 16_384,
            PreparedMemberKind::CurrentImage => 65_536,
            PreparedMemberKind::ProjectionLag => 4_096,
        };
        enforce_byte_limit(
            member.bytes_utf8.len(),
            limit,
            "prepared member byte limit 초과",
        )?;
        let artifact_id = member
            .binding
            .artifact_id
            .as_deref()
            .expect("validated above");
        if !artifact_ids.insert(artifact_id) {
            return Err(AppError::blocked("prepared member artifact id 중복"));
        }
        paths.entry(member.path.as_str()).or_default().push(member);
        if index > 0
            && prepared_member_order(
                &bundle.additional_members[index - 1],
                &bundle.additional_members[index],
            ) != std::cmp::Ordering::Less
        {
            return Err(AppError::blocked(
                "prepared member total order/duplicate full key 불일치",
            ));
        }
    }
    for (path, members) in paths {
        if members.len() == 1 {
            continue;
        }
        let workflow_id = bundle.workflow_id.as_deref().expect("validated above");
        let expected_path = format!(".rpotato/workflows/{workflow_id}.json");
        if members.len() != 2
            || path != expected_path
            || members
                .iter()
                .any(|member| member.kind != PreparedMemberKind::WorkflowPointer)
            || members[0].semantic_role_rank != 0
            || members[1].semantic_role_rank != 1
        {
            return Err(AppError::blocked(
                "prepared member duplicate path는 exact R+1/R+2 pointer pair만 허용됩니다.",
            ));
        }
    }
    let lag = bundle
        .additional_members
        .last()
        .expect("exact eight members validated");
    if lag.kind != PreparedMemberKind::ProjectionLag
        || bundle.projection_lag_member_index != Some(10)
        || lag.binding.event_id.as_deref() != Some(bundle.semantic_events[9].event_id.as_str())
    {
        return Err(AppError::blocked(
            "prepared projection lag E9/index binding 불일치",
        ));
    }
    validate_projection_lag_member(bundle, lag)?;
    Ok(())
}

fn validate_state_transition_members(bundle: &PreparedSourceBundle) -> Result<(), AppError> {
    let checkpoint = bundle.intent_kind == "checkpoint-workflow";
    let preserved_reconcile = bundle.intent_kind == "reconcile"
        && bundle.current_revision == 0
        && bundle.current_artifact_hash != "missing";
    let expected_kinds: &[PreparedMemberKind] = if checkpoint {
        &[
            PreparedMemberKind::WorkflowSnapshot,
            PreparedMemberKind::WorkflowPointer,
            PreparedMemberKind::CurrentImage,
        ]
    } else if preserved_reconcile {
        &[
            PreparedMemberKind::ToolOutput,
            PreparedMemberKind::CurrentImage,
        ]
    } else {
        &[PreparedMemberKind::CurrentImage]
    };
    if bundle.source_install.is_some()
        || bundle.before_bytes.is_some()
        || bundle.proposed_bytes.is_some()
        || bundle.projection_lag_member_index.is_some()
        || bundle.semantic_events.len() != 1
        || bundle.additional_members.len() != expected_kinds.len()
        || (checkpoint && bundle.workflow_id.is_none())
    {
        return Err(AppError::blocked(
            "prepared state transition exact shape 불일치",
        ));
    }
    let event = &bundle.semantic_events[0];
    let event_type_matches = match bundle.intent_kind.as_str() {
        "bootstrap" => event.event_type == "runtime.init",
        "checkpoint-workflow" => event.event_type == "workflow.checkpoint",
        "repair-workflow-pointer" => event.event_type == "workflow.pointer.recovered",
        "clear-terminal-workflow" => event.event_type == "workflow.pointer.cleared",
        "reconcile" => event.event_type.starts_with("state.reconcile."),
        "resume" => event.event_type.starts_with("workflow.resume."),
        "cancel" => event.event_type.starts_with("workflow.cancel."),
        "start-session" => event.event_type == "session.new",
        "select-session" => event.event_type == "session.resume.selected",
        "record-event" => !event.event_type.is_empty(),
        _ => false,
    };
    if !event_type_matches {
        return Err(AppError::blocked(
            "prepared state transition semantic event type 불일치",
        ));
    }
    let mut artifact_ids = std::collections::BTreeSet::new();
    let mut paths = std::collections::BTreeSet::new();
    for (index, member) in bundle.additional_members.iter().enumerate() {
        if member.kind != expected_kinds[index]
            || member.semantic_role_rank != 0
            || member.binding.source_key.is_some()
            || member.readonly
            || member.mode != 0o600
            || member.ownership.is_some()
            || member.bytes_utf8.is_empty()
        {
            return Err(AppError::blocked(
                "prepared state transition member metadata 불일치",
            ));
        }
        let expected_schema = match member.kind {
            PreparedMemberKind::ToolOutput => 1,
            PreparedMemberKind::WorkflowSnapshot | PreparedMemberKind::WorkflowPointer => 4,
            PreparedMemberKind::CurrentImage => 2,
            _ => {
                return Err(AppError::blocked(
                    "prepared state transition member kind 불일치",
                ))
            }
        };
        let limit = match member.kind {
            PreparedMemberKind::ToolOutput => 65_536,
            PreparedMemberKind::WorkflowSnapshot | PreparedMemberKind::CurrentImage => 65_536,
            PreparedMemberKind::WorkflowPointer => 16_384,
            _ => unreachable!("state transition kind validated above"),
        };
        if member.schema_version != expected_schema || member.bytes_utf8.len() > limit {
            return Err(AppError::blocked(
                "prepared state transition member schema/byte limit 불일치",
            ));
        }
        validate_stored_path(&member.path)?;
        let artifact_id = member
            .binding
            .artifact_id
            .as_deref()
            .ok_or_else(|| AppError::blocked("prepared state transition artifact id 누락"))?;
        validate_ascii_id(artifact_id, "member artifact")?;
        if member.binding.event_id.as_deref() != Some(event.event_id.as_str())
            || !artifact_ids.insert(artifact_id)
            || !paths.insert(member.path.as_str())
        {
            return Err(AppError::blocked(
                "prepared state transition member event/id/path 불일치",
            ));
        }
        if index > 0
            && prepared_member_order(
                &bundle.additional_members[index - 1],
                &bundle.additional_members[index],
            ) != std::cmp::Ordering::Less
        {
            return Err(AppError::blocked(
                "prepared state transition member order 불일치",
            ));
        }
    }
    if preserved_reconcile {
        let backup = &bundle.additional_members[0];
        let reason = if bundle.semantic_events[0].event_type == "state.reconcile.corrupt_recovered"
        {
            "corrupt"
        } else if bundle.semantic_events[0].event_type == "state.reconcile.stale_recovered" {
            "stale"
        } else {
            return Err(AppError::blocked(
                "prepared reconcile preserved reason 불일치",
            ));
        };
        let expected_path = format!("state/current-state.json.{reason}.{}", bundle.intent_id);
        if backup.path != expected_path
            || backup.expected_type != "absent"
            || sha256_bytes(backup.bytes_utf8.as_bytes()) != bundle.current_artifact_hash
            || backup.binding.causal_id.is_some()
        {
            return Err(AppError::blocked(
                "prepared reconcile preserved member binding 불일치",
            ));
        }
    }
    let current = bundle
        .additional_members
        .last()
        .ok_or_else(|| AppError::blocked("prepared state transition current 누락"))?;
    crate::app::workflow_adapter::state::validate_prepared_state_current_member(bundle, current)?;
    if checkpoint {
        let workflow_id = bundle
            .workflow_id
            .as_deref()
            .ok_or_else(|| AppError::blocked("prepared checkpoint workflow id 누락"))?;
        let prepared = crate::app::workflow_adapter::state::decode_prepared_workflow_revision(
            workflow_id,
            &bundle.additional_members[0],
            &bundle.additional_members[1],
            event,
        )?;
        let final_chain = bundle
            .event_chain_plan
            .last()
            .ok_or_else(|| AppError::blocked("prepared checkpoint final chain 누락"))?;
        let final_binding = crate::app::workflow_adapter::ledger::LedgerBinding {
            event_count: final_chain.ordinal,
            event_id: Some(final_chain.event_id.clone()),
            event_hash: final_chain.event_hash.clone(),
        };
        crate::app::workflow_adapter::state::decode_prepared_current_image(
            current,
            &prepared.record,
            &final_binding,
            &prepared.snapshot_member_id,
            &event.event_id,
        )?;
    }
    Ok(())
}

fn validate_verification_members(bundle: &PreparedSourceBundle) -> Result<(), AppError> {
    let expected_types = match bundle.intent_kind.as_str() {
        "approve-verification" => [
            "runtime.intent.accepted",
            "workflow.checkpoint",
            "patch.verification.approved",
        ],
        "deny-patch" => [
            "runtime.intent.accepted",
            "workflow.checkpoint",
            "patch.apply.denied",
        ],
        "deny-verification" => [
            "runtime.intent.accepted",
            "workflow.checkpoint",
            "patch.verification.denied",
        ],
        "cancel-workflow" => [
            "runtime.intent.accepted",
            "workflow.checkpoint",
            "workflow.user-cancelled",
        ],
        _ => return Err(AppError::blocked("prepared single revision intent 불일치")),
    };
    let expected_kinds = [
        PreparedMemberKind::WorkflowSnapshot,
        PreparedMemberKind::WorkflowPointer,
        PreparedMemberKind::CurrentImage,
    ];
    if bundle.additional_members.len() != expected_kinds.len()
        || bundle.semantic_events.len() != expected_types.len()
        || bundle.workflow_id.is_none()
        || bundle.projection_lag_member_index.is_some()
        || bundle
            .semantic_events
            .iter()
            .zip(expected_types)
            .any(|(event, expected)| event.event_type != expected)
    {
        return Err(AppError::blocked(
            "prepared verification approval exact shape 불일치",
        ));
    }
    let workflow_id = bundle.workflow_id.as_deref().expect("validated above");
    let mut artifact_ids = std::collections::BTreeSet::new();
    let mut paths = std::collections::BTreeSet::new();
    for (index, member) in bundle.additional_members.iter().enumerate() {
        if member.kind != expected_kinds[index]
            || member.semantic_role_rank != 0
            || member.binding.source_key.is_some()
            || member.readonly
            || member.mode != 0o600
            || member.ownership.is_some()
            || member.bytes_utf8.is_empty()
            || member.expected_type == "content-addressed-reference"
        {
            return Err(AppError::blocked(
                "prepared verification member kind/metadata 불일치",
            ));
        }
        let expected_schema = match member.kind {
            PreparedMemberKind::WorkflowSnapshot | PreparedMemberKind::WorkflowPointer => 4,
            PreparedMemberKind::CurrentImage => 2,
            _ => {
                return Err(AppError::blocked(
                    "prepared verification member kind 불일치",
                ))
            }
        };
        if member.schema_version != expected_schema
            || member
                .binding
                .artifact_id
                .as_deref()
                .is_none_or(|id| validate_ascii_id(id, "member artifact").is_err())
        {
            return Err(AppError::blocked(
                "prepared verification member schema/binding 불일치",
            ));
        }
        let limit = match member.kind {
            PreparedMemberKind::WorkflowSnapshot => 65_536,
            PreparedMemberKind::WorkflowPointer => 16_384,
            PreparedMemberKind::CurrentImage => 65_536,
            _ => unreachable!("verification member kind validated above"),
        };
        if member.bytes_utf8.len() > limit {
            return Err(AppError::blocked(
                "prepared verification member byte limit 초과",
            ));
        }
        validate_stored_path(&member.path)?;
        if member.kind == PreparedMemberKind::WorkflowPointer
            && member.path != format!(".rpotato/workflows/{workflow_id}.json")
        {
            return Err(AppError::blocked(
                "prepared verification workflow pointer path 불일치",
            ));
        }
        for (label, value) in [
            ("member causal", member.binding.causal_id.as_deref()),
            ("member event", member.binding.event_id.as_deref()),
        ] {
            if let Some(value) = value {
                validate_ascii_id(value, label)?;
            }
        }
        let artifact_id = member
            .binding
            .artifact_id
            .as_deref()
            .expect("validated above");
        if !artifact_ids.insert(artifact_id) || !paths.insert(member.path.as_str()) {
            return Err(AppError::blocked(
                "prepared verification member id/path 중복",
            ));
        }
        if index > 0
            && prepared_member_order(
                &bundle.additional_members[index - 1],
                &bundle.additional_members[index],
            ) != std::cmp::Ordering::Less
        {
            return Err(AppError::blocked(
                "prepared verification member total order 불일치",
            ));
        }
    }
    Ok(())
}

fn validate_projection_lag_member(
    bundle: &PreparedSourceBundle,
    lag: &PreparedMember,
) -> Result<(), AppError> {
    let object = strict_json::parse_canonical_object(
        &lag.bytes_utf8,
        PROJECTION_LAG_KEYS,
        "projection lag member",
    )?;
    let final_event = bundle
        .semantic_events
        .get(9)
        .ok_or_else(|| AppError::blocked("projection lag final event 누락"))?;
    let final_chain = bundle
        .event_chain_plan
        .get(9)
        .ok_or_else(|| AppError::blocked("projection lag final chain 누락"))?;
    let required_outputs = required_string_array(&object, "required_outputs")?;
    let required_event_ids = required_string_array(&object, "required_event_ids")?;
    let expected_event_ids = bundle
        .semantic_events
        .iter()
        .map(|event| event.event_id.clone())
        .collect::<Vec<_>>();
    let expected_path = format!(
        "state/projection-lag/{}-{}.json",
        bundle.intent_id, final_event.event_id
    );
    let hash = sha256_bytes(lag.bytes_utf8.as_bytes());
    if strict_json::canonical_u64(&object, "schema_version", "projection lag member")? != 1
        || required_string(&object, "intent_id")? != bundle.intent_id
        || required_string(&object, "event_id")? != final_event.event_id
        || strict_json::canonical_u64(&object, "event_ordinal", "projection lag member")?
            != final_chain.ordinal
        || required_string(&object, "event_hash")? != final_chain.event_hash
        || required_outputs
            != [
                "project-session-ledger".to_string(),
                "global-operation-log".to_string(),
                "sqlite".to_string(),
            ]
        || required_event_ids != expected_event_ids
        || lag.path != expected_path
        || lag.binding.artifact_id.as_deref() != Some(format!("projection-lag-{hash}").as_str())
        || lag.binding.causal_id.is_some()
        || lag.expected_type != "absent"
    {
        return Err(AppError::blocked(
            "prepared projection lag canonical/reference binding 불일치",
        ));
    }
    Ok(())
}

fn render_source_members(bundle: &PreparedSourceBundle) -> Result<String, AppError> {
    let Some(source) = bundle.source_install.as_ref() else {
        let members = bundle
            .additional_members
            .iter()
            .map(|member| render_additional_member(bundle, member))
            .collect::<Vec<_>>();
        return Ok(format!("[{}]", members.join(",")));
    };
    let before_bytes = bundle
        .before_bytes
        .as_deref()
        .ok_or_else(|| AppError::blocked("prepared source before bytes 누락"))?;
    let proposed_bytes = bundle
        .proposed_bytes
        .as_deref()
        .ok_or_else(|| AppError::blocked("prepared source proposed bytes 누락"))?;
    let mode = source.unix_metadata.before_mode;
    let owner = bundle.workflow_id.as_deref();
    let common_owner = |workflow_id: Option<&str>| {
        format!(
            "{{\"project_id\":\"{}\",\"session_id\":\"{}\",\"workflow_id\":{},\"intent_id\":\"{}\"}}",
            bundle.project_id,
            bundle.session_id,
            render_optional_string(workflow_id),
            bundle.intent_id
        )
    };
    let binding = |artifact_id: &str| {
        format!(
            "{{\"artifact_id\":\"{}\",\"causal_id\":null,\"source_key\":\"{}\",\"event_id\":null}}",
            artifact_id, source.source_key
        )
    };
    let member = |kind: &str,
                  path: &str,
                  artifact_id: &str,
                  bytes: Option<&str>,
                  byte_length: u64,
                  sha256: &str,
                  expected_type: &str,
                  expected_identity: Option<&str>,
                  ownership: Option<&str>| {
        format!(
            "{{\"member_kind\":\"{}\",\"path\":\"{}\",\"schema_version\":null,\"owner\":{},\"binding\":{},\"prepared_at_ms\":{},\"bytes_utf8\":{},\"byte_length\":{},\"sha256\":\"{}\",\"expected_type\":\"{}\",\"expected_identity\":{},\"permissions\":{{\"readonly\":false,\"mode\":{}}},\"ownership\":{}}}",
            kind,
            crate::app::workflow_adapter::ledger::json_string(path),
            common_owner(owner),
            binding(artifact_id),
            bundle.prepared_at_ms,
            render_optional_string(bytes),
            byte_length,
            sha256,
            expected_type,
            render_optional_string(expected_identity),
            mode,
            render_optional_string(ownership)
        )
    };
    let before = member(
        "before_blob",
        &source.before_blob.member_path,
        &source.before_blob.blob_id,
        Some(before_bytes),
        source.before_byte_length,
        &source.before_sha256,
        "file",
        source.target.expected_identity.as_deref(),
        Some(&source.ownership.before_owner),
    );
    let proposed = member(
        "proposed_blob",
        &source.proposed_blob.member_path,
        &source.proposed_blob.blob_id,
        Some(proposed_bytes),
        source.proposed_byte_length,
        &source.proposed_sha256,
        "file",
        None,
        Some(&source.ownership.install_owner),
    );
    let rollback = member(
        "rollback_ref",
        &source.rollback_final.path,
        &format!("rollback-ref-{}", source.source_key),
        None,
        source.before_byte_length,
        &source.before_sha256,
        "content-addressed-reference",
        Some(&source.before_sha256),
        Some(&source.ownership.before_owner),
    );
    let mut members = vec![before, proposed, rollback];
    members.extend(
        bundle
            .additional_members
            .iter()
            .map(|member| render_additional_member(bundle, member)),
    );
    Ok(format!("[{}]", members.join(",")))
}

fn render_additional_member(bundle: &PreparedSourceBundle, member: &PreparedMember) -> String {
    let binding = &member.binding;
    let byte_length = member.bytes_utf8.len();
    let hash = sha256_bytes(member.bytes_utf8.as_bytes());
    format!(
        "{{\"member_kind\":\"{}\",\"path\":\"{}\",\"schema_version\":{},\"owner\":{{\"project_id\":\"{}\",\"session_id\":\"{}\",\"workflow_id\":{},\"intent_id\":\"{}\"}},\"binding\":{{\"artifact_id\":{},\"causal_id\":{},\"source_key\":{},\"event_id\":{}}},\"prepared_at_ms\":{},\"bytes_utf8\":\"{}\",\"byte_length\":{},\"sha256\":\"{}\",\"expected_type\":\"{}\",\"expected_identity\":{},\"permissions\":{{\"readonly\":{},\"mode\":{}}},\"ownership\":{}}}",
        member.kind.as_str(),
        crate::app::workflow_adapter::ledger::json_string(&member.path),
        member.schema_version,
        bundle.project_id,
        bundle.session_id,
        render_optional_string(bundle.workflow_id.as_deref()),
        bundle.intent_id,
        render_optional_string(binding.artifact_id.as_deref()),
        render_optional_string(binding.causal_id.as_deref()),
        render_optional_string(binding.source_key.as_deref()),
        render_optional_string(binding.event_id.as_deref()),
        bundle.prepared_at_ms,
        crate::app::workflow_adapter::ledger::json_string(&member.bytes_utf8),
        byte_length,
        hash,
        member.expected_type,
        render_optional_string(member.expected_identity.as_deref()),
        member.readonly,
        member.mode,
        render_optional_string(member.ownership.as_deref()),
    )
}

fn parse_source_members(
    root: &CanonicalObject,
    source: &SourceInstallV1,
    context: &PreparedMemberParseContext<'_>,
) -> Result<(String, String, Vec<PreparedMember>), AppError> {
    let Some(CanonicalValue::Array(members)) = root.get("members") else {
        return Err(AppError::blocked("prepared source members 누락"));
    };
    if members.len() < 3 {
        return Err(AppError::blocked("prepared source members count 불일치"));
    }
    let expected = [
        (
            "before_blob",
            source.before_blob.member_path.as_str(),
            source.before_blob.blob_id.as_str(),
            source.before_sha256.as_str(),
            source.before_byte_length,
            true,
        ),
        (
            "proposed_blob",
            source.proposed_blob.member_path.as_str(),
            source.proposed_blob.blob_id.as_str(),
            source.proposed_sha256.as_str(),
            source.proposed_byte_length,
            true,
        ),
        (
            "rollback_ref",
            source.rollback_final.path.as_str(),
            "",
            source.before_sha256.as_str(),
            source.before_byte_length,
            false,
        ),
    ];
    let mut decoded = Vec::new();
    for (index, value) in members.iter().take(3).enumerate() {
        let CanonicalValue::Object(member) = value else {
            return Err(AppError::blocked("prepared source member type 불일치"));
        };
        require_keys(member, MEMBER_KEYS)?;
        let owner = required_object(member, "owner")?;
        require_keys(owner, OWNER_KEYS)?;
        if required_string(owner, "project_id")? != context.project_id
            || required_string(owner, "session_id")? != context.session_id
            || optional_string(owner, "workflow_id")?.as_deref() != context.workflow_id
            || required_string(owner, "intent_id")? != context.intent_id
        {
            return Err(AppError::blocked("prepared source member owner 불일치"));
        }
        let binding = required_object(member, "binding")?;
        require_keys(binding, BINDING_KEYS)?;
        let artifact_id = required_string(binding, "artifact_id")?;
        if optional_string(binding, "causal_id")?.is_some()
            || optional_string(binding, "source_key")?.as_deref()
                != Some(source.source_key.as_str())
            || optional_string(binding, "event_id")?.is_some()
        {
            return Err(AppError::blocked("prepared source member binding 불일치"));
        }
        let (kind, path, expected_artifact, hash, length, has_bytes) = expected[index];
        if required_string(member, "member_kind")? != kind
            || required_string(member, "path")? != path
            || (index < 2 && artifact_id != expected_artifact)
            || !matches!(member.get("schema_version"), Some(CanonicalValue::Null))
            || required_u128(member, "prepared_at_ms")? != context.prepared_at_ms
            || strict_json::canonical_u64(member, "byte_length", "prepared source member")?
                != length
            || required_string(member, "sha256")? != hash
        {
            return Err(AppError::blocked(
                "prepared source member scalar binding 불일치",
            ));
        }
        let permissions = required_object(member, "permissions")?;
        require_keys(permissions, MEMBER_PERMISSION_KEYS)?;
        let _ = required_bool(permissions, "readonly")?;
        let _ = required_u32(permissions, "mode")?;
        let bytes = optional_string(member, "bytes_utf8")?;
        if has_bytes != bytes.is_some() {
            return Err(AppError::blocked(
                "prepared source member bytes nullability 불일치",
            ));
        }
        if let Some(bytes) = bytes {
            if bytes.len() > MAX_SOURCE_BLOB_BYTES
                || sha256_bytes(bytes.as_bytes()) != hash
                || u64::try_from(bytes.len()).ok() != Some(length)
            {
                return Err(AppError::blocked(
                    "prepared source member embedded bytes 불일치",
                ));
            }
            decoded.push(bytes);
        }
    }
    let mut additional = Vec::with_capacity(members.len().saturating_sub(3));
    for value in members.iter().skip(3) {
        additional.push(parse_additional_member(value, context)?);
    }
    Ok((decoded.remove(0), decoded.remove(0), additional))
}

struct PreparedMemberParseContext<'a> {
    prepared_at_ms: u128,
    project_id: &'a str,
    session_id: &'a str,
    workflow_id: Option<&'a str>,
    intent_id: &'a str,
    intent_kind: &'a str,
    semantic_events: &'a [crate::app::workflow_adapter::ledger::LedgerEvent],
}

fn parse_additional_members(
    root: &CanonicalObject,
    context: &PreparedMemberParseContext<'_>,
) -> Result<Vec<PreparedMember>, AppError> {
    let Some(CanonicalValue::Array(members)) = root.get("members") else {
        return Err(AppError::blocked("prepared members type 불일치"));
    };
    members
        .iter()
        .map(|value| parse_additional_member(value, context))
        .collect()
}

fn parse_additional_member(
    value: &CanonicalValue,
    context: &PreparedMemberParseContext<'_>,
) -> Result<PreparedMember, AppError> {
    let CanonicalValue::Object(member) = value else {
        return Err(AppError::blocked("prepared member type 불일치"));
    };
    require_keys(member, MEMBER_KEYS)?;
    let owner = required_object(member, "owner")?;
    require_keys(owner, OWNER_KEYS)?;
    if required_string(owner, "project_id")? != context.project_id
        || required_string(owner, "session_id")? != context.session_id
        || optional_string(owner, "workflow_id")?.as_deref() != context.workflow_id
        || required_string(owner, "intent_id")? != context.intent_id
    {
        return Err(AppError::blocked("prepared member owner 불일치"));
    }
    let binding = required_object(member, "binding")?;
    require_keys(binding, BINDING_KEYS)?;
    let permissions = required_object(member, "permissions")?;
    require_keys(permissions, MEMBER_PERMISSION_KEYS)?;
    if required_u128(member, "prepared_at_ms")? != context.prepared_at_ms {
        return Err(AppError::blocked(
            "prepared member timestamp binding 불일치",
        ));
    }
    let bytes_utf8 = optional_string(member, "bytes_utf8")?
        .ok_or_else(|| AppError::blocked("prepared non-reference member bytes 누락"))?;
    let byte_length = strict_json::canonical_u64(member, "byte_length", "prepared member")?;
    if u64::try_from(bytes_utf8.len()).ok() != Some(byte_length)
        || sha256_bytes(bytes_utf8.as_bytes()) != required_string(member, "sha256")?
    {
        return Err(AppError::blocked(
            "prepared member byte/hash binding 불일치",
        ));
    }
    let kind = PreparedMemberKind::parse(&required_string(member, "member_kind")?)?;
    let event_id = optional_string(binding, "event_id")?;
    let semantic_role_rank = match kind {
        PreparedMemberKind::WorkflowSnapshot | PreparedMemberKind::WorkflowPointer => {
            derive_workflow_role_rank(
                event_id.as_deref(),
                context.intent_kind,
                context.semantic_events,
            )?
        }
        _ => 0,
    };
    Ok(PreparedMember {
        kind,
        path: required_string(member, "path")?,
        schema_version: strict_json::canonical_u64(member, "schema_version", "prepared member")?,
        binding: PreparedMemberBinding {
            artifact_id: optional_string(binding, "artifact_id")?,
            causal_id: optional_string(binding, "causal_id")?,
            source_key: optional_string(binding, "source_key")?,
            event_id,
        },
        bytes_utf8,
        expected_type: required_string(member, "expected_type")?,
        expected_identity: optional_string(member, "expected_identity")?,
        readonly: required_bool(permissions, "readonly")?,
        mode: required_u32(permissions, "mode")?,
        ownership: optional_string(member, "ownership")?,
        semantic_role_rank,
    })
}

fn render_semantic_events(events: &[crate::app::workflow_adapter::ledger::LedgerEvent]) -> String {
    let rows = events
        .iter()
        .map(render_semantic_event)
        .collect::<Vec<_>>()
        .join(",");
    format!("[{rows}]")
}

fn render_semantic_event(event: &crate::app::workflow_adapter::ledger::LedgerEvent) -> String {
    format!(
        "{{\"schema_version\":1,\"event_id\":\"{}\",\"ts_ms\":{},\"event_type\":\"{}\",\"project_id\":\"{}\",\"session_id\":\"{}\",\"summary\":\"{}\",\"details\":\"{}\"}}",
        crate::app::workflow_adapter::ledger::json_string(&event.event_id),
        event.ts_ms,
        crate::app::workflow_adapter::ledger::json_string(&event.event_type),
        crate::app::workflow_adapter::ledger::json_string(&event.project_id),
        crate::app::workflow_adapter::ledger::json_string(&event.session_id),
        crate::app::workflow_adapter::ledger::json_string(&event.summary),
        crate::app::workflow_adapter::ledger::json_string(&event.details),
    )
}

fn parse_semantic_events(
    object: &CanonicalObject,
) -> Result<Vec<crate::app::workflow_adapter::ledger::LedgerEvent>, AppError> {
    let Some(CanonicalValue::Array(values)) = object.get("semantic_events") else {
        return Err(AppError::blocked("prepared semantic_events type 불일치"));
    };
    values
        .iter()
        .map(|value| {
            let CanonicalValue::Object(event) = value else {
                return Err(AppError::blocked("prepared semantic event type 불일치"));
            };
            require_keys(event, SEMANTIC_EVENT_KEYS)?;
            if strict_json::canonical_u64(event, "schema_version", "semantic event")? != 1 {
                return Err(AppError::blocked("prepared semantic event schema 불일치"));
            }
            Ok(crate::app::workflow_adapter::ledger::LedgerEvent {
                event_id: required_string(event, "event_id")?,
                ts_ms: strict_json::canonical_u128(event, "ts_ms", "semantic event")?,
                event_type: required_string(event, "event_type")?,
                project_id: required_string(event, "project_id")?,
                session_id: required_string(event, "session_id")?,
                summary: required_string(event, "summary")?,
                details: required_string(event, "details")?,
            })
        })
        .collect()
}

fn render_event_chain_plan(plan: &[PreparedEventChain]) -> String {
    let rows = plan
        .iter()
        .map(|entry| {
            format!(
                "{{\"event_id\":\"{}\",\"ordinal\":{},\"previous_event_hash\":\"{}\",\"event_hash\":\"{}\"}}",
                crate::app::workflow_adapter::ledger::json_string(&entry.event_id),
                entry.ordinal,
                entry.previous_event_hash,
                entry.event_hash,
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("[{rows}]")
}

fn parse_event_chain_plan(object: &CanonicalObject) -> Result<Vec<PreparedEventChain>, AppError> {
    let Some(CanonicalValue::Array(values)) = object.get("event_chain_plan") else {
        return Err(AppError::blocked("prepared event_chain_plan type 불일치"));
    };
    values
        .iter()
        .map(|value| {
            let CanonicalValue::Object(entry) = value else {
                return Err(AppError::blocked("prepared event chain entry type 불일치"));
            };
            require_keys(entry, EVENT_CHAIN_PLAN_KEYS)?;
            Ok(PreparedEventChain {
                event_id: required_string(entry, "event_id")?,
                ordinal: strict_json::canonical_u64(entry, "ordinal", "event chain plan")?,
                previous_event_hash: required_string(entry, "previous_event_hash")?,
                event_hash: required_string(entry, "event_hash")?,
            })
        })
        .collect()
}

fn parse_projection_lag_reference(object: &CanonicalObject) -> Result<Option<u64>, AppError> {
    match object.get("projection_lag_v1") {
        Some(CanonicalValue::Null) => Ok(None),
        Some(CanonicalValue::Object(reference)) => {
            require_keys(reference, PROJECTION_LAG_REFERENCE_KEYS)?;
            if required_string(reference, "member_kind")? != "projection_lag" {
                return Err(AppError::blocked(
                    "prepared projection lag reference kind 불일치",
                ));
            }
            Ok(Some(strict_json::canonical_u64(
                reference,
                "member_index",
                "projection lag reference",
            )?))
        }
        _ => Err(AppError::blocked("prepared projection_lag_v1 type 불일치")),
    }
}

fn derive_workflow_role_rank(
    event_id: Option<&str>,
    intent_kind: &str,
    semantic_events: &[crate::app::workflow_adapter::ledger::LedgerEvent],
) -> Result<u8, AppError> {
    match intent_kind {
        "approve-patch" if semantic_events.len() == 10 => match event_id {
            Some(value) if value == semantic_events[1].event_id => Ok(0),
            Some(value) if value == semantic_events[9].event_id => Ok(1),
            _ => Err(AppError::blocked(
                "prepared workflow member event/role binding 불일치",
            )),
        },
        "approve-verification" if semantic_events.len() == 3 => match event_id {
            Some(value) if value == semantic_events[1].event_id => Ok(0),
            _ => Err(AppError::blocked(
                "prepared verification workflow member event binding 불일치",
            )),
        },
        kind if is_terminal_action_intent_kind(kind) && semantic_events.len() == 3 => {
            match event_id {
                Some(value) if value == semantic_events[1].event_id => Ok(0),
                _ => Err(AppError::blocked(
                    "prepared terminal workflow member event binding 불일치",
                )),
            }
        }
        "checkpoint-workflow" if semantic_events.len() == 1 => match event_id {
            Some(value) if value == semantic_events[0].event_id => Ok(0),
            _ => Err(AppError::blocked(
                "prepared checkpoint workflow member event binding 불일치",
            )),
        },
        _ => Err(AppError::blocked(
            "prepared workflow member event plan 불일치",
        )),
    }
}

fn prepared_member_order(left: &PreparedMember, right: &PreparedMember) -> std::cmp::Ordering {
    (
        left.kind.rank(),
        left.path.as_bytes(),
        left.semantic_role_rank,
        left.binding.artifact_id.as_deref(),
        left.binding.causal_id.as_deref(),
        left.binding.source_key.as_deref(),
        left.binding.event_id.as_deref(),
    )
        .cmp(&(
            right.kind.rank(),
            right.path.as_bytes(),
            right.semantic_role_rank,
            right.binding.artifact_id.as_deref(),
            right.binding.causal_id.as_deref(),
            right.binding.source_key.as_deref(),
            right.binding.event_id.as_deref(),
        ))
}

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
