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
const STATE_TRANSITION_INTENT_KINDS: &[&str] = &[
    "bootstrap",
    "checkpoint-workflow",
    "repair-workflow-pointer",
    "clear-terminal-workflow",
    "reconcile",
    "resume",
    "cancel",
    "start-session",
    "select-session",
    "record-event",
];
const TERMINAL_ACTION_INTENT_KINDS: &[&str] =
    &["deny-patch", "deny-verification", "cancel-workflow"];

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PreparedMemberKind {
    ToolOutput,
    TranscriptV2,
    WorkflowSnapshot,
    WorkflowPointer,
    CurrentImage,
    ProjectionLag,
}

impl PreparedMemberKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::ToolOutput => "tool_output",
            Self::TranscriptV2 => "transcript_v2",
            Self::WorkflowSnapshot => "workflow_snapshot",
            Self::WorkflowPointer => "workflow_pointer",
            Self::CurrentImage => "current_image",
            Self::ProjectionLag => "projection_lag",
        }
    }

    fn rank(self) -> u8 {
        match self {
            Self::ToolOutput => 3,
            Self::TranscriptV2 => 4,
            Self::WorkflowSnapshot => 5,
            Self::WorkflowPointer => 6,
            Self::CurrentImage => 7,
            Self::ProjectionLag => 8,
        }
    }

    fn parse(value: &str) -> Result<Self, AppError> {
        match value {
            "tool_output" => Ok(Self::ToolOutput),
            "transcript_v2" => Ok(Self::TranscriptV2),
            "workflow_snapshot" => Ok(Self::WorkflowSnapshot),
            "workflow_pointer" => Ok(Self::WorkflowPointer),
            "current_image" => Ok(Self::CurrentImage),
            "projection_lag" => Ok(Self::ProjectionLag),
            _ => Err(AppError::blocked("prepared member kind 불일치")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedMemberBinding {
    pub artifact_id: Option<String>,
    pub causal_id: Option<String>,
    pub source_key: Option<String>,
    pub event_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedMember {
    pub kind: PreparedMemberKind,
    pub path: String,
    pub schema_version: u64,
    pub binding: PreparedMemberBinding,
    pub bytes_utf8: String,
    pub expected_type: String,
    pub expected_identity: Option<String>,
    pub readonly: bool,
    pub mode: u32,
    pub ownership: Option<String>,
    pub semantic_role_rank: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedPath {
    pub namespace: String,
    pub path: String,
    pub parent: String,
    pub basename: String,
    pub expected_type: String,
    pub expected_identity: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedBlob {
    pub blob_id: String,
    pub member_path: String,
    pub sha256: String,
    pub byte_length: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourcePermissions {
    pub before_readonly: bool,
    pub install_readonly: bool,
    pub before_mode: u32,
    pub install_mode: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceOwnership {
    pub before_owner: String,
    pub install_owner: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnixSourceMetadata {
    pub before_mode: u32,
    pub install_mode: u32,
    pub before_uid: u32,
    pub before_gid: u32,
    pub install_uid: u32,
    pub install_gid: u32,
    pub before_dev: u64,
    pub before_ino: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceInstallV1 {
    pub schema_version: u64,
    pub source_key: String,
    pub target: PreparedPath,
    pub before_blob: PreparedBlob,
    pub proposed_blob: PreparedBlob,
    pub rollback_final: PreparedPath,
    pub install_temp: PreparedPath,
    pub guard_path: PreparedPath,
    pub before_sha256: String,
    pub before_byte_length: u64,
    pub proposed_sha256: String,
    pub proposed_byte_length: u64,
    pub permissions: SourcePermissions,
    pub ownership: SourceOwnership,
    pub platform: String,
    pub unix_metadata: UnixSourceMetadata,
    pub operations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedSourceBundle {
    pub intent_id: String,
    pub intent_kind: String,
    pub project_id: String,
    pub session_id: String,
    pub workflow_id: Option<String>,
    pub prepared_at_ms: u128,
    pub current_revision: u64,
    pub current_artifact_hash: String,
    pub ledger_binding: crate::ledger::LedgerBinding,
    pub source_install: Option<SourceInstallV1>,
    pub before_bytes: Option<String>,
    pub proposed_bytes: Option<String>,
    pub additional_members: Vec<PreparedMember>,
    pub semantic_events: Vec<crate::ledger::LedgerEvent>,
    pub event_chain_plan: Vec<PreparedEventChain>,
    pub projection_lag_member_index: Option<u64>,
}

pub(crate) struct PreparedBundleContext<'a> {
    pub identity: &'a crate::ledger::RuntimeIdentity,
    pub lease: &'a crate::state::CurrentStateLeaseView,
    pub ledger_binding: crate::ledger::LedgerBinding,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedEventChain {
    pub event_id: String,
    pub ordinal: u64,
    pub previous_event_hash: String,
    pub event_hash: String,
}

pub(crate) struct TransitionGuard {
    project_id: String,
    _lease: lease::RecoverableLease,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CurrentStateIntent {
    Bootstrap,
    CheckpointWorkflow,
    RecoverWorkflow,
    RepairWorkflowPointer,
    ClearTerminalWorkflow,
    Reconcile,
    ApprovePatch,
    ApproveVerification,
    Resume,
    Cancel,
    StartSession,
    SelectSession,
    RecordEvent,
}

impl TransitionGuard {
    pub(crate) fn acquire(project_id: &str) -> Result<Self, AppError> {
        validate_ascii_id(project_id, "project")?;
        fs::create_dir_all(paths::project_transition_journal_dir(project_id)).map_err(|err| {
            AppError::runtime(format!("transition journal directory 생성 실패: {err}"))
        })?;
        let lease = lease::RecoverableLease::acquire_with_wait(
            paths::project_transition_lock(project_id),
            "prepared transition journal",
            std::time::Duration::from_secs(5),
        )?;
        Ok(Self {
            project_id: project_id.to_string(),
            _lease: lease,
        })
    }

    pub(crate) fn acquire_for(
        project_id: &str,
        _intent: CurrentStateIntent,
    ) -> Result<Self, AppError> {
        let guard = Self::acquire(project_id)?;
        recover_pending_bundles_under_guard(project_id)?;
        Ok(guard)
    }

    pub(crate) fn commit(&self, bundle: &PreparedSourceBundle) -> Result<PathBuf, AppError> {
        if bundle.project_id != self.project_id {
            return Err(AppError::blocked(
                "transition guard/project bundle binding 불일치",
            ));
        }
        commit_prepared_source_bundle_under_guard(bundle)
    }

    pub(crate) fn remove(
        &self,
        bundle: &PreparedSourceBundle,
        path: &Path,
    ) -> Result<(), AppError> {
        if bundle.project_id != self.project_id {
            return Err(AppError::blocked(
                "transition cleanup guard/project binding 불일치",
            ));
        }
        remove_committed_source_bundle(bundle, path)
    }
}

impl CurrentStateIntent {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Bootstrap => "bootstrap",
            Self::CheckpointWorkflow => "checkpoint-workflow",
            Self::RecoverWorkflow => "recover-workflow",
            Self::RepairWorkflowPointer => "repair-workflow-pointer",
            Self::ClearTerminalWorkflow => "clear-terminal-workflow",
            Self::Reconcile => "reconcile",
            Self::ApprovePatch => "approve-patch",
            Self::ApproveVerification => "approve-verification",
            Self::Resume => "resume",
            Self::Cancel => "cancel",
            Self::StartSession => "start-session",
            Self::SelectSession => "select-session",
            Self::RecordEvent => "record-event",
        }
    }
}

fn is_state_transition_intent_kind(value: &str) -> bool {
    STATE_TRANSITION_INTENT_KINDS.contains(&value)
}

fn is_terminal_action_intent_kind(value: &str) -> bool {
    TERMINAL_ACTION_INTENT_KINDS.contains(&value)
}

pub(crate) fn prepare_state_transition_bundle(
    intent_id: &str,
    intent: CurrentStateIntent,
    identity: &crate::ledger::RuntimeIdentity,
    workflow_id: Option<&str>,
    current_revision: u64,
    current_artifact_hash: &str,
    ledger_binding: crate::ledger::LedgerBinding,
) -> Result<PreparedSourceBundle, AppError> {
    validate_ascii_id(intent_id, "intent")?;
    validate_ascii_id(&identity.project_id, "project")?;
    validate_ascii_id(&identity.session_id, "session")?;
    if let Some(workflow_id) = workflow_id {
        validate_ascii_id(workflow_id, "workflow")?;
    }
    let intent_kind = intent.as_str();
    if !is_state_transition_intent_kind(intent_kind) {
        return Err(AppError::blocked(
            "prepared state transition intent kind 불일치",
        ));
    }
    Ok(PreparedSourceBundle {
        intent_id: intent_id.to_string(),
        intent_kind: intent_kind.to_string(),
        project_id: identity.project_id.clone(),
        session_id: identity.session_id.clone(),
        workflow_id: workflow_id.map(str::to_string),
        prepared_at_ms: now_ms(),
        current_revision,
        current_artifact_hash: current_artifact_hash.to_string(),
        ledger_binding,
        source_install: None,
        before_bytes: None,
        proposed_bytes: None,
        additional_members: Vec::new(),
        semantic_events: Vec::new(),
        event_chain_plan: Vec::new(),
        projection_lag_member_index: None,
    })
}

pub(crate) fn prepare_source_bundle(
    intent_id: &str,
    workflow_id: Option<&str>,
    source_install: SourceInstallV1,
    before: &[u8],
    proposed: &[u8],
) -> Result<PreparedSourceBundle, AppError> {
    let identity = crate::ledger::validated_current_identity()?;
    let lease = crate::state::current_state_lease_view()?;
    let ledger_binding = crate::ledger::validated_ledger_binding()?;
    prepare_source_bundle_with_context(
        intent_id,
        workflow_id,
        source_install,
        before,
        proposed,
        PreparedBundleContext {
            identity: &identity,
            lease: &lease,
            ledger_binding,
        },
    )
}

pub(crate) fn prepare_source_bundle_with_context(
    intent_id: &str,
    workflow_id: Option<&str>,
    source_install: SourceInstallV1,
    before: &[u8],
    proposed: &[u8],
    context: PreparedBundleContext<'_>,
) -> Result<PreparedSourceBundle, AppError> {
    validate_ascii_id(intent_id, "intent")?;
    if let Some(workflow_id) = workflow_id {
        validate_ascii_id(workflow_id, "workflow")?;
    }
    validate_source_install_v1(&source_install)?;
    enforce_byte_limit(
        before.len(),
        MAX_SOURCE_BLOB_BYTES,
        "prepared source blob byte limit 초과",
    )?;
    enforce_byte_limit(
        proposed.len(),
        MAX_SOURCE_BLOB_BYTES,
        "prepared source blob byte limit 초과",
    )?;
    let before_bytes = std::str::from_utf8(before)
        .map_err(|_| AppError::blocked("prepared before blob는 UTF-8이어야 합니다."))?
        .to_string();
    let proposed_bytes = std::str::from_utf8(proposed)
        .map_err(|_| AppError::blocked("prepared proposed blob는 UTF-8이어야 합니다."))?
        .to_string();
    if sha256_bytes(before) != source_install.before_sha256
        || sha256_bytes(proposed) != source_install.proposed_sha256
    {
        return Err(AppError::blocked(
            "prepared source blob hash binding 불일치",
        ));
    }
    Ok(PreparedSourceBundle {
        intent_id: intent_id.to_string(),
        intent_kind: "approve-patch".to_string(),
        project_id: context.identity.project_id.clone(),
        session_id: context.identity.session_id.clone(),
        workflow_id: workflow_id.map(str::to_string),
        prepared_at_ms: now_ms(),
        current_revision: context.lease.revision,
        current_artifact_hash: context.lease.artifact_hash.clone(),
        ledger_binding: context.ledger_binding,
        source_install: Some(source_install),
        before_bytes: Some(before_bytes),
        proposed_bytes: Some(proposed_bytes),
        additional_members: Vec::new(),
        semantic_events: Vec::new(),
        event_chain_plan: Vec::new(),
        projection_lag_member_index: None,
    })
}

pub(crate) fn prepare_workflow_bundle_with_context(
    intent_id: &str,
    intent_kind: &str,
    workflow_id: &str,
    context: PreparedBundleContext<'_>,
) -> Result<PreparedSourceBundle, AppError> {
    validate_ascii_id(intent_id, "intent")?;
    validate_ascii_id(workflow_id, "workflow")?;
    if intent_kind != "approve-verification" {
        return Err(AppError::blocked("prepared workflow intent kind 불일치"));
    }
    Ok(PreparedSourceBundle {
        intent_id: intent_id.to_string(),
        intent_kind: intent_kind.to_string(),
        project_id: context.identity.project_id.clone(),
        session_id: context.identity.session_id.clone(),
        workflow_id: Some(workflow_id.to_string()),
        prepared_at_ms: now_ms(),
        current_revision: context.lease.revision,
        current_artifact_hash: context.lease.artifact_hash.clone(),
        ledger_binding: context.ledger_binding,
        source_install: None,
        before_bytes: None,
        proposed_bytes: None,
        additional_members: Vec::new(),
        semantic_events: Vec::new(),
        event_chain_plan: Vec::new(),
        projection_lag_member_index: None,
    })
}

pub(crate) fn prepare_terminal_action_bundle_with_context(
    intent_id: &str,
    intent_kind: &str,
    workflow_id: &str,
    source: Option<(SourceInstallV1, &[u8], &[u8])>,
    context: PreparedBundleContext<'_>,
) -> Result<PreparedSourceBundle, AppError> {
    validate_ascii_id(intent_id, "intent")?;
    validate_ascii_id(workflow_id, "workflow")?;
    if !is_terminal_action_intent_kind(intent_kind) {
        return Err(AppError::blocked(
            "prepared terminal action intent kind 불일치",
        ));
    }
    let (source_install, before_bytes, proposed_bytes) = match source {
        Some((plan, before, proposed)) => {
            validate_source_install_v1(&plan)?;
            let before = std::str::from_utf8(before)
                .map_err(|_| AppError::blocked("terminal source before UTF-8 불일치"))?
                .to_string();
            let proposed = std::str::from_utf8(proposed)
                .map_err(|_| AppError::blocked("terminal source proposed UTF-8 불일치"))?
                .to_string();
            if sha256_bytes(before.as_bytes()) != plan.before_sha256
                || sha256_bytes(proposed.as_bytes()) != plan.proposed_sha256
            {
                return Err(AppError::blocked(
                    "prepared terminal source hash binding 불일치",
                ));
            }
            (Some(plan), Some(before), Some(proposed))
        }
        None => (None, None, None),
    };
    if intent_kind == "deny-patch" && source_install.is_some()
        || intent_kind == "deny-verification" && source_install.is_none()
    {
        return Err(AppError::blocked(
            "prepared terminal source intent/nullability 불일치",
        ));
    }
    Ok(PreparedSourceBundle {
        intent_id: intent_id.to_string(),
        intent_kind: intent_kind.to_string(),
        project_id: context.identity.project_id.clone(),
        session_id: context.identity.session_id.clone(),
        workflow_id: Some(workflow_id.to_string()),
        prepared_at_ms: now_ms(),
        current_revision: context.lease.revision,
        current_artifact_hash: context.lease.artifact_hash.clone(),
        ledger_binding: context.ledger_binding,
        source_install,
        before_bytes,
        proposed_bytes,
        additional_members: Vec::new(),
        semantic_events: Vec::new(),
        event_chain_plan: Vec::new(),
        projection_lag_member_index: None,
    })
}

pub(crate) fn bind_additional_members(
    bundle: &mut PreparedSourceBundle,
    mut members: Vec<PreparedMember>,
) -> Result<(), AppError> {
    members.sort_by(prepared_member_order);
    let source_member_count = if bundle.source_install.is_some() {
        3
    } else {
        0
    };
    bundle.projection_lag_member_index = members
        .iter()
        .position(|member| member.kind == PreparedMemberKind::ProjectionLag)
        .map(|index| {
            u64::try_from(index + source_member_count)
                .map_err(|_| AppError::blocked("prepared projection lag index overflow"))
        })
        .transpose()?;
    bundle.additional_members = members;
    validate_prepared_source_bundle(bundle)
}

pub(crate) fn prepare_projection_lag_member(
    intent_id: &str,
    planned: &[crate::ledger::PlannedEvent],
) -> Result<PreparedMember, AppError> {
    validate_ascii_id(intent_id, "intent")?;
    if planned.len() != 10 {
        return Err(AppError::blocked(
            "projection lag는 exact E0..E9 plan이 필요합니다.",
        ));
    }
    let final_event = &planned[9];
    let required_event_ids = planned
        .iter()
        .map(|entry| format!("\"{}\"", crate::ledger::json_string(&entry.event.event_id)))
        .collect::<Vec<_>>()
        .join(",");
    let bytes_utf8 = format!(
        "{{\"schema_version\":1,\"intent_id\":\"{}\",\"event_id\":\"{}\",\"event_ordinal\":{},\"event_hash\":\"{}\",\"required_outputs\":[\"project-session-ledger\",\"global-operation-log\",\"sqlite\"],\"required_event_ids\":[{}]}}",
        crate::ledger::json_string(intent_id),
        crate::ledger::json_string(&final_event.event.event_id),
        final_event.ordinal,
        final_event.event_hash,
        required_event_ids,
    );
    let hash = sha256_bytes(bytes_utf8.as_bytes());
    Ok(PreparedMember {
        kind: PreparedMemberKind::ProjectionLag,
        path: format!(
            "state/projection-lag/{}-{}.json",
            intent_id, final_event.event.event_id
        ),
        schema_version: 1,
        binding: PreparedMemberBinding {
            artifact_id: Some(format!("projection-lag-{hash}")),
            causal_id: None,
            source_key: None,
            event_id: Some(final_event.event.event_id.clone()),
        },
        bytes_utf8,
        expected_type: "absent".to_string(),
        expected_identity: None,
        readonly: false,
        mode: 0o600,
        ownership: None,
        semantic_role_rank: 0,
    })
}

pub(crate) fn install_projection_lag(bundle: &PreparedSourceBundle) -> Result<PathBuf, AppError> {
    validate_prepared_source_bundle(bundle)?;
    let member = bundle
        .additional_members
        .iter()
        .find(|member| member.kind == PreparedMemberKind::ProjectionLag)
        .ok_or_else(|| AppError::blocked("prepared projection lag member 누락"))?;
    let event_id = member
        .binding
        .event_id
        .as_deref()
        .ok_or_else(|| AppError::blocked("prepared projection lag event binding 누락"))?;
    let path = paths::projection_lag_file(&bundle.intent_id, event_id);
    let expected_stored = format!(
        "state/projection-lag/{}-{}.json",
        bundle.intent_id, event_id
    );
    if member.path != expected_stored {
        return Err(AppError::blocked(
            "prepared projection lag path binding 불일치",
        ));
    }
    if path.exists() {
        let existing = fs::read_to_string(&path)
            .map_err(|err| AppError::blocked(format!("projection lag reread 실패: {err}")))?;
        if existing != member.bytes_utf8 {
            return Err(AppError::blocked("projection lag immutable conflict"));
        }
        return Ok(path);
    }
    let parent = path
        .parent()
        .ok_or_else(|| AppError::blocked("projection lag parent 누락"))?;
    fs::create_dir_all(parent)
        .map_err(|err| AppError::runtime(format!("projection lag directory 생성 실패: {err}")))?;
    let temporary = path.with_extension("json.tmp");
    if temporary.exists() {
        let existing = fs::read_to_string(&temporary)
            .map_err(|err| AppError::blocked(format!("projection lag temp reread 실패: {err}")))?;
        if existing != member.bytes_utf8 {
            return Err(AppError::blocked("projection lag temp immutable conflict"));
        }
    } else {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        use std::io::Write;
        let mut file = options
            .open(&temporary)
            .map_err(|err| AppError::runtime(format!("projection lag temp 생성 실패: {err}")))?;
        projection_lag_fault("temp-create")?;
        file.write_all(member.bytes_utf8.as_bytes())
            .map_err(|err| AppError::runtime(format!("projection lag temp write 실패: {err}")))?;
        projection_lag_fault("temp-write")?;
        file.sync_all()
            .map_err(|err| AppError::runtime(format!("projection lag temp fsync 실패: {err}")))?;
        projection_lag_fault("temp-fsync")?;
    }
    fs::rename(&temporary, &path)
        .map_err(|err| AppError::runtime(format!("projection lag rename 실패: {err}")))?;
    projection_lag_fault("rename")?;
    projection_lag_fault("parent-fsync")?;
    sync_parent(&path)?;
    Ok(path)
}

pub(crate) fn projection_lag_path(bundle: &PreparedSourceBundle) -> Result<PathBuf, AppError> {
    validate_prepared_source_bundle(bundle)?;
    let member = bundle
        .additional_members
        .iter()
        .find(|member| member.kind == PreparedMemberKind::ProjectionLag)
        .ok_or_else(|| AppError::blocked("prepared projection lag member 누락"))?;
    let event_id = member
        .binding
        .event_id
        .as_deref()
        .ok_or_else(|| AppError::blocked("prepared projection lag event binding 누락"))?;
    Ok(paths::projection_lag_file(&bundle.intent_id, event_id))
}

pub(crate) fn remove_projection_lag(bundle: &PreparedSourceBundle) -> Result<(), AppError> {
    validate_prepared_source_bundle(bundle)?;
    let member = bundle
        .additional_members
        .iter()
        .find(|member| member.kind == PreparedMemberKind::ProjectionLag)
        .ok_or_else(|| AppError::blocked("prepared projection lag member 누락"))?;
    let path = projection_lag_path(bundle)?;
    let temporary = path.with_extension("json.tmp");
    if temporary.exists() {
        let existing = fs::read_to_string(&temporary).map_err(|err| {
            AppError::blocked(format!("projection lag temp cleanup read 실패: {err}"))
        })?;
        if existing != member.bytes_utf8 {
            return Err(AppError::blocked("projection lag temp cleanup conflict"));
        }
        fs::remove_file(&temporary)
            .map_err(|err| AppError::runtime(format!("projection lag temp cleanup 실패: {err}")))?;
        sync_parent(&temporary)?;
    }
    if !path.exists() {
        return Ok(());
    }
    let existing = fs::read_to_string(&path)
        .map_err(|err| AppError::blocked(format!("projection lag cleanup read 실패: {err}")))?;
    if existing != member.bytes_utf8 {
        return Err(AppError::blocked(
            "projection lag cleanup immutable conflict",
        ));
    }
    fs::remove_file(&path)
        .map_err(|err| AppError::runtime(format!("projection lag cleanup 실패: {err}")))?;
    let cleanup = projection_lag_fault("lag-remove")
        .and_then(|_| projection_lag_fault("lag-parent-fsync"))
        .and_then(|_| sync_parent(&path));
    if let Err(error) = cleanup {
        restore_removed_file(&path, member.bytes_utf8.as_bytes(), "projection lag")?;
        return Err(error);
    }
    Ok(())
}

fn projection_lag_fault(point: &str) -> Result<(), AppError> {
    if cfg!(debug_assertions)
        && std::env::var("RPOTATO_TEST_PROJECTION_LAG_FAULT").as_deref() == Ok(point)
    {
        return Err(AppError::runtime(format!(
            "injected projection lag fault: {point}"
        )));
    }
    Ok(())
}

fn restore_removed_file(path: &Path, bytes: &[u8], label: &str) -> Result<(), AppError> {
    if path.exists() {
        if fs::read(path)
            .map_err(|err| AppError::runtime(format!("{label} restore reread 실패: {err}")))?
            != bytes
        {
            return Err(AppError::blocked(format!(
                "{label} restore immutable conflict"
            )));
        }
        return Ok(());
    }
    let temporary = path.with_extension("restore.tmp");
    if temporary.exists() {
        fs::remove_file(&temporary).map_err(|err| {
            AppError::runtime(format!("{label} restore temp cleanup 실패: {err}"))
        })?;
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    let mut file = options
        .open(&temporary)
        .map_err(|err| AppError::runtime(format!("{label} restore temp 생성 실패: {err}")))?;
    file.write_all(bytes)
        .map_err(|err| AppError::runtime(format!("{label} restore write 실패: {err}")))?;
    file.sync_all()
        .map_err(|err| AppError::runtime(format!("{label} restore fsync 실패: {err}")))?;
    fs::rename(&temporary, path)
        .map_err(|err| AppError::runtime(format!("{label} restore rename 실패: {err}")))?;
    sync_parent(path)
}

pub(crate) fn planned_events(
    bundle: &PreparedSourceBundle,
) -> Result<Vec<crate::ledger::PlannedEvent>, AppError> {
    validate_prepared_source_bundle(bundle)?;
    Ok(bundle
        .semantic_events
        .iter()
        .cloned()
        .zip(bundle.event_chain_plan.iter())
        .map(|(event, chain)| crate::ledger::PlannedEvent {
            event,
            ordinal: chain.ordinal,
            previous_event_hash: chain.previous_event_hash.clone(),
            event_hash: chain.event_hash.clone(),
        })
        .collect())
}

pub(crate) fn bind_planned_events(
    bundle: &mut PreparedSourceBundle,
    planned: &[crate::ledger::PlannedEvent],
) -> Result<(), AppError> {
    bundle.semantic_events = planned.iter().map(|entry| entry.event.clone()).collect();
    bundle.event_chain_plan = planned
        .iter()
        .map(|entry| PreparedEventChain {
            event_id: entry.event.event_id.clone(),
            ordinal: entry.ordinal,
            previous_event_hash: entry.previous_event_hash.clone(),
            event_hash: entry.event_hash.clone(),
        })
        .collect();
    validate_event_chain(bundle)
}

pub(crate) fn render_prepared_source_bundle(
    bundle: &PreparedSourceBundle,
) -> Result<String, AppError> {
    validate_prepared_source_bundle(bundle)?;
    let source = bundle
        .source_install
        .as_ref()
        .map(render_source_install_v1)
        .transpose()?
        .unwrap_or_else(|| "null".to_string());
    let members = render_source_members(bundle)?;
    let semantic_events = render_semantic_events(&bundle.semantic_events);
    let event_chain_plan = render_event_chain_plan(&bundle.event_chain_plan);
    let projection_lag = bundle
        .projection_lag_member_index
        .map(|index| format!("{{\"member_kind\":\"projection_lag\",\"member_index\":{index}}}"))
        .unwrap_or_else(|| "null".to_string());
    let body = format!(
        "{{\"schema_version\":1,\"intent_id\":\"{}\",\"intent_kind\":\"{}\",\"project_id\":\"{}\",\"session_id\":\"{}\",\"workflow_id\":{},\"prepared_at_ms\":{},\"before_binding\":{{\"current_revision\":{},\"current_artifact_hash\":\"{}\",\"ledger_count\":{},\"ledger_event_id\":{},\"ledger_hash\":\"{}\"}},\"members\":{},\"semantic_events\":{},\"event_chain_plan\":{},\"source_install_v1\":{},\"projection_lag_v1\":{}}}",
        crate::ledger::json_string(&bundle.intent_id),
        bundle.intent_kind,
        crate::ledger::json_string(&bundle.project_id),
        crate::ledger::json_string(&bundle.session_id),
        render_optional_string(bundle.workflow_id.as_deref()),
        bundle.prepared_at_ms,
        bundle.current_revision,
        bundle.current_artifact_hash,
        bundle.ledger_binding.event_count,
        render_optional_string(bundle.ledger_binding.event_id.as_deref()),
        bundle.ledger_binding.event_hash,
        members,
        semantic_events,
        event_chain_plan,
        source,
        projection_lag,
    );
    enforce_byte_limit(
        body.len(),
        MAX_PREPARED_BUNDLE_BYTES,
        "prepared bundle byte limit 초과",
    )?;
    Ok(body)
}

pub(crate) fn parse_prepared_source_bundle(body: &str) -> Result<PreparedSourceBundle, AppError> {
    enforce_byte_limit(
        body.len(),
        MAX_PREPARED_BUNDLE_BYTES,
        "prepared bundle byte limit 초과",
    )?;
    let object =
        strict_json::parse_canonical_object(body, PREPARED_BUNDLE_KEYS, "prepared source bundle")?;
    if strict_json::canonical_u64(&object, "schema_version", "prepared source bundle")? != 1 {
        return Err(AppError::blocked(
            "prepared source bundle schema/kind 불일치",
        ));
    }
    let intent_kind = required_string(&object, "intent_kind")?;
    if !matches!(
        intent_kind.as_str(),
        "approve-patch" | "approve-verification"
    ) && !is_state_transition_intent_kind(&intent_kind)
        && !is_terminal_action_intent_kind(&intent_kind)
    {
        return Err(AppError::blocked(
            "prepared source bundle intent kind 불일치",
        ));
    }
    let workflow_id = optional_string(&object, "workflow_id")?;
    let before_binding = required_object(&object, "before_binding")?;
    require_keys(before_binding, BEFORE_BINDING_KEYS)?;
    let source_install = match object.get("source_install_v1") {
        Some(CanonicalValue::Object(source_object)) => Some(parse_source_install_v1(
            &strict_json::render_canonical_object(source_object),
        )?),
        Some(CanonicalValue::Null) => None,
        _ => return Err(AppError::blocked("prepared source_install_v1 type 불일치")),
    };
    let semantic_events = parse_semantic_events(&object)?;
    let prepared_at_ms = required_u128(&object, "prepared_at_ms")?;
    let project_id = required_string(&object, "project_id")?;
    let session_id = required_string(&object, "session_id")?;
    let intent_id = required_string(&object, "intent_id")?;
    let member_context = PreparedMemberParseContext {
        prepared_at_ms,
        project_id: &project_id,
        session_id: &session_id,
        workflow_id: workflow_id.as_deref(),
        intent_id: &intent_id,
        intent_kind: &intent_kind,
        semantic_events: &semantic_events,
    };
    let (before_bytes, proposed_bytes, additional_members) =
        if let Some(source) = source_install.as_ref() {
            let (before, proposed, additional) =
                parse_source_members(&object, source, &member_context)?;
            (Some(before), Some(proposed), additional)
        } else {
            (
                None,
                None,
                parse_additional_members(&object, &member_context)?,
            )
        };
    let event_chain_plan = parse_event_chain_plan(&object)?;
    let projection_lag_member_index = parse_projection_lag_reference(&object)?;
    let bundle = PreparedSourceBundle {
        intent_id,
        intent_kind,
        project_id,
        session_id,
        workflow_id,
        prepared_at_ms,
        current_revision: strict_json::canonical_u64(
            before_binding,
            "current_revision",
            "prepared source bundle",
        )?,
        current_artifact_hash: required_string(before_binding, "current_artifact_hash")?,
        ledger_binding: crate::ledger::LedgerBinding {
            event_count: strict_json::canonical_u64(
                before_binding,
                "ledger_count",
                "prepared source bundle",
            )?,
            event_id: optional_string(before_binding, "ledger_event_id")?,
            event_hash: required_string(before_binding, "ledger_hash")?,
        },
        source_install,
        before_bytes,
        proposed_bytes,
        additional_members,
        semantic_events,
        event_chain_plan,
        projection_lag_member_index,
    };
    validate_prepared_source_bundle(&bundle)?;
    if render_prepared_source_bundle(&bundle)? != body {
        return Err(AppError::blocked(
            "prepared source bundle canonical re-render 불일치",
        ));
    }
    Ok(bundle)
}

pub(crate) fn commit_prepared_source_bundle(
    bundle: &PreparedSourceBundle,
) -> Result<PathBuf, AppError> {
    let guard = TransitionGuard::acquire_for(&bundle.project_id, CurrentStateIntent::ApprovePatch)?;
    guard.commit(bundle)
}

fn commit_prepared_source_bundle_under_guard(
    bundle: &PreparedSourceBundle,
) -> Result<PathBuf, AppError> {
    let body = render_prepared_source_bundle(bundle)?;
    let final_path = paths::project_transition_journal_file(&bundle.project_id, &bundle.intent_id);
    let temp_path = paths::project_transition_journal_temp(&bundle.project_id, &bundle.intent_id);
    validate_no_competing_prepared_journal(bundle, &final_path, &temp_path)?;
    if final_path.exists() {
        let existing = fs::read_to_string(&final_path)
            .map_err(|err| AppError::blocked(format!("prepared journal 읽기 실패: {err}")))?;
        let parsed = parse_prepared_source_bundle(&existing)?;
        if parsed != *bundle || existing != body {
            return Err(AppError::blocked("prepared journal immutable conflict"));
        }
        if temp_path.exists() {
            let temp = fs::read_to_string(&temp_path)
                .map_err(|err| AppError::blocked(format!("prepared temp 읽기 실패: {err}")))?;
            if temp != existing {
                return Err(AppError::blocked("prepared journal/temp conflict"));
            }
            fs::remove_file(&temp_path)
                .map_err(|err| AppError::runtime(format!("prepared temp cleanup 실패: {err}")))?;
            sync_parent(&temp_path)?;
        }
        return Ok(final_path);
    }
    if temp_path.exists() {
        let temp = fs::read_to_string(&temp_path)
            .map_err(|err| AppError::blocked(format!("prepared temp 읽기 실패: {err}")))?;
        if temp != body {
            return Err(AppError::blocked("prepared temp immutable conflict"));
        }
        parse_prepared_source_bundle(&temp)?;
    } else {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        use std::io::Write;
        let mut file = options
            .open(&temp_path)
            .map_err(|err| AppError::runtime(format!("prepared temp create-new 실패: {err}")))?;
        file.write_all(body.as_bytes())
            .map_err(|err| AppError::runtime(format!("prepared temp write 실패: {err}")))?;
        file.sync_all()
            .map_err(|err| AppError::runtime(format!("prepared temp fsync 실패: {err}")))?;
    }
    fs::rename(&temp_path, &final_path)
        .map_err(|err| AppError::runtime(format!("prepared journal rename 실패: {err}")))?;
    sync_parent(&final_path)?;
    let installed = fs::read_to_string(&final_path)
        .map_err(|err| AppError::blocked(format!("prepared journal reread 실패: {err}")))?;
    if installed != body || parse_prepared_source_bundle(&installed)? != *bundle {
        return Err(AppError::blocked("prepared journal installed bytes 불일치"));
    }
    Ok(final_path)
}

fn validate_no_competing_prepared_journal(
    bundle: &PreparedSourceBundle,
    final_path: &Path,
    temp_path: &Path,
) -> Result<(), AppError> {
    let directory = paths::project_transition_journal_dir(&bundle.project_id);
    for entry in fs::read_dir(&directory)
        .map_err(|err| AppError::blocked(format!("transition journal discovery 실패: {err}")))?
    {
        let entry = entry
            .map_err(|err| AppError::blocked(format!("transition journal entry 실패: {err}")))?;
        let path = entry.path();
        if path == final_path || path == temp_path {
            continue;
        }
        let name = entry
            .file_name()
            .to_str()
            .ok_or_else(|| AppError::blocked("transition journal filename UTF-8 불일치"))?
            .to_string();
        if name == "transition.lock" {
            continue;
        }
        if name.ends_with(".prepared.json") || name.ends_with(".prepared.json.tmp") {
            return Err(AppError::blocked(format!(
                "competing prepared journal 차단\n- pending: {name}\n- requested intent: {}\n- 동작: 새 journal을 만들지 않았습니다.",
                bundle.intent_id
            )));
        }
        return Err(AppError::blocked(format!(
            "unknown transition journal entry 보존: {name}"
        )));
    }
    Ok(())
}

pub(crate) fn remove_committed_source_bundle(
    bundle: &PreparedSourceBundle,
    path: &Path,
) -> Result<(), AppError> {
    let expected = paths::project_transition_journal_file(&bundle.project_id, &bundle.intent_id);
    if path != expected {
        return Err(AppError::blocked(
            "prepared journal cleanup path binding 불일치",
        ));
    }
    let body = fs::read_to_string(path)
        .map_err(|err| AppError::blocked(format!("prepared journal cleanup read 실패: {err}")))?;
    if parse_prepared_source_bundle(&body)? != *bundle {
        return Err(AppError::blocked("prepared journal cleanup binding 불일치"));
    }
    fs::remove_file(path)
        .map_err(|err| AppError::runtime(format!("prepared journal cleanup 실패: {err}")))?;
    let cleanup = projection_lag_fault("journal-remove")
        .and_then(|_| projection_lag_fault("journal-parent-fsync"))
        .and_then(|_| sync_parent(path));
    if let Err(error) = cleanup {
        restore_removed_file(path, body.as_bytes(), "prepared journal")?;
        return Err(error);
    }
    Ok(())
}

pub(crate) fn validate_committed_bundle_cleanup_authority(
    bundle: &PreparedSourceBundle,
    journal: &Path,
) -> Result<(), AppError> {
    validate_prepared_source_bundle(bundle)?;
    let expected = paths::project_transition_journal_file(&bundle.project_id, &bundle.intent_id);
    if journal != expected {
        return Err(AppError::blocked(
            "prepared cleanup journal path binding 불일치",
        ));
    }
    let body = fs::read_to_string(journal)
        .map_err(|err| AppError::blocked(format!("prepared cleanup journal 읽기 실패: {err}")))?;
    if parse_prepared_source_bundle(&body)? != *bundle {
        return Err(AppError::blocked(
            "prepared cleanup journal bytes binding 불일치",
        ));
    }
    if let Some(member) = bundle
        .additional_members
        .iter()
        .find(|member| member.kind == PreparedMemberKind::ProjectionLag)
    {
        let name = Path::new(&member.path)
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| AppError::blocked("prepared cleanup lag filename 불일치"))?;
        let path = paths::projection_lag_dir().join(name);
        let temporary = path.with_extension("json.tmp");
        if temporary.exists() {
            return Err(AppError::blocked(
                "prepared cleanup lag temp가 남아 있어 증거를 보존했습니다.",
            ));
        }
        if path.exists()
            && fs::read(&path).map_err(|err| {
                AppError::blocked(format!("prepared cleanup lag 읽기 실패: {err}"))
            })? != member.bytes_utf8.as_bytes()
        {
            return Err(AppError::blocked(
                "prepared cleanup lag/member binding 불일치",
            ));
        }
    }
    Ok(())
}

pub(crate) fn recover_pending_source_bundles() -> Result<usize, AppError> {
    if !recovery_work_may_exist() {
        return Ok(0);
    }
    let identity = if paths::current_state_file().exists() {
        crate::ledger::validated_current_identity()?
    } else {
        crate::ledger::fresh_identity()
    };
    let _guard = TransitionGuard::acquire(&identity.project_id)?;
    recover_pending_bundles_under_guard(&identity.project_id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectionLagReadStatus {
    Clear,
    Lagging,
    Unavailable,
}

pub(crate) fn projection_lag_status_read_only(project_id: &str) -> ProjectionLagReadStatus {
    let journal_directory = paths::project_transition_journal_dir(project_id);
    match validate_projection_lag_authority(project_id, &journal_directory) {
        Ok(false) => ProjectionLagReadStatus::Clear,
        Ok(true) => ProjectionLagReadStatus::Lagging,
        Err(_) => ProjectionLagReadStatus::Unavailable,
    }
}

struct BoundedRegularEntry {
    name: String,
    path: PathBuf,
}

fn bounded_regular_entries(
    directory: &Path,
    max_entries: usize,
    max_bytes: usize,
    allowed_name: impl Fn(&str) -> bool,
) -> Result<Vec<BoundedRegularEntry>, std::io::Error> {
    let mut entries = Vec::new();
    let mut bytes = 0_usize;
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        let name = entry
            .file_name()
            .to_str()
            .ok_or_else(|| std::io::Error::other("non-UTF-8 recovery entry"))?
            .to_string();
        if !metadata.file_type().is_file()
            || metadata.file_type().is_symlink()
            || !allowed_name(&name)
        {
            return Err(std::io::Error::other("invalid recovery entry"));
        }
        if entries.len() >= max_entries {
            return Err(std::io::Error::other("recovery read bound exceeded"));
        }
        bytes = bytes
            .checked_add(usize::try_from(metadata.len()).unwrap_or(usize::MAX))
            .ok_or_else(|| std::io::Error::other("recovery entry byte overflow"))?;
        if bytes > max_bytes {
            return Err(std::io::Error::other("recovery read bound exceeded"));
        }
        entries.push(BoundedRegularEntry { name, path });
    }
    Ok(entries)
}

fn read_regular_utf8_bounded(
    path: &Path,
    max_bytes: usize,
    context: &str,
) -> Result<String, AppError> {
    let path_metadata = fs::symlink_metadata(path)
        .map_err(|err| AppError::blocked(format!("{context} metadata 실패: {err}")))?;
    if path_metadata.file_type().is_symlink()
        || !path_metadata.is_file()
        || path_metadata.len() > u64::try_from(max_bytes).unwrap_or(u64::MAX)
    {
        return Err(AppError::blocked(format!(
            "{context} regular-file/byte budget 불일치"
        )));
    }
    let mut file = fs::File::open(path)
        .map_err(|err| AppError::blocked(format!("{context} 열기 실패: {err}")))?;
    validate_open_regular_file_identity(path, &file, context)?;
    let mut bytes = Vec::with_capacity(
        usize::try_from(path_metadata.len())
            .unwrap_or(max_bytes)
            .min(max_bytes),
    );
    file.by_ref()
        .take(
            u64::try_from(max_bytes)
                .unwrap_or(u64::MAX)
                .saturating_add(1),
        )
        .read_to_end(&mut bytes)
        .map_err(|err| AppError::blocked(format!("{context} 읽기 실패: {err}")))?;
    if bytes.len() > max_bytes {
        return Err(AppError::blocked(format!(
            "{context} byte budget 초과; 증거를 보존했습니다."
        )));
    }
    validate_open_regular_file_identity(path, &file, context)?;
    String::from_utf8(bytes).map_err(|_| AppError::blocked(format!("{context} UTF-8 불일치")))
}

#[cfg(unix)]
fn validate_open_regular_file_identity(
    path: &Path,
    file: &fs::File,
    context: &str,
) -> Result<(), AppError> {
    use std::os::unix::fs::MetadataExt;

    let path_metadata = fs::symlink_metadata(path)
        .map_err(|err| AppError::blocked(format!("{context} 경로 재검증 실패: {err}")))?;
    let file_metadata = file
        .metadata()
        .map_err(|err| AppError::blocked(format!("{context} handle 검증 실패: {err}")))?;
    if path_metadata.file_type().is_symlink()
        || !path_metadata.is_file()
        || path_metadata.dev() != file_metadata.dev()
        || path_metadata.ino() != file_metadata.ino()
    {
        return Err(AppError::blocked(format!(
            "{context} path/handle identity 불일치; 증거를 보존했습니다."
        )));
    }
    Ok(())
}

#[cfg(windows)]
fn validate_open_regular_file_identity(
    path: &Path,
    file: &fs::File,
    context: &str,
) -> Result<(), AppError> {
    let path_metadata = fs::symlink_metadata(path)
        .map_err(|err| AppError::blocked(format!("{context} 경로 재검증 실패: {err}")))?;
    let same_file = windows_replace::path_refers_to_open_file(path, file)
        .map_err(|err| AppError::blocked(format!("{context} handle 검증 실패: {err}")))?;
    if path_metadata.file_type().is_symlink() || !path_metadata.is_file() || !same_file {
        return Err(AppError::blocked(format!(
            "{context} path/handle identity 불일치; 증거를 보존했습니다."
        )));
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn validate_open_regular_file_identity(
    path: &Path,
    file: &fs::File,
    context: &str,
) -> Result<(), AppError> {
    let path_metadata = fs::symlink_metadata(path)
        .map_err(|err| AppError::blocked(format!("{context} 경로 재검증 실패: {err}")))?;
    let file_metadata = file
        .metadata()
        .map_err(|err| AppError::blocked(format!("{context} handle 검증 실패: {err}")))?;
    if path_metadata.file_type().is_symlink()
        || !path_metadata.is_file()
        || path_metadata.len() != file_metadata.len()
    {
        return Err(AppError::blocked(format!(
            "{context} path/handle identity 불일치; 증거를 보존했습니다."
        )));
    }
    Ok(())
}

fn recovery_work_may_exist() -> bool {
    let lag_directory = paths::projection_lag_dir();
    if directory_has_entry_or_is_suspicious(&lag_directory, |_| true) {
        return true;
    }
    let journal_root = paths::project_state_dir().join("transition-journal");
    let projects = match fs::read_dir(&journal_root) {
        Ok(projects) => projects,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return false,
        Err(_) => return true,
    };
    let mut project_count = 0_usize;
    for project in projects {
        project_count = project_count.saturating_add(1);
        if project_count > MAX_RECOVERY_PROJECT_ENTRIES {
            return true;
        }
        let Ok(project) = project else {
            return true;
        };
        let Ok(metadata) = fs::symlink_metadata(project.path()) else {
            return true;
        };
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return true;
        }
        if directory_has_entry_or_is_suspicious(&project.path(), |name| name != "transition.lock") {
            return true;
        }
    }
    false
}

fn directory_has_entry_or_is_suspicious(
    directory: &Path,
    counts_as_recovery_work: impl Fn(&str) -> bool,
) -> bool {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return false,
        Err(_) => return true,
    };
    for entry in entries {
        let Ok(entry) = entry else {
            return true;
        };
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            return true;
        };
        if counts_as_recovery_work(&name) {
            return true;
        }
    }
    false
}

fn recover_pending_bundles_under_guard(project_id: &str) -> Result<usize, AppError> {
    let directory = paths::project_transition_journal_dir(project_id);
    let lag_directory = paths::projection_lag_dir();
    if !directory.exists() && !lag_directory.exists() {
        return Ok(0);
    }
    validate_projection_lag_authority(project_id, &directory)?;
    if !directory.exists() {
        return Ok(0);
    }
    let mut entries = bounded_regular_entries(
        &directory,
        MAX_RECOVERY_JOURNAL_ENTRIES,
        MAX_RECOVERY_JOURNAL_BYTES,
        |_| true,
    )
    .map_err(|err| {
        AppError::blocked(format!(
            "transition journal recovery bound 검증 실패: {err}"
        ))
    })?;
    entries.sort_by(|left, right| left.name.as_bytes().cmp(right.name.as_bytes()));
    let mut recovered = 0_usize;
    for entry in entries {
        let name = entry.name;
        if name == "transition.lock" {
            continue;
        }
        if let Some(intent_id) = name.strip_suffix(".prepared.json.tmp") {
            validate_ascii_id(intent_id, "intent")?;
            let final_path = paths::project_transition_journal_file(project_id, intent_id);
            let temp_body = read_regular_utf8_bounded(
                &entry.path,
                MAX_PREPARED_BUNDLE_BYTES,
                "transition temp",
            )?;
            let temp_bundle = parse_prepared_source_bundle(&temp_body)?;
            if temp_bundle.intent_id != intent_id || temp_bundle.project_id != project_id {
                return Err(AppError::blocked(
                    "transition temp owner/name binding 불일치",
                ));
            }
            if final_path.exists() {
                let final_body = read_regular_utf8_bounded(
                    &final_path,
                    MAX_PREPARED_BUNDLE_BYTES,
                    "transition final",
                )?;
                if final_body != temp_body {
                    return Err(AppError::blocked("transition final/temp bytes conflict"));
                }
            }
            fs::remove_file(&entry.path).map_err(|err| {
                AppError::runtime(format!("zero-effect transition temp cleanup 실패: {err}"))
            })?;
            sync_parent(&entry.path)?;
            continue;
        }
        let Some(intent_id) = name.strip_suffix(".prepared.json") else {
            return Err(AppError::blocked(format!(
                "unknown transition journal entry 보존: {name}"
            )));
        };
        validate_ascii_id(intent_id, "intent")?;
        let body =
            read_regular_utf8_bounded(&entry.path, MAX_PREPARED_BUNDLE_BYTES, "transition final")?;
        let bundle = parse_prepared_source_bundle(&body)?;
        if bundle.intent_id != intent_id || bundle.project_id != project_id {
            return Err(AppError::blocked(
                "transition final owner/name binding 불일치",
            ));
        }
        match bundle.intent_kind.as_str() {
            "approve-patch" if bundle.additional_members.is_empty() => {
                #[cfg(not(unix))]
                return Err(AppError::blocked(format!(
                    "source install recovery 차단\n- code: source-install.unsupported-platform\n- platform: {}\n- 동작: committed journal을 보존했습니다.",
                    std::env::consts::OS
                )));
                #[cfg(unix)]
                {
                    crate::state::validate_current_state_recovery_cas(
                        bundle.current_revision,
                        &bundle.current_artifact_hash,
                        None,
                    )?;
                    crate::state::install_prepared_source_bundle(&bundle, &entry.path)?;
                }
            }
            "approve-patch" => {
                #[cfg(not(unix))]
                return Err(AppError::blocked(format!(
                    "source install recovery 차단\n- code: source-install.unsupported-platform\n- platform: {}\n- 동작: committed journal을 보존했습니다.",
                    std::env::consts::OS
                )));
                #[cfg(unix)]
                crate::patch::recover_prepared_approval_bundle(&bundle, &entry.path)?;
            }
            "approve-verification" => {
                crate::patch::recover_prepared_verification_bundle(&bundle, &entry.path)?;
            }
            kind if is_terminal_action_intent_kind(kind) => {
                crate::state::recover_project_current_state_prepared_terminal_action(
                    &bundle,
                    &entry.path,
                )?;
            }
            kind if is_state_transition_intent_kind(kind) => {
                crate::state::recover_prepared_state_transition(&bundle)?;
            }
            _ => return Err(AppError::blocked("transition recovery intent kind 불일치")),
        }
        remove_committed_source_bundle(&bundle, &entry.path)?;
        recovered = recovered
            .checked_add(1)
            .ok_or_else(|| AppError::blocked("transition recovery count overflow"))?;
    }
    Ok(recovered)
}

fn validate_projection_lag_authority(
    project_id: &str,
    journal_directory: &Path,
) -> Result<bool, AppError> {
    let lag_directory = paths::projection_lag_dir();
    if !lag_directory.exists() {
        return Ok(false);
    }
    let lag_entries = bounded_regular_entries(
        &lag_directory,
        MAX_PROJECTION_LAG_ENTRIES,
        MAX_PROJECTION_LAG_BYTES,
        |name| name.ends_with(".json") || name.ends_with(".json.tmp"),
    )
    .map_err(|err| AppError::blocked(format!("projection lag recovery bound 검증 실패: {err}")))?;
    if lag_entries.is_empty() {
        return Ok(false);
    }
    let mut bundles = Vec::new();
    if journal_directory.exists() {
        let entries = bounded_regular_entries(
            journal_directory,
            MAX_RECOVERY_JOURNAL_ENTRIES,
            MAX_RECOVERY_JOURNAL_BYTES,
            |name| {
                name == "transition.lock"
                    || name.ends_with(".prepared.json")
                    || name.ends_with(".prepared.json.tmp")
            },
        )
        .map_err(|err| {
            AppError::blocked(format!(
                "projection lag journal recovery bound 검증 실패: {err}"
            ))
        })?;
        for entry in entries {
            let name = entry.name;
            if name == "transition.lock" || !name.ends_with(".prepared.json") {
                continue;
            }
            let body = read_regular_utf8_bounded(
                &entry.path,
                MAX_PREPARED_BUNDLE_BYTES,
                "projection lag journal",
            )?;
            let bundle = parse_prepared_source_bundle(&body)?;
            if bundle.project_id != project_id {
                return Err(AppError::blocked(
                    "projection lag journal project binding 불일치",
                ));
            }
            bundles.push(bundle);
        }
    }
    for entry in lag_entries {
        let name = entry.name;
        let final_name = name.strip_suffix(".tmp").unwrap_or(&name);
        if !final_name.ends_with(".json") {
            return Err(AppError::blocked(
                "unknown projection lag entry를 보존했습니다.",
            ));
        }
        let body =
            read_regular_utf8_bounded(&entry.path, MAX_PROJECTION_LAG_BYTES, "projection lag")?;
        let matches = bundles
            .iter()
            .filter(|bundle| {
                bundle.additional_members.iter().any(|member| {
                    member.kind == PreparedMemberKind::ProjectionLag
                        && member.bytes_utf8 == body
                        && Path::new(&member.path)
                            .file_name()
                            .and_then(|value| value.to_str())
                            == Some(final_name)
                })
            })
            .count();
        if matches != 1 {
            return Err(AppError::blocked(
                "orphan 또는 ambiguous projection lag를 보존했습니다.",
            ));
        }
    }
    Ok(true)
}

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
        let expected_hash = crate::ledger::planned_event_hash(event, &previous);
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
    crate::state::validate_prepared_state_current_member(bundle, current)?;
    if checkpoint {
        let workflow_id = bundle
            .workflow_id
            .as_deref()
            .ok_or_else(|| AppError::blocked("prepared checkpoint workflow id 누락"))?;
        let prepared = crate::state::decode_prepared_workflow_revision(
            workflow_id,
            &bundle.additional_members[0],
            &bundle.additional_members[1],
            event,
        )?;
        let final_chain = bundle
            .event_chain_plan
            .last()
            .ok_or_else(|| AppError::blocked("prepared checkpoint final chain 누락"))?;
        let final_binding = crate::ledger::LedgerBinding {
            event_count: final_chain.ordinal,
            event_id: Some(final_chain.event_id.clone()),
            event_hash: final_chain.event_hash.clone(),
        };
        crate::state::decode_prepared_current_image(
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
            crate::ledger::json_string(path),
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
        crate::ledger::json_string(&member.path),
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
        crate::ledger::json_string(&member.bytes_utf8),
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
    semantic_events: &'a [crate::ledger::LedgerEvent],
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

fn render_semantic_events(events: &[crate::ledger::LedgerEvent]) -> String {
    let rows = events
        .iter()
        .map(render_semantic_event)
        .collect::<Vec<_>>()
        .join(",");
    format!("[{rows}]")
}

fn render_semantic_event(event: &crate::ledger::LedgerEvent) -> String {
    format!(
        "{{\"schema_version\":1,\"event_id\":\"{}\",\"ts_ms\":{},\"event_type\":\"{}\",\"project_id\":\"{}\",\"session_id\":\"{}\",\"summary\":\"{}\",\"details\":\"{}\"}}",
        crate::ledger::json_string(&event.event_id),
        event.ts_ms,
        crate::ledger::json_string(&event.event_type),
        crate::ledger::json_string(&event.project_id),
        crate::ledger::json_string(&event.session_id),
        crate::ledger::json_string(&event.summary),
        crate::ledger::json_string(&event.details),
    )
}

fn parse_semantic_events(
    object: &CanonicalObject,
) -> Result<Vec<crate::ledger::LedgerEvent>, AppError> {
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
            Ok(crate::ledger::LedgerEvent {
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
                crate::ledger::json_string(&entry.event_id),
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
    semantic_events: &[crate::ledger::LedgerEvent],
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
        .map(|value| format!("\"{}\"", crate::ledger::json_string(value)))
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
    crate::state::validate_source_install_initial_admission(&plan)?;
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
        .map(|operation| format!("\"{}\"", crate::ledger::json_string(operation)))
        .collect::<Vec<_>>()
        .join(",");
    let body = format!(
        "{{\"schema_version\":{},\"source_key\":\"{}\",\"target\":{},\"before_blob\":{},\"proposed_blob\":{},\"rollback_final\":{},\"install_temp\":{},\"guard_path\":{},\"before_sha256\":\"{}\",\"before_byte_length\":{},\"proposed_sha256\":\"{}\",\"proposed_byte_length\":{},\"permissions\":{},\"ownership\":{},\"platform\":\"{}\",\"unix_metadata\":{},\"operations\":[{}]}}",
        plan.schema_version,
        crate::ledger::json_string(&plan.source_key),
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
        crate::ledger::json_string(&path.namespace),
        crate::ledger::json_string(&path.path),
        crate::ledger::json_string(&path.parent),
        crate::ledger::json_string(&path.basename),
        crate::ledger::json_string(&path.expected_type),
        path.expected_identity
            .as_ref()
            .map(|value| format!("\"{}\"", crate::ledger::json_string(value)))
            .unwrap_or_else(|| "null".to_string())
    )
}

fn render_blob(blob: &PreparedBlob) -> String {
    format!(
        "{{\"blob_id\":\"{}\",\"member_path\":\"{}\",\"sha256\":\"{}\",\"byte_length\":{}}}",
        crate::ledger::json_string(&blob.blob_id),
        crate::ledger::json_string(&blob.member_path),
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
        crate::ledger::json_string(&value.before_owner),
        crate::ledger::json_string(&value.install_owner)
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
mod tests {
    use super::*;

    #[test]
    fn recovery_rejects_and_preserves_unknown_lock_candidates() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-transition-lock-candidates-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let project_root = root.join("project");
        let data_home = root.join("data");
        fs::create_dir_all(&project_root).unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", &data_home);
        crate::state::initialize().unwrap();
        let project_id = crate::ledger::validated_current_identity()
            .unwrap()
            .project_id;
        let transition_guard = TransitionGuard::acquire(&project_id).unwrap();
        let directory = paths::project_transition_journal_dir(&project_id);
        let malformed = directory.join("transition.candidate.1.2");
        fs::write(&malformed, b"").unwrap();
        let error = recover_pending_bundles_under_guard(&project_id).unwrap_err();
        assert!(error.message.contains("unknown transition journal entry"));
        assert!(malformed.exists());

        drop(transition_guard);
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn recovery_enforces_file_and_directory_read_bounds_before_parsing() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-transition-recovery-bounds-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let project_root = root.join("project");
        let data_home = root.join("data");
        fs::create_dir_all(&project_root).unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", &data_home);
        crate::state::initialize().unwrap();
        let project_id = crate::ledger::validated_current_identity()
            .unwrap()
            .project_id;
        let transition_guard = TransitionGuard::acquire(&project_id).unwrap();
        let directory = paths::project_transition_journal_dir(&project_id);

        for index in 0..MAX_RECOVERY_JOURNAL_ENTRIES {
            fs::write(
                directory.join(format!("intent-bound-{index}.prepared.json")),
                b"{}",
            )
            .unwrap();
        }
        let entry_error = recover_pending_bundles_under_guard(&project_id).unwrap_err();
        assert!(entry_error
            .message
            .contains("transition journal recovery bound"));

        for index in 0..MAX_RECOVERY_JOURNAL_ENTRIES {
            fs::remove_file(directory.join(format!("intent-bound-{index}.prepared.json"))).unwrap();
        }
        let oversized = directory.join("intent-oversized.prepared.json");
        fs::write(&oversized, vec![b'x'; MAX_PREPARED_BUNDLE_BYTES + 1]).unwrap();
        let byte_error = recover_pending_bundles_under_guard(&project_id).unwrap_err();
        assert!(byte_error.message.contains("regular-file/byte budget"));

        fs::remove_file(oversized).unwrap();
        let lag_directory = paths::projection_lag_dir();
        fs::create_dir_all(&lag_directory).unwrap();
        let oversized_lag = lag_directory.join("oversized.json");
        fs::write(&oversized_lag, vec![b'x'; MAX_PROJECTION_LAG_BYTES + 1]).unwrap();
        let lag_error = recover_pending_bundles_under_guard(&project_id).unwrap_err();
        assert!(lag_error.message.contains("projection lag recovery bound"));

        assert!(oversized_lag.exists());
        drop(transition_guard);
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn recovery_discovery_treats_oversized_project_root_as_suspicious() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-transition-project-discovery-bound-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let project_root = root.join("project");
        let data_home = root.join("data");
        fs::create_dir_all(&project_root).unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", &data_home);
        crate::state::initialize().unwrap();
        let journal_root = paths::project_state_dir().join("transition-journal");
        for index in 0..=MAX_RECOVERY_PROJECT_ENTRIES {
            fs::create_dir_all(journal_root.join(format!("empty-project-{index}"))).unwrap();
        }

        assert!(recovery_work_may_exist());

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn bounded_recovery_file_read_rejects_oversized_bytes() {
        let path = std::env::temp_dir().join(format!(
            "rpotato-transition-bounded-read-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, vec![b'x'; 65]).unwrap();

        let error = read_regular_utf8_bounded(&path, 64, "bounded fixture").unwrap_err();

        assert!(error.message.contains("regular-file/byte budget"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn projection_lag_member_full_bytes_golden_is_independent() {
        let planned = (0_u64..10)
            .map(|index| crate::ledger::PlannedEvent {
                event: crate::ledger::LedgerEvent {
                    event_id: format!("event-{index}"),
                    ts_ms: u128::from(index),
                    event_type: "approval.event".to_string(),
                    project_id: "project-golden".to_string(),
                    session_id: "session-golden".to_string(),
                    summary: "golden".to_string(),
                    details: format!("index={index}"),
                },
                ordinal: index + 1,
                previous_event_hash: "0".repeat(64),
                event_hash: if index == 9 {
                    "a".repeat(64)
                } else {
                    "0".repeat(64)
                },
            })
            .collect::<Vec<_>>();

        let member = prepare_projection_lag_member("intent-golden", &planned).unwrap();

        assert_eq!(
            member.bytes_utf8.as_bytes(),
            b"{\"schema_version\":1,\"intent_id\":\"intent-golden\",\"event_id\":\"event-9\",\"event_ordinal\":10,\"event_hash\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"required_outputs\":[\"project-session-ledger\",\"global-operation-log\",\"sqlite\"],\"required_event_ids\":[\"event-0\",\"event-1\",\"event-2\",\"event-3\",\"event-4\",\"event-5\",\"event-6\",\"event-7\",\"event-8\",\"event-9\"]}"
        );
        assert_eq!(member.binding.event_id.as_deref(), Some("event-9"));
        assert_eq!(
            member.path,
            "state/projection-lag/intent-golden-event-9.json"
        );
    }

    #[test]
    fn transition_component_byte_caps_accept_limit_and_reject_limit_plus_one() {
        for (label, limit) in [
            ("before-blob", MAX_SOURCE_BLOB_BYTES),
            ("proposed-blob", MAX_SOURCE_BLOB_BYTES),
            ("tool-output", 262_144),
            ("transcript-v2", 131_072),
            ("workflow-snapshot", 65_536),
            ("workflow-pointer", 16_384),
            ("current-image", 65_536),
            ("semantic-event", MAX_PREPARED_EVENT_BYTES),
            ("semantic-events", MAX_PREPARED_EVENTS_BYTES),
            ("projection-lag", 4_096),
            ("source-install-v1", MAX_SOURCE_INSTALL_BYTES),
            ("full-journal", MAX_PREPARED_BUNDLE_BYTES),
        ] {
            assert!(
                enforce_byte_limit(limit - 1, limit, "limit exceeded").is_ok(),
                "{label} limit-1"
            );
            assert!(
                enforce_byte_limit(limit, limit, "limit exceeded").is_ok(),
                "{label} limit"
            );
            assert!(
                enforce_byte_limit(limit + 1, limit, "limit exceeded").is_err(),
                "{label} limit+1"
            );
        }
        assert!(checked_add_bytes(
            usize::MAX,
            1,
            MAX_PREPARED_BUNDLE_BYTES,
            "overflow",
            "limit exceeded",
        )
        .unwrap_err()
        .message
        .contains("overflow"));
        let multibyte = "가".repeat((MAX_PREPARED_EVENT_BYTES / 3) + 1);
        assert!(multibyte.chars().count() < MAX_PREPARED_EVENT_BYTES);
        assert!(
            enforce_byte_limit(multibyte.len(), MAX_PREPARED_EVENT_BYTES, "limit exceeded")
                .is_err()
        );
    }

    #[test]
    fn source_identity_v1_matches_normative_golden() {
        let hash = "473b0fef5f0626d3fe806f10b931f085d511ba15b1117c53d5f2ec27d5b9452e";
        assert_eq!(sha256_bytes(b"current source\n"), hash);
        assert_eq!(
            source_identity_v1(0x0102_0304_0506_0708, 0x1112_1314_1516_1718, hash).unwrap(),
            "2b3452be6ffa18621fcd39e56162e5b46ef9428657dd6cdc9e02847e521420d0"
        );
        assert!(source_identity_v1(
            0x0102_0304_0506_0708,
            0x1112_1314_1516_1718,
            &hash.to_ascii_uppercase()
        )
        .is_err());
    }

    #[cfg(unix)]
    #[test]
    fn source_install_v1_round_trips_exact_order_and_bindings() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-source-install-v1-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        let target = root.join("src/lib.rs");
        fs::write(&target, b"current source\n").unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        crate::state::initialize().unwrap();
        let plan = prepare_source_install_v1(
            "intent-source-fixture",
            "proposal-fixture",
            &target,
            b"current source\n",
            b"proposed source\n",
        )
        .unwrap();
        let body = render_source_install_v1(&plan).unwrap();
        assert_eq!(parse_source_install_v1(&body).unwrap(), plan);
        assert_eq!(plan.operations.len(), 19);
        assert_eq!(plan.target.path, "src/lib.rs");
        assert!(plan
            .rollback_final
            .path
            .starts_with(".rpotato/patches/proposal-fixture/intent-source-fixture-"));
        assert!(!body.ends_with('\n'));
        assert!(body.starts_with("{\"schema_version\":1,\"source_key\":"));

        let reordered = body.replacen("\"schema_version\":1,\"source_key\":", "\"source_key\":", 1);
        assert!(parse_source_install_v1(&reordered).is_err());

        let bundle = prepare_source_bundle(
            "intent-source-fixture",
            None,
            plan,
            b"current source\n",
            b"proposed source\n",
        )
        .unwrap();
        let bundle_body = render_prepared_source_bundle(&bundle).unwrap();
        assert_eq!(parse_prepared_source_bundle(&bundle_body).unwrap(), bundle);
        assert_eq!(bundle_body.matches("\"member_kind\"").count(), 3);
        let journal = commit_prepared_source_bundle(&bundle).unwrap();
        assert_eq!(commit_prepared_source_bundle(&bundle).unwrap(), journal);
        assert!(
            !paths::project_transition_journal_temp(&bundle.project_id, &bundle.intent_id).exists()
        );
        remove_committed_source_bundle(&bundle, &journal).unwrap();
        assert!(!journal.exists());
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn source_install_initial_admission_rejects_preexisting_exact_rollback() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-source-rollback-admission-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        let target = root.join("src/lib.rs");
        fs::write(&target, b"current source\n").unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        crate::state::initialize().unwrap();
        let plan = prepare_source_install_v1(
            "intent-rollback-admission",
            "proposal-rollback-admission",
            &target,
            b"current source\n",
            b"proposed source\n",
        )
        .unwrap();
        let rollback = root.join(&plan.rollback_final.path);
        fs::create_dir_all(rollback.parent().unwrap()).unwrap();
        fs::write(&rollback, b"current source\n").unwrap();

        let error = prepare_source_install_v1(
            "intent-rollback-admission",
            "proposal-rollback-admission",
            &target,
            b"current source\n",
            b"proposed source\n",
        )
        .unwrap_err();

        assert!(error
            .message
            .contains("rollback path가 journal commit 전에 이미 존재"));
        assert!(!paths::project_transition_journal_file(
            &crate::ledger::fresh_identity().project_id,
            "intent-rollback-admission"
        )
        .exists());
        assert_eq!(fs::read(&target).unwrap(), b"current source\n");
        assert_eq!(fs::read(&rollback).unwrap(), b"current source\n");

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn source_install_v1_rejects_metadata_changes_in_prepared_bytes() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-source-install-metadata-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        let target = root.join("src/lib.rs");
        fs::write(&target, b"current source\n").unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        crate::state::initialize().unwrap();
        let plan = prepare_source_install_v1(
            "intent-source-metadata",
            "proposal-metadata",
            &target,
            b"current source\n",
            b"proposed source\n",
        )
        .unwrap();

        let mut readonly = plan.clone();
        readonly.permissions.install_readonly = !readonly.permissions.before_readonly;
        assert!(validate_source_install_v1(&readonly).is_err());

        let mut mode = plan.clone();
        mode.permissions.install_mode ^= 0o100;
        mode.unix_metadata.install_mode = mode.permissions.install_mode;
        assert!(validate_source_install_v1(&mode).is_err());

        let mut owner = plan;
        owner.unix_metadata.install_uid = owner.unix_metadata.install_uid.wrapping_add(1);
        owner.ownership.install_owner = format!(
            "uid:{}:gid:{}",
            owner.unix_metadata.install_uid, owner.unix_metadata.install_gid
        );
        assert!(validate_source_install_v1(&owner).is_err());

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn aggregate_bundle_limit_rejects_before_journal_commit() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-prepared-aggregate-cap-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        let target = root.join("src/lib.rs");
        let before = vec![b'"'; MAX_SOURCE_BLOB_BYTES];
        let proposed = vec![b'\\'; MAX_SOURCE_BLOB_BYTES];
        fs::write(&target, &before).unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        crate::state::initialize().unwrap();
        let plan = prepare_source_install_v1(
            "intent-aggregate-cap",
            "proposal-aggregate-cap",
            &target,
            &before,
            &proposed,
        )
        .unwrap();
        let bundle =
            prepare_source_bundle("intent-aggregate-cap", None, plan, &before, &proposed).unwrap();
        let journal = paths::project_transition_journal_file(&bundle.project_id, &bundle.intent_id);

        let error = commit_prepared_source_bundle(&bundle).unwrap_err();

        assert!(error.message.contains("prepared bundle byte limit"));
        assert!(!journal.exists());
        assert!(
            !paths::project_transition_journal_temp(&bundle.project_id, &bundle.intent_id,)
                .exists()
        );
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn prepared_bundle_strictly_binds_semantic_event_chain_plan() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-prepared-event-chain-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        let target = root.join("src/lib.rs");
        fs::write(&target, b"current source\n").unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        crate::state::initialize().unwrap();

        let source = prepare_source_install_v1(
            "intent-event-chain",
            "proposal-event-chain",
            &target,
            b"current source\n",
            b"proposed source\n",
        )
        .unwrap();
        let mut bundle = prepare_source_bundle(
            "intent-event-chain",
            Some("workflow-event-chain"),
            source,
            b"current source\n",
            b"proposed source\n",
        )
        .unwrap();
        let identity = crate::ledger::validated_current_identity().unwrap();
        let events = [
            crate::ledger::new_event_for(
                &identity,
                "approval.prepared",
                "승인 준비",
                "intent_id=intent-event-chain workflow_id=workflow-event-chain",
            ),
            crate::ledger::new_event_for(
                &identity,
                "source.installed",
                "소스 설치",
                "intent_id=intent-event-chain workflow_id=workflow-event-chain",
            ),
        ];
        let writer = crate::ledger::LedgerWriterGuard::acquire().unwrap();
        let planned = writer.plan_events(&events).unwrap();
        bind_planned_events(&mut bundle, &planned).unwrap();

        let body = render_prepared_source_bundle(&bundle).unwrap();
        assert_eq!(parse_prepared_source_bundle(&body).unwrap(), bundle);
        assert_eq!(bundle.semantic_events, events);
        assert_eq!(bundle.event_chain_plan.len(), 2);
        assert_eq!(
            bundle.event_chain_plan[0].ordinal,
            bundle.ledger_binding.event_count + 1
        );
        assert_eq!(
            bundle.event_chain_plan[1].previous_event_hash,
            bundle.event_chain_plan[0].event_hash
        );

        let wrong_ordinal = body.replacen(
            &format!("\"ordinal\":{}", bundle.event_chain_plan[0].ordinal),
            &format!("\"ordinal\":{}", bundle.event_chain_plan[0].ordinal + 1),
            1,
        );
        assert!(parse_prepared_source_bundle(&wrong_ordinal).is_err());
        let wrong_hash = body.replacen(&bundle.event_chain_plan[1].event_hash, &"f".repeat(64), 1);
        assert!(parse_prepared_source_bundle(&wrong_hash).is_err());

        drop(writer);
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn prepared_production_member_array_has_exact_eleven_order_and_lag_index() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-prepared-exact-eleven-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        let target = root.join("src/lib.rs");
        fs::write(&target, b"current source\n").unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        crate::state::initialize().unwrap();
        let source = prepare_source_install_v1(
            "intent-exact-eleven",
            "proposal-exact-eleven",
            &target,
            b"current source\n",
            b"proposed source\n",
        )
        .unwrap();
        let mut bundle = prepare_source_bundle(
            "intent-exact-eleven",
            Some("workflow-exact-eleven"),
            source,
            b"current source\n",
            b"proposed source\n",
        )
        .unwrap();
        let identity = crate::ledger::validated_current_identity().unwrap();
        let events = (0..10)
            .map(|index| {
                crate::ledger::new_event_for(
                    &identity,
                    &format!("approval.event.{index}"),
                    &format!("approval event {index}"),
                    &format!("intent_id=intent-exact-eleven index={index}"),
                )
            })
            .collect::<Vec<_>>();
        let writer = crate::ledger::LedgerWriterGuard::acquire().unwrap();
        let planned = writer.plan_events(&events).unwrap();
        bind_planned_events(&mut bundle, &planned).unwrap();
        let member = |kind,
                      path: &str,
                      schema_version,
                      artifact_id: &str,
                      causal_id: Option<&str>,
                      event_id: Option<&str>,
                      role| PreparedMember {
            kind,
            path: path.to_string(),
            schema_version,
            binding: PreparedMemberBinding {
                artifact_id: Some(artifact_id.to_string()),
                causal_id: causal_id.map(str::to_string),
                source_key: None,
                event_id: event_id.map(str::to_string),
            },
            bytes_utf8: format!("{{\"artifact\":\"{artifact_id}\"}}"),
            expected_type: "absent".to_string(),
            expected_identity: None,
            readonly: false,
            mode: 0o600,
            ownership: None,
            semantic_role_rank: role,
        };
        let e1 = events[1].event_id.as_str();
        let e9 = events[9].event_id.as_str();
        let lag = prepare_projection_lag_member("intent-exact-eleven", &planned).unwrap();
        let members = vec![
            lag,
            member(
                PreparedMemberKind::WorkflowPointer,
                ".rpotato/workflows/workflow-exact-eleven.json",
                4,
                "pointer-r2",
                Some("snapshot-r2"),
                Some(e9),
                1,
            ),
            member(
                PreparedMemberKind::ToolOutput,
                "state/tool-output/project/session/workflow/tool.json",
                1,
                "tool-exact-eleven",
                None,
                Some(events[7].event_id.as_str()),
                0,
            ),
            member(
                PreparedMemberKind::CurrentImage,
                "state/current-state.json",
                2,
                "current-exact-eleven",
                Some("snapshot-r2"),
                Some(e9),
                0,
            ),
            member(
                PreparedMemberKind::WorkflowSnapshot,
                ".rpotato/workflows/workflow-exact-eleven.snapshots/00000000000000000002.json",
                4,
                "snapshot-r1",
                None,
                Some(e1),
                0,
            ),
            member(
                PreparedMemberKind::TranscriptV2,
                "state/transcripts/project/session/transcript.json",
                2,
                "transcript-exact-eleven",
                Some("tool-exact-eleven"),
                Some(events[8].event_id.as_str()),
                0,
            ),
            member(
                PreparedMemberKind::WorkflowPointer,
                ".rpotato/workflows/workflow-exact-eleven.json",
                4,
                "pointer-r1",
                Some("snapshot-r1"),
                Some(e1),
                0,
            ),
            member(
                PreparedMemberKind::WorkflowSnapshot,
                ".rpotato/workflows/workflow-exact-eleven.snapshots/00000000000000000003.json",
                4,
                "snapshot-r2",
                None,
                Some(e9),
                1,
            ),
        ];
        bind_additional_members(&mut bundle, members).unwrap();

        let body = render_prepared_source_bundle(&bundle).unwrap();
        assert_eq!(parse_prepared_source_bundle(&body).unwrap(), bundle);
        assert_eq!(bundle.additional_members.len() + 3, 11);
        assert_eq!(bundle.projection_lag_member_index, Some(10));
        assert_eq!(body.matches("\"member_kind\"").count(), 12);
        assert!(body.ends_with(
            "\"projection_lag_v1\":{\"member_kind\":\"projection_lag\",\"member_index\":10}}"
        ));

        let wrong_index = body.replacen("\"member_index\":10", "\"member_index\":9", 1);
        assert!(parse_prepared_source_bundle(&wrong_index).is_err());
        let wrong_shared_path = body.replacen(
            ".rpotato/workflows/workflow-exact-eleven.json",
            ".rpotato/workflows/other.json",
            1,
        );
        assert!(parse_prepared_source_bundle(&wrong_shared_path).is_err());

        drop(writer);
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);
    }
}
