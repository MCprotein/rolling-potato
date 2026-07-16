//! Legal transition intents and prepared transaction domain records.

use crate::foundation::error::AppError;
use crate::runtime_core::workflow::domain::snapshot::CurrentStateLeaseView;
use crate::runtime_core::workflow::storage_compat::ledger::{
    LedgerBinding, LedgerEvent, RuntimeIdentity,
};

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
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::ToolOutput => "tool_output",
            Self::TranscriptV2 => "transcript_v2",
            Self::WorkflowSnapshot => "workflow_snapshot",
            Self::WorkflowPointer => "workflow_pointer",
            Self::CurrentImage => "current_image",
            Self::ProjectionLag => "projection_lag",
        }
    }

    pub(crate) fn rank(self) -> u8 {
        match self {
            Self::ToolOutput => 3,
            Self::TranscriptV2 => 4,
            Self::WorkflowSnapshot => 5,
            Self::WorkflowPointer => 6,
            Self::CurrentImage => 7,
            Self::ProjectionLag => 8,
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, AppError> {
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
    pub ledger_binding: LedgerBinding,
    pub source_install: Option<SourceInstallV1>,
    pub before_bytes: Option<String>,
    pub proposed_bytes: Option<String>,
    pub additional_members: Vec<PreparedMember>,
    pub semantic_events: Vec<LedgerEvent>,
    pub event_chain_plan: Vec<PreparedEventChain>,
    pub projection_lag_member_index: Option<u64>,
}

pub(crate) struct PreparedBundleContext<'a> {
    pub identity: &'a RuntimeIdentity,
    pub lease: &'a CurrentStateLeaseView,
    pub ledger_binding: LedgerBinding,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedEventChain {
    pub event_id: String,
    pub ordinal: u64,
    pub previous_event_hash: String,
    pub event_hash: String,
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

pub(crate) fn is_state_transition_intent_kind(value: &str) -> bool {
    STATE_TRANSITION_INTENT_KINDS.contains(&value)
}

pub(crate) fn is_terminal_action_intent_kind(value: &str) -> bool {
    TERMINAL_ACTION_INTENT_KINDS.contains(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legal_intents_have_stable_names_and_closed_admission_sets() {
        assert_eq!(
            CurrentStateIntent::CheckpointWorkflow.as_str(),
            "checkpoint-workflow"
        );
        assert!(is_state_transition_intent_kind("select-session"));
        assert!(!is_state_transition_intent_kind("approve-patch"));
        assert!(is_terminal_action_intent_kind("deny-verification"));
        assert!(!is_terminal_action_intent_kind("resume"));
    }

    #[test]
    fn prepared_member_kind_round_trips_with_stable_rank() {
        for (kind, name, rank) in [
            (PreparedMemberKind::ToolOutput, "tool_output", 3),
            (PreparedMemberKind::TranscriptV2, "transcript_v2", 4),
            (PreparedMemberKind::WorkflowSnapshot, "workflow_snapshot", 5),
            (PreparedMemberKind::WorkflowPointer, "workflow_pointer", 6),
            (PreparedMemberKind::CurrentImage, "current_image", 7),
            (PreparedMemberKind::ProjectionLag, "projection_lag", 8),
        ] {
            assert_eq!(kind.as_str(), name);
            assert_eq!(kind.rank(), rank);
            assert_eq!(PreparedMemberKind::parse(name).unwrap(), kind);
        }
        assert!(PreparedMemberKind::parse("unknown").is_err());
    }
}
