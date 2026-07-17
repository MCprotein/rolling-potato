use std::cell::Cell;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(windows)]
use crate::adapters::filesystem::windows_replace;
use crate::adapters::filesystem::{layout as paths, lease};
use crate::app::observability_adapter::{self as observability, SessionHistoryEntry, StoreStatus};
use crate::app::workflow_adapter::ledger::{self, RuntimeIdentity};
use crate::app::workflow_adapter::transcript;
use crate::app::workflow_adapter::transition;
use crate::foundation::error::AppError;
use crate::foundation::serialization as strict_json;
use crate::runtime_core::workflow::application::projection_barrier::{
    self as projection_barrier, ProjectionBarrierRecoveryPort,
};
use crate::runtime_core::workflow::application::recovery::{
    self as workflow_recovery, PendingWorkflowTransaction, PreparedStateRecoveryPort,
    RecoveryArtifact, WorkflowRecoveryPort,
};
use crate::runtime_core::workflow::application::transaction_coordinator::{
    self as transaction_coordinator, ApprovalFault, ApprovalRevision, ApprovalTransactionPort,
    ReconcileTransactionPort, StateTransitionFault, StateTransitionTransactionPort,
    TerminalActionFault, TerminalActionTransactionPort, TransactionExecution, VerificationFault,
    VerificationTransactionPort,
};
use crate::runtime_core::workflow::domain::snapshot::{
    self as snapshot_domain, CurrentStateLeaseView, CurrentStateSnapshot, CurrentWorkflowBinding,
    TuiStateSnapshot,
};
use crate::runtime_core::workflow::storage_compat::record::WorkflowPointer;
pub use crate::runtime_core::workflow::storage_compat::record::WorkflowRecord;
use crate::runtime_core::workflow::storage_compat::record::{
    payload as workflow_payload, render as render_workflow,
};
#[cfg(test)]
use crate::runtime_core::workflow::storage_compat::record::{
    payload_v2 as workflow_payload_v2, payload_v3 as workflow_payload_v3,
    render_v2 as render_workflow_v2, render_v3 as render_workflow_v3,
};
use crate::surfaces::tui::runtime_bridge::{
    lease_matches_active_workflow, lease_matches_terminal_selection, new_tui_intent_id,
    ObservedWorkflow, SelectionLease, SelectionObservation,
};
use sha2::{Digest, Sha256};

const WORKFLOW_SCHEMA_VERSION: u64 = 4;
#[cfg(test)]
const PREVIOUS_WORKFLOW_SCHEMA_VERSION: u64 = 3;
#[cfg(test)]
const LEGACY_WORKFLOW_SCHEMA_VERSION: u64 = 2;
const MAX_WORKFLOW_POINTER_BYTES: u64 = 64 * 1024;
const MAX_WORKFLOW_SNAPSHOT_BYTES: u64 = 512 * 1024;
const MAX_PREPARED_SOURCE_BUNDLE_BYTES: u64 = 1024 * 1024;
const CURRENT_STATE_V1_KEYS: &[&str] = &[
    "schema_version",
    "project_id",
    "project_root",
    "session_id",
    "active_workflow",
    "parent_session_id",
    "branch_from_event_id",
    "compaction_boundary",
    "resume_source",
    "terminal_states",
];
const CURRENT_STATE_V2_KEYS: &[&str] = &[
    "schema_version",
    "revision",
    "previous_artifact_hash",
    "project_id",
    "project_root",
    "session_id",
    "active_workflow",
    "parent_session_id",
    "branch_from_event_id",
    "compaction_boundary",
    "resume_source",
    "terminal_states",
    "ledger_binding",
    "artifact_hash",
];

thread_local! {
    static SUPPRESS_VALIDATION_GAP_WRITES: Cell<bool> = const { Cell::new(false) };
}

pub(crate) struct WorkflowCheckpointGuard {
    workflow_id: String,
    _lease: lease::RecoverableLease,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedWorkflowRevision {
    pub record: WorkflowRecord,
    pub snapshot_path: PathBuf,
    pub snapshot_stored_path: String,
    pub snapshot_bytes: String,
    pub snapshot_member_id: String,
    pub pointer_path: PathBuf,
    pub pointer_stored_path: String,
    pub pointer_bytes: String,
    pub pointer_member_id: String,
    pub event: ledger::LedgerEvent,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedCurrentImage {
    pub path: PathBuf,
    pub stored_path: String,
    pub bytes: String,
    pub artifact_id: String,
    pub revision: u64,
}

pub(crate) struct PreparedTerminalSource {
    pub plan: transition::SourceInstallV1,
    pub before: Vec<u8>,
    pub proposed: Vec<u8>,
}

pub fn create_workflow(request: &str) -> Result<WorkflowRecord, AppError> {
    ensure_layout()?;
    let identity = ledger::validated_current_identity()?;
    let record = WorkflowRecord::new(&identity, request);
    checkpoint_workflow(record, 0)
}

pub fn checkpoint_workflow(
    next: WorkflowRecord,
    expected_revision: u64,
) -> Result<WorkflowRecord, AppError> {
    validate_workflow_id(&next.workflow_id)?;
    let transition_guard = transition::TransitionGuard::acquire_for(
        &next.project_id,
        transition::CurrentStateIntent::CheckpointWorkflow,
    )?;
    checkpoint_workflow_under_transition(&transition_guard, next, expected_revision)
}

pub(crate) fn checkpoint_workflow_under_transition(
    transition_guard: &transition::TransitionGuard,
    next: WorkflowRecord,
    expected_revision: u64,
) -> Result<WorkflowRecord, AppError> {
    validate_workflow_id(&next.workflow_id)?;
    let workflow_guard = WorkflowCheckpointGuard::acquire(&next.workflow_id)?;
    let prepared = if expected_revision == 0 {
        workflow_guard.prepare_initial(next)?
    } else {
        let current = workflow_guard.load_current()?;
        if current.revision != expected_revision || current.artifact_hash != next.artifact_hash {
            return Err(AppError::blocked(format!(
                "workflow 저장 차단\n- 이유: revision/hash conflict\n- workflow id: {}\n- expected revision: {}\n- current revision: {}",
                next.workflow_id, expected_revision, current.revision
            )));
        }
        workflow_guard.prepare_revision(&current, next)?
    };
    let identity = workflow_identity(&prepared.record);
    let previous = read_valid_current_for_transition()?;
    if previous
        .as_ref()
        .is_some_and(|current| current.project_id != identity.project_id)
    {
        return Err(AppError::blocked(
            "workflow checkpoint current-state owner binding 불일치",
        ));
    }
    let intent_id = internal_transition_intent_id(&prepared.event);
    transition_project_current_state_under_guard(
        transition_guard,
        StateTransitionRequest {
            intent_id: &intent_id,
            intent: transition::CurrentStateIntent::CheckpointWorkflow,
            identity: &identity,
            event: &prepared.event,
            resume_source: None,
            active_workflow: Some(&prepared.record),
            previous: previous.as_ref(),
            workflow: Some((&workflow_guard, &prepared)),
        },
    )?;
    Ok(prepared.record)
}

impl WorkflowCheckpointGuard {
    pub(crate) fn acquire(workflow_id: &str) -> Result<Self, AppError> {
        validate_workflow_id(workflow_id)?;
        let lease = lease::RecoverableLease::acquire(
            paths::project_workflows_dir().join(format!("{workflow_id}.checkpoint.lock")),
            "workflow checkpoint",
        )?;
        recover_workflow_transaction(workflow_id)?;
        Ok(Self {
            workflow_id: workflow_id.to_string(),
            _lease: lease,
        })
    }

    pub(crate) fn load_current(&self) -> Result<WorkflowRecord, AppError> {
        load_workflow_under_transition(&self.workflow_id)
    }

    fn prepare_initial(
        &self,
        mut next: WorkflowRecord,
    ) -> Result<PreparedWorkflowRevision, AppError> {
        if next.workflow_id != self.workflow_id
            || next.revision != 0
            || paths::project_workflow_file(&self.workflow_id).exists()
        {
            return Err(AppError::blocked(
                "prepared initial workflow predecessor binding 불일치",
            ));
        }
        next.revision = 1;
        next.previous_hash = "none".to_string();
        next.artifact_hash = sha256_text(&workflow_payload(&next));
        build_prepared_workflow_revision(next)
    }

    pub(crate) fn load_recovery_current(
        &self,
        allowed_bindings: &[(u64, &str)],
    ) -> Result<WorkflowRecord, AppError> {
        let pointer_path = paths::project_workflow_file(&self.workflow_id);
        let pointer_bytes = read_regular_file_bounded(
            &pointer_path,
            MAX_WORKFLOW_POINTER_BYTES,
            "prepared recovery workflow pointer",
        )?;
        let pointer = parse_workflow_pointer(&pointer_path, &pointer_bytes)?;
        if pointer.workflow_id != self.workflow_id
            || !allowed_bindings.iter().any(|(revision, hash)| {
                *revision == pointer.committed_revision && *hash == pointer.artifact_hash
            })
        {
            return Err(AppError::blocked(
                "prepared recovery workflow pointer binding 불일치",
            ));
        }
        let snapshot_path =
            paths::project_workflow_snapshot_file(&self.workflow_id, pointer.committed_revision);
        let snapshot_bytes = read_regular_file_bounded(
            &snapshot_path,
            MAX_WORKFLOW_SNAPSHOT_BYTES,
            "prepared recovery workflow snapshot",
        )?;
        let record = parse_workflow_snapshot(&snapshot_path, &snapshot_bytes)?;
        if record.workflow_id != self.workflow_id
            || record.revision != pointer.committed_revision
            || record.artifact_hash != pointer.artifact_hash
            || render_workflow(&record) != snapshot_bytes
            || render_workflow_pointer_bytes(&record, pointer.schema_version)? != pointer_bytes
        {
            return Err(AppError::blocked(
                "prepared recovery workflow snapshot/pointer canonical binding 불일치",
            ));
        }
        Ok(record)
    }

    pub(crate) fn prepare_revision(
        &self,
        current: &WorkflowRecord,
        mut next: WorkflowRecord,
    ) -> Result<PreparedWorkflowRevision, AppError> {
        if current.workflow_id != self.workflow_id
            || next.workflow_id != self.workflow_id
            || next.revision != current.revision
            || next.artifact_hash != current.artifact_hash
        {
            return Err(AppError::blocked(
                "prepared workflow revision CAS binding 불일치",
            ));
        }
        next.previous_hash = current.artifact_hash.clone();
        next.revision = current
            .revision
            .checked_add(1)
            .ok_or_else(|| AppError::blocked("workflow revision overflow"))?;
        next.artifact_hash = sha256_text(&workflow_payload(&next));
        build_prepared_workflow_revision(next)
    }

    fn install_snapshot(&self, prepared: &PreparedWorkflowRevision) -> Result<(), AppError> {
        if prepared.record.workflow_id != self.workflow_id
            || prepared.snapshot_path
                != paths::project_workflow_snapshot_file(
                    &self.workflow_id,
                    prepared.record.revision,
                )
        {
            return Err(AppError::blocked(
                "prepared workflow snapshot guard binding 불일치",
            ));
        }
        write_workflow_snapshot_bytes(&prepared.record, prepared.snapshot_bytes.as_bytes())
    }

    fn install_pointer(&self, prepared: &PreparedWorkflowRevision) -> Result<(), AppError> {
        if prepared.record.workflow_id != self.workflow_id
            || prepared.pointer_path != paths::project_workflow_file(&self.workflow_id)
        {
            return Err(AppError::blocked(
                "prepared workflow pointer guard binding 불일치",
            ));
        }
        if prepared.pointer_path.exists() {
            let existing_bytes = read_regular_file_bounded(
                &prepared.pointer_path,
                MAX_WORKFLOW_POINTER_BYTES,
                "prepared workflow pointer reread",
            )?;
            let existing = parse_workflow_pointer(&prepared.pointer_path, &existing_bytes)?;
            if existing.workflow_id != self.workflow_id {
                return Err(AppError::blocked(
                    "prepared workflow pointer workflow binding 불일치",
                ));
            }
            if existing.committed_revision == prepared.record.revision {
                if existing_bytes == prepared.pointer_bytes
                    && existing.artifact_hash == prepared.record.artifact_hash
                {
                    return Ok(());
                }
                return Err(AppError::blocked(
                    "prepared workflow pointer immutable revision conflict",
                ));
            }
            if existing.committed_revision > prepared.record.revision {
                if existing.committed_revision
                    != prepared.record.revision.checked_add(1).unwrap_or(0)
                {
                    return Err(AppError::blocked(
                        "prepared workflow pointer newer revision 범위 불일치",
                    ));
                }
                let newer_path = paths::project_workflow_snapshot_file(
                    &self.workflow_id,
                    existing.committed_revision,
                );
                let newer_bytes = read_regular_file_bounded(
                    &newer_path,
                    MAX_WORKFLOW_SNAPSHOT_BYTES,
                    "prepared workflow pointer newer snapshot",
                )?;
                let newer = parse_workflow_snapshot(&newer_path, &newer_bytes)?;
                if newer.workflow_id != self.workflow_id
                    || newer.revision != existing.committed_revision
                    || newer.previous_hash != prepared.record.artifact_hash
                    || newer.artifact_hash != existing.artifact_hash
                    || render_workflow(&newer) != newer_bytes
                {
                    return Err(AppError::blocked(
                        "prepared workflow pointer newer chain binding 불일치",
                    ));
                }
                return Ok(());
            }
        }
        atomic_replace_bytes(&prepared.pointer_path, prepared.pointer_bytes.as_bytes())
    }
}

fn build_prepared_workflow_revision(
    next: WorkflowRecord,
) -> Result<PreparedWorkflowRevision, AppError> {
    let snapshot_bytes = render_workflow(&next);
    let pointer_bytes = render_workflow_pointer_bytes(&next, WORKFLOW_SCHEMA_VERSION)?;
    let identity = workflow_identity(&next);
    let event = workflow_checkpoint_event(&next, &identity);
    Ok(PreparedWorkflowRevision {
        snapshot_path: paths::project_workflow_snapshot_file(&next.workflow_id, next.revision),
        snapshot_stored_path: format!(
            ".rpotato/workflows/{}.snapshots/{:020}.json",
            next.workflow_id, next.revision
        ),
        snapshot_member_id: prepared_workflow_member_id(
            "rpotato.workflow-snapshot-member-id/v1",
            "workflow-snapshot",
            &next.workflow_id,
            next.revision,
        ),
        pointer_path: paths::project_workflow_file(&next.workflow_id),
        pointer_stored_path: format!(".rpotato/workflows/{}.json", next.workflow_id),
        pointer_member_id: prepared_workflow_member_id(
            "rpotato.workflow-pointer-member-id/v1",
            "workflow-pointer",
            &next.workflow_id,
            next.revision,
        ),
        pointer_bytes,
        snapshot_bytes,
        event,
        record: next,
    })
}

pub(crate) fn decode_prepared_workflow_revision(
    workflow_id: &str,
    snapshot_member: &transition::PreparedMember,
    pointer_member: &transition::PreparedMember,
    event: &ledger::LedgerEvent,
) -> Result<PreparedWorkflowRevision, AppError> {
    use transition::PreparedMemberKind;

    validate_workflow_id(workflow_id)?;
    if snapshot_member.kind != PreparedMemberKind::WorkflowSnapshot
        || pointer_member.kind != PreparedMemberKind::WorkflowPointer
        || snapshot_member.schema_version != WORKFLOW_SCHEMA_VERSION
        || pointer_member.schema_version != WORKFLOW_SCHEMA_VERSION
        || snapshot_member.expected_type != "absent"
        || pointer_member.expected_type != "file"
    {
        return Err(AppError::blocked(
            "prepared workflow member kind/schema/type 불일치",
        ));
    }
    let probe_path = paths::project_workflow_file(workflow_id);
    let record = parse_workflow_snapshot(&probe_path, &snapshot_member.bytes_utf8)?;
    let snapshot_path = paths::project_workflow_snapshot_file(workflow_id, record.revision);
    let pointer_path = paths::project_workflow_file(workflow_id);
    let snapshot_stored_path = format!(
        ".rpotato/workflows/{workflow_id}.snapshots/{:020}.json",
        record.revision
    );
    let pointer_stored_path = format!(".rpotato/workflows/{workflow_id}.json");
    let snapshot_member_id = prepared_workflow_member_id(
        "rpotato.workflow-snapshot-member-id/v1",
        "workflow-snapshot",
        workflow_id,
        record.revision,
    );
    let pointer_member_id = prepared_workflow_member_id(
        "rpotato.workflow-pointer-member-id/v1",
        "workflow-pointer",
        workflow_id,
        record.revision,
    );
    let pointer = parse_workflow_pointer(&pointer_path, &pointer_member.bytes_utf8)?;
    let expected_pointer_bytes = render_workflow_pointer_bytes(&record, WORKFLOW_SCHEMA_VERSION)?;
    let expected_event_details = workflow_checkpoint_event_details(&record);
    if record.workflow_id != workflow_id
        || render_workflow(&record) != snapshot_member.bytes_utf8
        || snapshot_member.path != snapshot_stored_path
        || pointer_member.path != pointer_stored_path
        || snapshot_member.binding.artifact_id.as_deref() != Some(snapshot_member_id.as_str())
        || pointer_member.binding.artifact_id.as_deref() != Some(pointer_member_id.as_str())
        || snapshot_member.binding.causal_id.is_some()
        || pointer_member.binding.causal_id.as_deref() != Some(snapshot_member_id.as_str())
        || snapshot_member.binding.event_id.as_deref() != Some(event.event_id.as_str())
        || pointer_member.binding.event_id.as_deref() != Some(event.event_id.as_str())
        || pointer.workflow_id != workflow_id
        || pointer.committed_revision != record.revision
        || pointer.artifact_hash != record.artifact_hash
        || pointer.schema_version != WORKFLOW_SCHEMA_VERSION
        || pointer_member.bytes_utf8 != expected_pointer_bytes
        || event.event_type != "workflow.checkpoint"
        || event.project_id != record.project_id
        || event.session_id != record.session_id
        || event.summary != "canonical workflow revision persisted"
        || event.details != expected_event_details
    {
        return Err(AppError::blocked(
            "prepared workflow snapshot/pointer/event binding 불일치",
        ));
    }
    Ok(PreparedWorkflowRevision {
        record,
        snapshot_path,
        snapshot_stored_path,
        snapshot_bytes: snapshot_member.bytes_utf8.clone(),
        snapshot_member_id,
        pointer_path,
        pointer_stored_path,
        pointer_bytes: pointer_member.bytes_utf8.clone(),
        pointer_member_id,
        event: event.clone(),
    })
}

pub(crate) fn prepare_current_image(
    workflow: &WorkflowRecord,
    ledger_binding: &ledger::LedgerBinding,
) -> Result<PreparedCurrentImage, AppError> {
    let expected_previous_workflow_revision = workflow
        .revision
        .checked_sub(2)
        .ok_or_else(|| AppError::blocked("prepared current image workflow revision underflow"))?;
    prepare_current_image_after(
        workflow,
        expected_previous_workflow_revision,
        ledger_binding,
    )
}

pub(crate) fn prepare_current_image_after(
    workflow: &WorkflowRecord,
    expected_previous_workflow_revision: u64,
    ledger_binding: &ledger::LedgerBinding,
) -> Result<PreparedCurrentImage, AppError> {
    prepare_current_image_after_with_binding(
        workflow,
        expected_previous_workflow_revision,
        ledger_binding,
        true,
    )
}

fn prepare_terminal_current_image_after(
    workflow: &WorkflowRecord,
    expected_previous_workflow_revision: u64,
    ledger_binding: &ledger::LedgerBinding,
) -> Result<PreparedCurrentImage, AppError> {
    prepare_current_image_after_with_binding(
        workflow,
        expected_previous_workflow_revision,
        ledger_binding,
        false,
    )
}

fn prepare_current_image_after_with_binding(
    workflow: &WorkflowRecord,
    expected_previous_workflow_revision: u64,
    ledger_binding: &ledger::LedgerBinding,
    keep_active: bool,
) -> Result<PreparedCurrentImage, AppError> {
    let path = paths::current_state_file();
    let body = fs::read_to_string(&path)
        .map_err(|err| AppError::blocked(format!("current image precondition 읽기 실패: {err}")))?;
    let previous = parse_current_state(&body, "prepared current image")?;
    if previous.schema_version != 2
        || previous.project_id != workflow.project_id
        || previous.session_id != workflow.session_id
        || previous.active_workflow.as_ref().is_none_or(|active| {
            active.workflow_id != workflow.workflow_id
                || active.revision != expected_previous_workflow_revision
        })
    {
        return Err(AppError::blocked(
            "prepared current image workflow predecessor binding 불일치",
        ));
    }
    let revision = previous
        .revision
        .checked_add(1)
        .ok_or_else(|| AppError::blocked("current-state revision overflow"))?;
    let mut snapshot = CurrentStateSnapshot {
        schema_version: 2,
        revision,
        previous_artifact_hash: previous.artifact_hash,
        project_id: previous.project_id,
        project_root: previous.project_root,
        session_id: previous.session_id,
        active_workflow: keep_active.then(|| CurrentWorkflowBinding {
            workflow_id: workflow.workflow_id.clone(),
            revision: workflow.revision,
            artifact_hash: workflow.artifact_hash.clone(),
        }),
        parent_session_id: previous.parent_session_id,
        branch_from_event_id: previous.branch_from_event_id,
        compaction_boundary: previous.compaction_boundary,
        resume_source: previous.resume_source,
        ledger_binding: ledger_binding.clone(),
        artifact_hash: String::new(),
        legacy_canonical_hash: None,
    };
    snapshot.artifact_hash = sha256_text(&render_current_state_v2_payload(&snapshot));
    let bytes = render_current_state_v2(&snapshot);
    Ok(PreparedCurrentImage {
        path,
        stored_path: "state/current-state.json".to_string(),
        artifact_id: format!("current-image-{}", snapshot.artifact_hash),
        bytes,
        revision,
    })
}

fn prepare_state_transition_current_image(
    identity: &RuntimeIdentity,
    resume_source: Option<&str>,
    active_workflow: Option<&WorkflowRecord>,
    final_binding: &ledger::LedgerBinding,
    previous: Option<&CurrentStateSnapshot>,
) -> Result<PreparedCurrentImage, AppError> {
    let revision = previous.map_or(Ok(1), |snapshot| {
        snapshot
            .revision
            .checked_add(1)
            .ok_or_else(|| AppError::blocked("current-state revision overflow"))
    })?;
    let mut snapshot = CurrentStateSnapshot {
        schema_version: 2,
        revision,
        previous_artifact_hash: previous
            .map(|snapshot| snapshot.artifact_hash.clone())
            .unwrap_or_else(|| "none".to_string()),
        project_id: identity.project_id.clone(),
        project_root: identity.project_root.clone(),
        session_id: identity.session_id.clone(),
        active_workflow: active_workflow.map(|workflow| CurrentWorkflowBinding {
            workflow_id: workflow.workflow_id.clone(),
            revision: workflow.revision,
            artifact_hash: workflow.artifact_hash.clone(),
        }),
        parent_session_id: previous.and_then(|snapshot| snapshot.parent_session_id.clone()),
        branch_from_event_id: previous.and_then(|snapshot| snapshot.branch_from_event_id.clone()),
        compaction_boundary: previous.and_then(|snapshot| snapshot.compaction_boundary.clone()),
        resume_source: resume_source.map(str::to_string),
        ledger_binding: final_binding.clone(),
        artifact_hash: String::new(),
        legacy_canonical_hash: None,
    };
    snapshot.artifact_hash = sha256_text(&render_current_state_v2_payload(&snapshot));
    let bytes = render_current_state_v2(&snapshot);
    Ok(PreparedCurrentImage {
        path: paths::current_state_file(),
        stored_path: "state/current-state.json".to_string(),
        artifact_id: format!("current-image-{}", snapshot.artifact_hash),
        bytes,
        revision,
    })
}

fn state_transition_current_member(
    prepared: &PreparedCurrentImage,
    event_id: &str,
    causal_id: Option<String>,
    expected_type: &str,
) -> transition::PreparedMember {
    transition::PreparedMember {
        kind: transition::PreparedMemberKind::CurrentImage,
        path: prepared.stored_path.clone(),
        schema_version: 2,
        binding: transition::PreparedMemberBinding {
            artifact_id: Some(prepared.artifact_id.clone()),
            causal_id,
            source_key: None,
            event_id: Some(event_id.to_string()),
        },
        bytes_utf8: prepared.bytes.clone(),
        expected_type: expected_type.to_string(),
        expected_identity: None,
        readonly: false,
        mode: 0o600,
        ownership: None,
        semantic_role_rank: 0,
    }
}

pub(crate) fn validate_prepared_state_current_member(
    bundle: &transition::PreparedSourceBundle,
    member: &transition::PreparedMember,
) -> Result<(), AppError> {
    let snapshot = parse_current_state(&member.bytes_utf8, "prepared state current member")?;
    let final_chain = bundle
        .event_chain_plan
        .last()
        .ok_or_else(|| AppError::blocked("prepared state current final event 누락"))?;
    let expected_revision = bundle
        .current_revision
        .checked_add(1)
        .ok_or_else(|| AppError::blocked("prepared state current revision overflow"))?;
    let expected_previous = if bundle.current_revision == 0 {
        "none"
    } else {
        bundle.current_artifact_hash.as_str()
    };
    let expected_type = if bundle.current_revision == 0 && bundle.current_artifact_hash == "missing"
    {
        "absent"
    } else {
        "file"
    };
    let final_binding = ledger::LedgerBinding {
        event_count: final_chain.ordinal,
        event_id: Some(final_chain.event_id.clone()),
        event_hash: final_chain.event_hash.clone(),
    };
    if member.kind != transition::PreparedMemberKind::CurrentImage
        || member.path != "state/current-state.json"
        || member.schema_version != 2
        || member.expected_type != expected_type
        || member.binding.event_id.as_deref() != Some(final_chain.event_id.as_str())
        || snapshot.schema_version != 2
        || snapshot.revision != expected_revision
        || snapshot.previous_artifact_hash != expected_previous
        || snapshot.project_id != bundle.project_id
        || snapshot.session_id != bundle.session_id
        || snapshot.ledger_binding != final_binding
        || snapshot.artifact_hash != sha256_text(&render_current_state_v2_payload(&snapshot))
        || render_current_state_v2(&snapshot) != member.bytes_utf8
        || member.binding.artifact_id.as_deref()
            != Some(format!("current-image-{}", snapshot.artifact_hash).as_str())
    {
        return Err(AppError::blocked(
            "prepared state current canonical/binding 불일치",
        ));
    }
    if let Some(active) = snapshot.active_workflow.as_ref() {
        validate_workflow_id(&active.workflow_id)?;
        if active.revision == 0 || active.artifact_hash.len() != 64 {
            return Err(AppError::blocked(
                "prepared state current active workflow binding 불일치",
            ));
        }
    }
    Ok(())
}

fn prepared_state_current_image(
    bundle: &transition::PreparedSourceBundle,
) -> Result<PreparedCurrentImage, AppError> {
    let member = bundle
        .additional_members
        .last()
        .ok_or_else(|| AppError::blocked("prepared state current member 누락"))?;
    validate_prepared_state_current_member(bundle, member)?;
    let snapshot = parse_current_state(&member.bytes_utf8, "prepared state current decode")?;
    Ok(PreparedCurrentImage {
        path: paths::current_state_file(),
        stored_path: member.path.clone(),
        artifact_id: format!("current-image-{}", snapshot.artifact_hash),
        bytes: member.bytes_utf8.clone(),
        revision: snapshot.revision,
    })
}

fn validate_state_transition_current_cas(
    bundle: &transition::PreparedSourceBundle,
    completed_bytes: &str,
) -> Result<bool, AppError> {
    let path = paths::current_state_file();
    let existing = match fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => {
            return Err(AppError::blocked(format!(
                "prepared state current CAS 읽기 실패: {err}"
            )))
        }
    };
    if existing.as_deref() == Some(completed_bytes.as_bytes()) {
        return Ok(true);
    }
    if bundle.current_revision == 0 {
        match (bundle.current_artifact_hash.as_str(), existing.as_deref()) {
            ("missing", None) => return Ok(false),
            (hash, Some(bytes)) if sha256_bytes(bytes) == hash => return Ok(false),
            _ => {
                return Err(AppError::blocked(
                    "prepared state current missing/preserved CAS conflict",
                ))
            }
        }
    }
    let body = existing
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .ok_or_else(|| AppError::blocked("prepared state current UTF-8/CAS conflict"))?;
    let current = parse_current_state(&body, "prepared state current CAS")?;
    if current.revision != bundle.current_revision
        || current.artifact_hash != bundle.current_artifact_hash
    {
        return Err(AppError::blocked(
            "prepared state current exact CAS conflict",
        ));
    }
    Ok(false)
}

pub(crate) fn recover_prepared_state_transition(
    bundle: &transition::PreparedSourceBundle,
) -> Result<(), AppError> {
    let planned = transition::planned_events(bundle)?;
    let current_image = prepared_state_current_image(bundle)?;
    validate_state_transition_current_cas(bundle, &current_image.bytes)?;
    let workflow = if bundle.intent_kind == "checkpoint-workflow" {
        let workflow_id = bundle
            .workflow_id
            .as_deref()
            .ok_or_else(|| AppError::blocked("prepared checkpoint workflow id 누락"))?;
        let guard = WorkflowCheckpointGuard::acquire(workflow_id)?;
        let prepared = decode_prepared_workflow_revision(
            workflow_id,
            &bundle.additional_members[0],
            &bundle.additional_members[1],
            &bundle.semantic_events[0],
        )?;
        Some((guard, prepared))
    } else {
        None
    };
    let writer = ledger::LedgerWriterGuard::acquire()?;
    let expected_binding = ledger::LedgerBinding {
        event_count: planned[0].ordinal,
        event_id: Some(planned[0].event.event_id.clone()),
        event_hash: planned[0].event_hash.clone(),
    };
    let mut port = StateTransitionRecoveryPort {
        bundle,
        current_image: &current_image,
        workflow: workflow.as_ref(),
        writer: &writer,
        sink: writer.event_sink(&planned),
        expected_binding,
    };
    workflow_recovery::recover_prepared_state_transition(&mut port)
}

struct StateTransitionRecoveryPort<'a> {
    bundle: &'a transition::PreparedSourceBundle,
    current_image: &'a PreparedCurrentImage,
    workflow: Option<&'a (WorkflowCheckpointGuard, PreparedWorkflowRevision)>,
    writer: &'a ledger::LedgerWriterGuard,
    sink: ledger::EventSink<'a>,
    expected_binding: ledger::LedgerBinding,
}

impl PreparedStateRecoveryPort for StateTransitionRecoveryPort<'_> {
    fn install_reconcile_backup(&mut self) -> Result<(), AppError> {
        install_prepared_reconcile_backup(self.bundle)
    }

    fn install_workflow_snapshot(&mut self) -> Result<(), AppError> {
        if let Some((guard, prepared)) = self.workflow {
            guard.install_snapshot(prepared)?;
        }
        Ok(())
    }

    fn append_event(&mut self) -> Result<(), AppError> {
        self.sink
            .append_planned_under_guard(0, &self.bundle.semantic_events[0])
            .map(|_| ())
    }

    fn install_workflow_pointer(&mut self) -> Result<(), AppError> {
        if let Some((guard, prepared)) = self.workflow {
            guard.install_pointer(prepared)?;
        }
        Ok(())
    }

    fn finish_events(&mut self) -> Result<(), AppError> {
        self.sink.finish()
    }

    fn validate_ledger_binding(&mut self) -> Result<(), AppError> {
        if self.writer.binding()? != self.expected_binding {
            return Err(AppError::blocked(
                "prepared state transition ledger successor conflict",
            ));
        }
        Ok(())
    }

    fn install_current_state(&mut self) -> Result<(), AppError> {
        if !validate_state_transition_current_cas(self.bundle, &self.current_image.bytes)? {
            atomic_replace_bytes(
                &self.current_image.path,
                self.current_image.bytes.as_bytes(),
            )?;
        }
        Ok(())
    }

    fn converge_projections(&mut self) -> Result<(), AppError> {
        self.sink.converge_derived(&self.bundle.project_id)
    }
}

fn install_prepared_reconcile_backup(
    bundle: &transition::PreparedSourceBundle,
) -> Result<(), AppError> {
    if bundle.intent_kind != "reconcile"
        || bundle.current_revision != 0
        || bundle.current_artifact_hash == "missing"
    {
        return Ok(());
    }
    let member = bundle
        .additional_members
        .first()
        .ok_or_else(|| AppError::blocked("prepared reconcile backup member 누락"))?;
    let basename = PathBuf::from(&member.path)
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| AppError::blocked("prepared reconcile backup basename 불일치"))?
        .to_string();
    let path = paths::state_dir().join(basename);
    if path.exists() {
        let existing = fs::read(&path).map_err(|err| {
            AppError::blocked(format!("prepared reconcile backup reread 실패: {err}"))
        })?;
        if existing != member.bytes_utf8.as_bytes() {
            return Err(AppError::blocked(
                "prepared reconcile backup immutable conflict",
            ));
        }
        return Ok(());
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&path)
        .map_err(|err| AppError::runtime(format!("prepared reconcile backup 생성 실패: {err}")))?;
    file.write_all(member.bytes_utf8.as_bytes())
        .map_err(|err| AppError::runtime(format!("prepared reconcile backup 쓰기 실패: {err}")))?;
    file.sync_all()
        .map_err(|err| AppError::runtime(format!("prepared reconcile backup sync 실패: {err}")))?;
    sync_parent(&path)
}

struct StateTransitionRequest<'a> {
    intent_id: &'a str,
    intent: transition::CurrentStateIntent,
    identity: &'a RuntimeIdentity,
    event: &'a ledger::LedgerEvent,
    resume_source: Option<&'a str>,
    active_workflow: Option<&'a WorkflowRecord>,
    previous: Option<&'a CurrentStateSnapshot>,
    workflow: Option<(&'a WorkflowCheckpointGuard, &'a PreparedWorkflowRevision)>,
}

fn transition_project_current_state_under_guard(
    transition_guard: &transition::TransitionGuard,
    request: StateTransitionRequest<'_>,
) -> Result<PreparedCurrentImage, AppError> {
    let StateTransitionRequest {
        intent_id,
        intent,
        identity,
        event,
        resume_source,
        active_workflow,
        previous,
        workflow,
    } = request;
    let writer = ledger::LedgerWriterGuard::acquire()?;
    let before_ledger = writer.binding()?;
    let planned = writer.plan_events(std::slice::from_ref(event))?;
    let final_binding = ledger::LedgerBinding {
        event_count: planned[0].ordinal,
        event_id: Some(planned[0].event.event_id.clone()),
        event_hash: planned[0].event_hash.clone(),
    };
    let current_image = prepare_state_transition_current_image(
        identity,
        resume_source,
        active_workflow,
        &final_binding,
        previous,
    )?;
    let current_revision = previous.map_or(0, |snapshot| snapshot.revision);
    let current_artifact_hash = previous
        .map(|snapshot| snapshot.artifact_hash.as_str())
        .unwrap_or("missing");
    let mut bundle = transition::prepare_state_transition_bundle(
        intent_id,
        intent,
        identity,
        workflow.map(|(_, prepared)| prepared.record.workflow_id.as_str()),
        current_revision,
        current_artifact_hash,
        before_ledger,
    )?;
    transition::bind_planned_events(&mut bundle, &planned)?;
    let expected_type = if previous.is_some() { "file" } else { "absent" };
    let mut members = Vec::new();
    let causal_id = workflow.map(|(_, prepared)| prepared.snapshot_member_id.clone());
    if let Some((_, prepared)) = workflow {
        members.push(prepared_workflow_member(
            transition::PreparedMemberKind::WorkflowSnapshot,
            prepared.snapshot_stored_path.clone(),
            prepared.snapshot_member_id.clone(),
            None,
            event.event_id.clone(),
            prepared.snapshot_bytes.clone(),
            "absent",
        ));
        members.push(prepared_workflow_member(
            transition::PreparedMemberKind::WorkflowPointer,
            prepared.pointer_stored_path.clone(),
            prepared.pointer_member_id.clone(),
            Some(prepared.snapshot_member_id.clone()),
            event.event_id.clone(),
            prepared.pointer_bytes.clone(),
            "file",
        ));
    }
    members.push(state_transition_current_member(
        &current_image,
        &event.event_id,
        causal_id,
        expected_type,
    ));
    transition::bind_additional_members(&mut bundle, members)?;
    let journal = transition_guard.commit(&bundle)?;
    let checkpoint = intent == transition::CurrentStateIntent::CheckpointWorkflow;
    let mut port = StateTransitionTransactionAdapter {
        transition_guard,
        bundle: &bundle,
        current: &current_image,
        workflow,
        event,
        journal: &journal,
        sink: writer.event_sink(&planned),
    };
    transaction_coordinator::execute_state_transition(&mut port, checkpoint)?;
    Ok(current_image)
}

struct StateTransitionTransactionAdapter<'a> {
    transition_guard: &'a transition::TransitionGuard,
    bundle: &'a transition::PreparedSourceBundle,
    current: &'a PreparedCurrentImage,
    workflow: Option<(&'a WorkflowCheckpointGuard, &'a PreparedWorkflowRevision)>,
    event: &'a ledger::LedgerEvent,
    journal: &'a std::path::Path,
    sink: ledger::EventSink<'a>,
}

impl StateTransitionTransactionPort for StateTransitionTransactionAdapter<'_> {
    fn fault(&mut self, point: StateTransitionFault) -> Result<(), AppError> {
        match point {
            StateTransitionFault::Journal => state_transition_fault("after-journal"),
            StateTransitionFault::CheckpointTransaction => checkpoint_fault("after-transaction"),
            StateTransitionFault::CheckpointSnapshot => checkpoint_fault("after-snapshot"),
            StateTransitionFault::Artifacts => state_transition_fault("after-artifacts"),
            StateTransitionFault::Ledger => state_transition_fault("after-ledger"),
            StateTransitionFault::CheckpointLedger => checkpoint_fault("after-ledger"),
            StateTransitionFault::CheckpointPointer => checkpoint_fault("after-pointer"),
            StateTransitionFault::Current => state_transition_fault("after-current"),
            StateTransitionFault::Projection => state_transition_fault("after-projection"),
        }
    }

    fn install_snapshot(&mut self) -> Result<(), AppError> {
        if let Some((guard, prepared)) = self.workflow {
            guard.install_snapshot(prepared)?;
        }
        Ok(())
    }

    fn append_event(&mut self) -> Result<(), AppError> {
        self.sink
            .append_planned_under_guard(0, self.event)
            .map(|_| ())
    }

    fn install_pointer(&mut self) -> Result<(), AppError> {
        if let Some((guard, prepared)) = self.workflow {
            guard.install_pointer(prepared)?;
        }
        Ok(())
    }

    fn finish_events(&mut self) -> Result<(), AppError> {
        self.sink.finish()
    }

    fn install_current(&mut self) -> Result<(), AppError> {
        if !validate_state_transition_current_cas(self.bundle, &self.current.bytes)? {
            atomic_replace_bytes(&self.current.path, self.current.bytes.as_bytes())?;
        }
        Ok(())
    }

    fn converge(&mut self) -> Result<(), AppError> {
        self.sink.converge_derived(&self.bundle.project_id)
    }

    fn remove_journal(&mut self) -> Result<(), AppError> {
        self.transition_guard.remove(self.bundle, self.journal)
    }
}

fn prepared_workflow_member(
    kind: transition::PreparedMemberKind,
    path: String,
    artifact_id: String,
    causal_id: Option<String>,
    event_id: String,
    bytes_utf8: String,
    expected_type: &str,
) -> transition::PreparedMember {
    transition::PreparedMember {
        kind,
        path,
        schema_version: WORKFLOW_SCHEMA_VERSION,
        binding: transition::PreparedMemberBinding {
            artifact_id: Some(artifact_id),
            causal_id,
            source_key: None,
            event_id: Some(event_id),
        },
        bytes_utf8,
        expected_type: expected_type.to_string(),
        expected_identity: None,
        readonly: false,
        mode: 0o600,
        ownership: None,
        semantic_role_rank: 0,
    }
}

pub(crate) struct TerminalActionRequest<'a> {
    pub intent_id: &'a str,
    pub intent_kind: &'a str,
    pub identity: &'a RuntimeIdentity,
    pub before: &'a WorkflowRecord,
    pub terminal: WorkflowRecord,
    pub audit_event_type: &'a str,
    pub audit_summary: &'a str,
    pub audit_details: &'a str,
    pub source: Option<PreparedTerminalSource>,
}

pub(crate) fn transition_project_current_state_prepared_terminal_action(
    transition_guard: &transition::TransitionGuard,
    workflow_guard: &WorkflowCheckpointGuard,
    request: TerminalActionRequest<'_>,
) -> Result<WorkflowRecord, AppError> {
    let TerminalActionRequest {
        intent_id,
        intent_kind,
        identity,
        before,
        terminal,
        audit_event_type,
        audit_summary,
        audit_details,
        source,
    } = request;
    let current_lease = current_state_lease_view_under_transition()?;
    let revision = workflow_guard.prepare_revision(before, terminal)?;
    let e0 = ledger::new_event_for(
        identity,
        "runtime.intent.accepted",
        "interactive runtime intent accepted",
        &format!(
            "intent_id={intent_id} intent_kind={intent_kind} workflow_id={}",
            before.workflow_id
        ),
    );
    let e2 = ledger::new_event_for(
        identity,
        audit_event_type,
        audit_summary,
        &format!(
            "intent_id={intent_id} workflow_id={} revision={} artifact_hash={} {audit_details}",
            revision.record.workflow_id, revision.record.revision, revision.record.artifact_hash
        ),
    );
    let events = vec![e0, revision.event.clone(), e2];
    let writer = ledger::LedgerWriterGuard::acquire()?;
    let ledger_binding = writer.binding()?;
    let planned = writer.plan_events(&events)?;
    let final_binding = ledger::LedgerBinding {
        event_count: planned[2].ordinal,
        event_id: Some(planned[2].event.event_id.clone()),
        event_hash: planned[2].event_hash.clone(),
    };
    let current =
        prepare_terminal_current_image_after(&revision.record, before.revision, &final_binding)?;
    let source_context = source.as_ref().map(|source| {
        (
            source.plan.clone(),
            source.before.as_slice(),
            source.proposed.as_slice(),
        )
    });
    let mut bundle = transition::prepare_terminal_action_bundle_with_context(
        intent_id,
        intent_kind,
        &before.workflow_id,
        source_context,
        transition::PreparedBundleContext {
            identity,
            lease: &current_lease,
            ledger_binding,
        },
    )?;
    transition::bind_planned_events(&mut bundle, &planned)?;
    transition::bind_additional_members(
        &mut bundle,
        vec![
            prepared_workflow_member(
                transition::PreparedMemberKind::WorkflowSnapshot,
                revision.snapshot_stored_path.clone(),
                revision.snapshot_member_id.clone(),
                None,
                events[1].event_id.clone(),
                revision.snapshot_bytes.clone(),
                "absent",
            ),
            prepared_workflow_member(
                transition::PreparedMemberKind::WorkflowPointer,
                revision.pointer_stored_path.clone(),
                revision.pointer_member_id.clone(),
                Some(revision.snapshot_member_id.clone()),
                events[1].event_id.clone(),
                revision.pointer_bytes.clone(),
                "file",
            ),
            state_transition_current_member(
                &current,
                &events[2].event_id,
                Some(revision.snapshot_member_id.clone()),
                "file",
            ),
        ],
    )?;
    let journal = transition_guard.commit(&bundle)?;
    let mut port = StateTerminalActionTransactionPort {
        transition_guard: Some(transition_guard),
        workflow_guard,
        bundle: &bundle,
        revision: &revision,
        current: &current,
        events: &events,
        journal: &journal,
        sink: writer.event_sink(&planned),
    };
    transaction_coordinator::execute_terminal_action_transaction(
        &mut port,
        TransactionExecution::Commit,
    )?;
    Ok(revision.record)
}

struct StateTerminalActionTransactionPort<'a> {
    transition_guard: Option<&'a transition::TransitionGuard>,
    workflow_guard: &'a WorkflowCheckpointGuard,
    bundle: &'a transition::PreparedSourceBundle,
    revision: &'a PreparedWorkflowRevision,
    current: &'a PreparedCurrentImage,
    events: &'a [ledger::LedgerEvent],
    journal: &'a std::path::Path,
    sink: ledger::EventSink<'a>,
}

impl TerminalActionTransactionPort for StateTerminalActionTransactionPort<'_> {
    fn fault(&mut self, point: TerminalActionFault) -> Result<(), AppError> {
        terminal_action_fault(point.as_str())
    }

    fn append_event(&mut self, index: usize) -> Result<(), AppError> {
        let event = self
            .events
            .get(index)
            .ok_or_else(|| AppError::blocked("prepared terminal event index 범위 초과"))?;
        self.sink
            .append_planned_under_guard(index, event)
            .map(|_| ())
    }

    fn install_source(&mut self) -> Result<(), AppError> {
        if self.bundle.source_install.is_some() {
            install_prepared_source_bundle(self.bundle, self.journal)?;
        }
        Ok(())
    }

    fn install_snapshot(&mut self) -> Result<(), AppError> {
        self.workflow_guard.install_snapshot(self.revision)
    }

    fn install_pointer(&mut self) -> Result<(), AppError> {
        self.workflow_guard.install_pointer(self.revision)
    }

    fn finish_events(&mut self) -> Result<(), AppError> {
        self.sink.finish()
    }

    fn install_current(&mut self) -> Result<(), AppError> {
        install_current_image(
            self.current,
            self.bundle.current_revision,
            &self.bundle.current_artifact_hash,
        )
    }

    fn converge(&mut self) -> Result<(), AppError> {
        self.sink.converge_prepared(self.bundle, self.journal)
    }

    fn remove_journal(&mut self) -> Result<(), AppError> {
        self.transition_guard
            .ok_or_else(|| AppError::blocked("prepared terminal cleanup guard 누락"))?
            .remove(self.bundle, self.journal)
    }
}

pub(crate) struct PreparedApprovalTransition<'a> {
    pub transition_guard: Option<&'a transition::TransitionGuard>,
    pub workflow_guard: &'a WorkflowCheckpointGuard,
    pub writer: &'a ledger::LedgerWriterGuard,
    pub planned: &'a [ledger::PlannedEvent],
    pub bundle: &'a transition::PreparedSourceBundle,
    pub r1: &'a PreparedWorkflowRevision,
    pub r2: &'a PreparedWorkflowRevision,
    pub transcript: &'a transcript::PreparedTranscriptTurn,
    pub current: &'a PreparedCurrentImage,
    pub events: &'a [ledger::LedgerEvent],
}

pub(crate) fn transition_project_current_state_prepared_approval(
    prepared: PreparedApprovalTransition<'_>,
) -> Result<(), AppError> {
    let transition_guard = prepared
        .transition_guard
        .ok_or_else(|| AppError::blocked("prepared approval transition guard 누락"))?;
    let journal = transition_guard.commit(prepared.bundle)?;
    execute_prepared_approval(prepared, &journal, TransactionExecution::Commit)
}

pub(crate) fn recover_project_current_state_prepared_approval(
    prepared: PreparedApprovalTransition<'_>,
    journal: &std::path::Path,
) -> Result<(), AppError> {
    let lag_path = transition::projection_lag_path(prepared.bundle)?;
    let mut port = ApprovalProjectionRecoveryPort {
        prepared: Some(prepared),
        journal,
        lag_path,
    };
    projection_barrier::recover_through_projection_barrier(&mut port)
}

struct ApprovalProjectionRecoveryPort<'a> {
    prepared: Option<PreparedApprovalTransition<'a>>,
    journal: &'a std::path::Path,
    lag_path: PathBuf,
}

impl ApprovalProjectionRecoveryPort<'_> {
    fn prepared(&self) -> &PreparedApprovalTransition<'_> {
        self.prepared
            .as_ref()
            .expect("approval recovery port retains prepared transition")
    }
}

impl ProjectionBarrierRecoveryPort for ApprovalProjectionRecoveryPort<'_> {
    fn lag_exists(&self) -> bool {
        self.lag_path.exists()
    }

    fn lag_temp_exists(&self) -> bool {
        self.lag_path.with_extension("json.tmp").exists()
    }

    fn target_is_converged(&self) -> Result<bool, AppError> {
        let prepared = self.prepared();
        prepared
            .writer
            .prepared_target_is_converged(prepared.bundle, self.journal)
    }

    fn install_lag(&self) -> Result<PathBuf, AppError> {
        let prepared = self.prepared();
        transition::install_projection_lag(prepared.bundle).map_err(|error| {
            AppError::blocked(format!(
                "projection lag install 실패\n- code: projection.lag-install-failed\n- intent: {}\n- error: {}",
                prepared.bundle.intent_id, error.message,
            ))
        })
    }

    fn repair_required(&self, lag: &std::path::Path) -> AppError {
        AppError::blocked(format!(
            "projection repair 필요\n- code: projection.repair-required\n- intent: {}\n- lag: {}\n- error: interrupted repair requires a durable lag marker",
            self.prepared().bundle.intent_id,
            lag.display()
        ))
    }

    fn resume_recovery(&mut self) -> Result<(), AppError> {
        let prepared = self
            .prepared
            .take()
            .expect("approval recovery executes at most once");
        execute_prepared_approval(prepared, self.journal, TransactionExecution::Recovery)
    }
}

fn execute_prepared_approval(
    prepared: PreparedApprovalTransition<'_>,
    journal: &std::path::Path,
    execution: TransactionExecution,
) -> Result<(), AppError> {
    let PreparedApprovalTransition {
        transition_guard,
        workflow_guard,
        writer,
        planned,
        bundle,
        r1,
        r2,
        transcript,
        current,
        events,
    } = prepared;
    let mut port = StateApprovalTransactionPort {
        transition_guard,
        workflow_guard,
        bundle,
        r1,
        r2,
        transcript,
        current,
        events,
        journal,
        sink: writer.event_sink(planned),
    };
    transaction_coordinator::execute_approval_transaction(&mut port, execution)
}

struct StateApprovalTransactionPort<'a> {
    transition_guard: Option<&'a transition::TransitionGuard>,
    workflow_guard: &'a WorkflowCheckpointGuard,
    bundle: &'a transition::PreparedSourceBundle,
    r1: &'a PreparedWorkflowRevision,
    r2: &'a PreparedWorkflowRevision,
    transcript: &'a transcript::PreparedTranscriptTurn,
    current: &'a PreparedCurrentImage,
    events: &'a [ledger::LedgerEvent],
    journal: &'a std::path::Path,
    sink: ledger::EventSink<'a>,
}

impl ApprovalTransactionPort for StateApprovalTransactionPort<'_> {
    fn fault(&mut self, point: ApprovalFault) -> Result<(), AppError> {
        crate::patch::approval_transaction_fault(point.as_str())
    }

    fn append_event(&mut self, index: usize) -> Result<(), AppError> {
        let event = self
            .events
            .get(index)
            .ok_or_else(|| AppError::blocked("prepared approval event index 범위 초과"))?;
        self.sink
            .append_planned_under_guard(index, event)
            .map(|_| ())
    }

    fn install_snapshot(&mut self, revision: ApprovalRevision) -> Result<(), AppError> {
        let prepared = match revision {
            ApprovalRevision::First => self.r1,
            ApprovalRevision::Second => self.r2,
        };
        self.workflow_guard.install_snapshot(prepared)
    }

    fn install_pointer(&mut self, revision: ApprovalRevision) -> Result<(), AppError> {
        let prepared = match revision {
            ApprovalRevision::First => self.r1,
            ApprovalRevision::Second => self.r2,
        };
        self.workflow_guard.install_pointer(prepared)
    }

    fn install_source(&mut self) -> Result<(), AppError> {
        install_prepared_source_bundle(self.bundle, self.journal)
    }

    fn install_transcript(&mut self) -> Result<(), AppError> {
        transcript::install_prepared_no_stream_tool_turn(self.transcript)
    }

    fn install_current(&mut self) -> Result<(), AppError> {
        install_current_image(
            self.current,
            self.bundle.current_revision,
            &self.bundle.current_artifact_hash,
        )
    }

    fn finish_events(&mut self) -> Result<(), AppError> {
        self.sink.finish()
    }

    fn converge(&mut self) -> Result<(), AppError> {
        crate::patch::approval_projection_fault()
            .and_then(|_| self.sink.converge_prepared(self.bundle, self.journal))
    }

    fn projection_repair_required(&mut self, convergence_error: AppError) -> AppError {
        match transition::install_projection_lag(self.bundle) {
            Ok(lag) => AppError::blocked(format!(
                "projection repair 필요\n- code: projection.repair-required\n- intent: {}\n- lag: {}\n- error: {}",
                self.bundle.intent_id,
                lag.display(),
                convergence_error.message,
            )),
            Err(lag_error) => AppError::blocked(format!(
                "projection lag install 실패\n- code: projection.lag-install-failed\n- intent: {}\n- converge error: {}\n- lag error: {}",
                self.bundle.intent_id, convergence_error.message, lag_error.message,
            )),
        }
    }

    fn remove_projection_lag(&mut self) -> Result<(), AppError> {
        transition::remove_projection_lag(self.bundle)
    }

    fn validate_cleanup_authority(&mut self) -> Result<(), AppError> {
        transition::validate_committed_bundle_cleanup_authority(self.bundle, self.journal)
    }

    fn remove_journal(&mut self) -> Result<(), AppError> {
        self.transition_guard
            .ok_or_else(|| AppError::blocked("prepared approval cleanup guard 누락"))?
            .remove(self.bundle, self.journal)
    }
}

pub(crate) struct PreparedVerificationTransition<'a> {
    pub transition_guard: Option<&'a transition::TransitionGuard>,
    pub workflow_guard: &'a WorkflowCheckpointGuard,
    pub writer: &'a ledger::LedgerWriterGuard,
    pub planned: &'a [ledger::PlannedEvent],
    pub bundle: &'a transition::PreparedSourceBundle,
    pub revision: &'a PreparedWorkflowRevision,
    pub current: &'a PreparedCurrentImage,
    pub events: &'a [ledger::LedgerEvent],
}

pub(crate) fn transition_project_current_state_prepared_verification(
    prepared: PreparedVerificationTransition<'_>,
) -> Result<(), AppError> {
    let transition_guard = prepared
        .transition_guard
        .ok_or_else(|| AppError::blocked("prepared verification transition guard 누락"))?;
    let journal = transition_guard.commit(prepared.bundle)?;
    execute_prepared_verification(prepared, &journal, TransactionExecution::Commit)
}

pub(crate) fn recover_project_current_state_prepared_verification(
    prepared: PreparedVerificationTransition<'_>,
    journal: &std::path::Path,
) -> Result<(), AppError> {
    execute_prepared_verification(prepared, journal, TransactionExecution::Recovery)
}

fn execute_prepared_verification(
    prepared: PreparedVerificationTransition<'_>,
    journal: &std::path::Path,
    execution: TransactionExecution,
) -> Result<(), AppError> {
    let PreparedVerificationTransition {
        transition_guard,
        workflow_guard,
        writer,
        planned,
        bundle,
        revision,
        current,
        events,
    } = prepared;
    let mut port = StateVerificationTransactionPort {
        transition_guard,
        workflow_guard,
        bundle,
        revision,
        current,
        events,
        journal,
        sink: writer.event_sink(planned),
    };
    transaction_coordinator::execute_verification_transaction(&mut port, execution)
}

struct StateVerificationTransactionPort<'a> {
    transition_guard: Option<&'a transition::TransitionGuard>,
    workflow_guard: &'a WorkflowCheckpointGuard,
    bundle: &'a transition::PreparedSourceBundle,
    revision: &'a PreparedWorkflowRevision,
    current: &'a PreparedCurrentImage,
    events: &'a [ledger::LedgerEvent],
    journal: &'a std::path::Path,
    sink: ledger::EventSink<'a>,
}

impl VerificationTransactionPort for StateVerificationTransactionPort<'_> {
    fn fault(&mut self, point: VerificationFault) -> Result<(), AppError> {
        crate::patch::verification_approval_transaction_fault(point.as_str())
    }

    fn append_event(&mut self, index: usize) -> Result<(), AppError> {
        let event = self
            .events
            .get(index)
            .ok_or_else(|| AppError::blocked("prepared verification event index 범위 초과"))?;
        self.sink
            .append_planned_under_guard(index, event)
            .map(|_| ())
    }

    fn install_snapshot(&mut self) -> Result<(), AppError> {
        self.workflow_guard.install_snapshot(self.revision)
    }

    fn install_pointer(&mut self) -> Result<(), AppError> {
        self.workflow_guard.install_pointer(self.revision)
    }

    fn install_current(&mut self) -> Result<(), AppError> {
        install_current_image(
            self.current,
            self.bundle.current_revision,
            &self.bundle.current_artifact_hash,
        )
    }

    fn finish_events(&mut self) -> Result<(), AppError> {
        self.sink.finish()
    }

    fn converge(&mut self) -> Result<(), AppError> {
        self.sink.converge_prepared(self.bundle, self.journal)
    }

    fn remove_journal(&mut self) -> Result<(), AppError> {
        self.transition_guard
            .ok_or_else(|| AppError::blocked("prepared verification cleanup guard 누락"))?
            .remove(self.bundle, self.journal)
    }
}

pub(crate) fn recover_project_current_state_prepared_terminal_action(
    bundle: &transition::PreparedSourceBundle,
    journal: &std::path::Path,
) -> Result<(), AppError> {
    let planned = transition::planned_events(bundle)?;
    if planned.len() != 3 || bundle.additional_members.len() != 3 {
        return Err(AppError::blocked(
            "prepared terminal recovery exact shape 불일치",
        ));
    }
    let workflow_id = bundle
        .workflow_id
        .as_deref()
        .ok_or_else(|| AppError::blocked("prepared terminal workflow id 누락"))?;
    let revision = decode_prepared_workflow_revision(
        workflow_id,
        &bundle.additional_members[0],
        &bundle.additional_members[1],
        &bundle.semantic_events[1],
    )?;
    let final_binding = ledger::LedgerBinding {
        event_count: planned[2].ordinal,
        event_id: Some(planned[2].event.event_id.clone()),
        event_hash: planned[2].event_hash.clone(),
    };
    let current = decode_prepared_terminal_current_image(
        &bundle.additional_members[2],
        &revision.record,
        &final_binding,
        &revision.snapshot_member_id,
        &bundle.semantic_events[2].event_id,
    )?;
    validate_current_state_recovery_cas(
        bundle.current_revision,
        &bundle.current_artifact_hash,
        Some(&current.bytes),
    )?;
    if bundle.source_install.is_some() {
        validate_prepared_source_parent(bundle)?;
    }
    let workflow_guard = WorkflowCheckpointGuard::acquire(workflow_id)?;
    let predecessor_revision = revision
        .record
        .revision
        .checked_sub(1)
        .ok_or_else(|| AppError::blocked("prepared terminal predecessor underflow"))?;
    let installed = workflow_guard.load_recovery_current(&[
        (predecessor_revision, revision.record.previous_hash.as_str()),
        (
            revision.record.revision,
            revision.record.artifact_hash.as_str(),
        ),
    ])?;
    let predecessor = installed.revision == predecessor_revision
        && installed.artifact_hash == revision.record.previous_hash;
    if installed != revision.record && !predecessor {
        return Err(AppError::blocked(
            "prepared terminal workflow predecessor conflict",
        ));
    }
    let writer = ledger::LedgerWriterGuard::acquire()?;
    let mut port = StateTerminalActionTransactionPort {
        transition_guard: None,
        workflow_guard: &workflow_guard,
        bundle,
        revision: &revision,
        current: &current,
        events: &bundle.semantic_events,
        journal,
        sink: writer.event_sink(&planned),
    };
    transaction_coordinator::execute_terminal_action_transaction(
        &mut port,
        TransactionExecution::Recovery,
    )
}

fn terminal_action_fault(point: &str) -> Result<(), AppError> {
    if cfg!(debug_assertions)
        && std::env::var("RPOTATO_TEST_TERMINAL_ACTION_FAULT").as_deref() == Ok(point)
    {
        return Err(AppError::runtime(format!(
            "injected terminal action fault: {point}"
        )));
    }
    Ok(())
}

fn read_valid_current_for_transition() -> Result<Option<CurrentStateSnapshot>, AppError> {
    let path = paths::current_state_file();
    let body = match fs::read_to_string(&path) {
        Ok(body) => body,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(AppError::blocked(format!(
                "current-state transition precondition 읽기 실패: {err}"
            )))
        }
    };
    let snapshot = parse_current_state(&body, "current-state transition precondition")?;
    if snapshot.schema_version == 1 {
        promote_current_state_v1()?;
        return read_valid_current_for_transition();
    }
    Ok(Some(snapshot))
}

fn internal_transition_intent_id(event: &ledger::LedgerEvent) -> String {
    format!("intent-{}", event.event_id)
}

fn commit_state_event(
    intent_id: &str,
    intent: transition::CurrentStateIntent,
    identity: &RuntimeIdentity,
    event: &ledger::LedgerEvent,
    resume_source: Option<&str>,
    active_workflow_id: Option<&str>,
) -> Result<PreparedCurrentImage, AppError> {
    let transition_guard = transition::TransitionGuard::acquire_for(&identity.project_id, intent)?;
    let previous = read_valid_current_for_transition()?;
    if previous
        .as_ref()
        .is_some_and(|snapshot| snapshot.project_id != identity.project_id)
    {
        return Err(AppError::blocked(
            "state transition current project binding 불일치",
        ));
    }
    if intent == transition::CurrentStateIntent::Bootstrap {
        if let Some(snapshot) = previous.as_ref() {
            if snapshot.ledger_binding != ledger::validated_ledger_binding()? {
                return Err(AppError::blocked(
                    "bootstrap existing current/ledger binding 불일치",
                ));
            }
            return Ok(PreparedCurrentImage {
                path: paths::current_state_file(),
                stored_path: "state/current-state.json".to_string(),
                artifact_id: format!("current-image-{}", snapshot.artifact_hash),
                bytes: render_current_state_v2(snapshot),
                revision: snapshot.revision,
            });
        }
    }
    let active_workflow = active_workflow_id
        .map(load_workflow_under_transition)
        .transpose()?;
    if active_workflow
        .as_ref()
        .is_some_and(|workflow| workflow.session_id != identity.session_id)
    {
        return Err(AppError::blocked(
            "state transition active workflow session binding 불일치",
        ));
    }
    transition_project_current_state_under_guard(
        &transition_guard,
        StateTransitionRequest {
            intent_id,
            intent,
            identity,
            event,
            resume_source,
            active_workflow: active_workflow.as_ref(),
            previous: previous.as_ref(),
            workflow: None,
        },
    )
}

fn reconcile_invalid_current_under_guard(
    transition_guard: &transition::TransitionGuard,
    identity: &RuntimeIdentity,
    reason: &str,
    before_bytes: &str,
) -> Result<(ledger::LedgerEvent, PathBuf), AppError> {
    let event_type = match reason {
        "corrupt" => "state.reconcile.corrupt_recovered",
        "stale" => "state.reconcile.stale_recovered",
        _ => return Err(AppError::blocked("reconcile preserved reason 불일치")),
    };
    let event = ledger::new_event_for(
        identity,
        event_type,
        "invalid current-state preserved and recovered",
        &format!("reason={reason}"),
    );
    let intent_id = internal_transition_intent_id(&event);
    let writer = ledger::LedgerWriterGuard::acquire()?;
    let before_ledger = writer.binding()?;
    let planned = writer.plan_events(std::slice::from_ref(&event))?;
    let final_binding = ledger::LedgerBinding {
        event_count: planned[0].ordinal,
        event_id: Some(planned[0].event.event_id.clone()),
        event_hash: planned[0].event_hash.clone(),
    };
    let current_image = prepare_state_transition_current_image(
        identity,
        Some("state-reconcile"),
        None,
        &final_binding,
        None,
    )?;
    let before_hash = sha256_bytes(before_bytes.as_bytes());
    let mut bundle = transition::prepare_state_transition_bundle(
        &intent_id,
        transition::CurrentStateIntent::Reconcile,
        identity,
        None,
        0,
        &before_hash,
        before_ledger,
    )?;
    transition::bind_planned_events(&mut bundle, &planned)?;
    let backup_path = format!("state/current-state.json.{reason}.{intent_id}");
    let backup = transition::PreparedMember {
        kind: transition::PreparedMemberKind::ToolOutput,
        path: backup_path,
        schema_version: 1,
        binding: transition::PreparedMemberBinding {
            artifact_id: Some(format!("state-backup-{before_hash}")),
            causal_id: None,
            source_key: None,
            event_id: Some(event.event_id.clone()),
        },
        bytes_utf8: before_bytes.to_string(),
        expected_type: "absent".to_string(),
        expected_identity: None,
        readonly: false,
        mode: 0o600,
        ownership: None,
        semantic_role_rank: 0,
    };
    let current = state_transition_current_member(&current_image, &event.event_id, None, "file");
    transition::bind_additional_members(&mut bundle, vec![backup, current])?;
    let journal = transition_guard.commit(&bundle)?;
    let mut port = StateReconcileTransactionPort {
        transition_guard,
        bundle: &bundle,
        current: &current_image,
        event: &event,
        journal: &journal,
        sink: writer.event_sink(&planned),
    };
    transaction_coordinator::execute_reconcile_transaction(&mut port)?;
    let backup = bundle
        .additional_members
        .first()
        .and_then(|member| {
            PathBuf::from(&member.path)
                .file_name()
                .map(|name| name.to_owned())
        })
        .map(|name| paths::state_dir().join(name))
        .ok_or_else(|| AppError::blocked("reconcile backup result path 불일치"))?;
    Ok((event, backup))
}

struct StateReconcileTransactionPort<'a> {
    transition_guard: &'a transition::TransitionGuard,
    bundle: &'a transition::PreparedSourceBundle,
    current: &'a PreparedCurrentImage,
    event: &'a ledger::LedgerEvent,
    journal: &'a std::path::Path,
    sink: ledger::EventSink<'a>,
}

impl ReconcileTransactionPort for StateReconcileTransactionPort<'_> {
    fn fault(&mut self, point: StateTransitionFault) -> Result<(), AppError> {
        match point {
            StateTransitionFault::Journal => state_transition_fault("after-journal"),
            StateTransitionFault::Artifacts => state_transition_fault("after-artifacts"),
            StateTransitionFault::Ledger => state_transition_fault("after-ledger"),
            StateTransitionFault::Current => state_transition_fault("after-current"),
            StateTransitionFault::Projection => state_transition_fault("after-projection"),
            StateTransitionFault::CheckpointTransaction
            | StateTransitionFault::CheckpointSnapshot
            | StateTransitionFault::CheckpointLedger
            | StateTransitionFault::CheckpointPointer => Err(AppError::blocked(
                "reconcile transaction checkpoint fault 범위 불일치",
            )),
        }
    }

    fn install_backup(&mut self) -> Result<(), AppError> {
        install_prepared_reconcile_backup(self.bundle)
    }

    fn append_event(&mut self) -> Result<(), AppError> {
        self.sink
            .append_planned_under_guard(0, self.event)
            .map(|_| ())
    }

    fn finish_events(&mut self) -> Result<(), AppError> {
        self.sink.finish()
    }

    fn install_current(&mut self) -> Result<(), AppError> {
        if !validate_state_transition_current_cas(self.bundle, &self.current.bytes)? {
            atomic_replace_bytes(&self.current.path, self.current.bytes.as_bytes())?;
        }
        Ok(())
    }

    fn converge(&mut self) -> Result<(), AppError> {
        self.sink.converge_derived(&self.bundle.project_id)
    }

    fn remove_journal(&mut self) -> Result<(), AppError> {
        self.transition_guard.remove(self.bundle, self.journal)
    }
}

fn install_current_image(
    prepared: &PreparedCurrentImage,
    before_revision: u64,
    before_artifact_hash: &str,
) -> Result<(), AppError> {
    if prepared.path != paths::current_state_file() {
        return Err(AppError::blocked(
            "prepared current image path binding 불일치",
        ));
    }
    if prepared.revision <= before_revision {
        return Err(AppError::blocked(
            "prepared current image revision CAS 불일치",
        ));
    }
    if validate_current_state_recovery_cas(
        before_revision,
        before_artifact_hash,
        Some(&prepared.bytes),
    )? {
        return Ok(());
    }
    atomic_replace_bytes(&prepared.path, prepared.bytes.as_bytes())
}

pub(crate) fn validate_current_state_recovery_cas(
    before_revision: u64,
    before_artifact_hash: &str,
    completed_bytes: Option<&str>,
) -> Result<bool, AppError> {
    let path = paths::current_state_file();
    let existing = fs::read_to_string(&path)
        .map_err(|err| AppError::blocked(format!("current-state recovery CAS 읽기 실패: {err}")))?;
    if completed_bytes == Some(existing.as_str()) {
        return Ok(true);
    }
    let current = parse_current_state(&existing, "current-state recovery CAS")?;
    if current.revision != before_revision || current.artifact_hash != before_artifact_hash {
        return Err(AppError::blocked(
            "prepared current-state exact CAS conflict",
        ));
    }
    Ok(false)
}

pub(crate) fn decode_prepared_current_image(
    member: &transition::PreparedMember,
    workflow: &WorkflowRecord,
    ledger_binding: &ledger::LedgerBinding,
    causal_id: &str,
    event_id: &str,
) -> Result<PreparedCurrentImage, AppError> {
    use transition::PreparedMemberKind;

    if member.kind != PreparedMemberKind::CurrentImage
        || member.schema_version != 2
        || member.path != "state/current-state.json"
        || member.expected_type != "file"
        || member.binding.causal_id.as_deref() != Some(causal_id)
        || member.binding.event_id.as_deref() != Some(event_id)
    {
        return Err(AppError::blocked(
            "prepared current image member binding 불일치",
        ));
    }
    let snapshot = parse_current_state(&member.bytes_utf8, "prepared current image member")?;
    let Some(active) = snapshot.active_workflow.as_ref() else {
        return Err(AppError::blocked(
            "prepared current image active workflow 누락",
        ));
    };
    if snapshot.schema_version != 2
        || snapshot.project_id != workflow.project_id
        || snapshot.session_id != workflow.session_id
        || active.workflow_id != workflow.workflow_id
        || active.revision != workflow.revision
        || active.artifact_hash != workflow.artifact_hash
        || snapshot.ledger_binding != *ledger_binding
        || snapshot.artifact_hash != sha256_text(&render_current_state_v2_payload(&snapshot))
        || render_current_state_v2(&snapshot) != member.bytes_utf8
        || member.binding.artifact_id.as_deref()
            != Some(format!("current-image-{}", snapshot.artifact_hash).as_str())
    {
        return Err(AppError::blocked(
            "prepared current image canonical/workflow/ledger binding 불일치",
        ));
    }
    Ok(PreparedCurrentImage {
        path: paths::current_state_file(),
        stored_path: member.path.clone(),
        bytes: member.bytes_utf8.clone(),
        artifact_id: format!("current-image-{}", snapshot.artifact_hash),
        revision: snapshot.revision,
    })
}

pub(crate) fn decode_prepared_terminal_current_image(
    member: &transition::PreparedMember,
    workflow: &WorkflowRecord,
    ledger_binding: &ledger::LedgerBinding,
    causal_id: &str,
    event_id: &str,
) -> Result<PreparedCurrentImage, AppError> {
    use transition::PreparedMemberKind;

    if member.kind != PreparedMemberKind::CurrentImage
        || member.schema_version != 2
        || member.path != "state/current-state.json"
        || member.expected_type != "file"
        || member.binding.causal_id.as_deref() != Some(causal_id)
        || member.binding.event_id.as_deref() != Some(event_id)
    {
        return Err(AppError::blocked(
            "prepared terminal current member binding 불일치",
        ));
    }
    let snapshot = parse_current_state(&member.bytes_utf8, "prepared terminal current member")?;
    if snapshot.schema_version != 2
        || snapshot.project_id != workflow.project_id
        || snapshot.session_id != workflow.session_id
        || snapshot.active_workflow.is_some()
        || snapshot.ledger_binding != *ledger_binding
        || snapshot.artifact_hash != sha256_text(&render_current_state_v2_payload(&snapshot))
        || render_current_state_v2(&snapshot) != member.bytes_utf8
        || member.binding.artifact_id.as_deref()
            != Some(format!("current-image-{}", snapshot.artifact_hash).as_str())
    {
        return Err(AppError::blocked(
            "prepared terminal current canonical/workflow/ledger binding 불일치",
        ));
    }
    Ok(PreparedCurrentImage {
        path: paths::current_state_file(),
        stored_path: member.path.clone(),
        bytes: member.bytes_utf8.clone(),
        artifact_id: format!("current-image-{}", snapshot.artifact_hash),
        revision: snapshot.revision,
    })
}

pub fn load_workflow(workflow_id: &str) -> Result<WorkflowRecord, AppError> {
    let identity = ledger::validated_current_identity()?;
    let _transition_guard = transition::TransitionGuard::acquire_for(
        &identity.project_id,
        transition::CurrentStateIntent::RecoverWorkflow,
    )?;
    load_workflow_under_transition(workflow_id)
}

pub(crate) fn load_workflow_revision(
    workflow_id: &str,
    revision: u64,
) -> Result<WorkflowRecord, AppError> {
    if revision == 0 {
        return Err(AppError::blocked(
            "workflow historical revision은 1 이상이어야 합니다.",
        ));
    }
    let identity = ledger::validated_current_identity()?;
    let _transition_guard = transition::TransitionGuard::acquire_for(
        &identity.project_id,
        transition::CurrentStateIntent::RecoverWorkflow,
    )?;
    let latest = load_workflow_under_transition(workflow_id)?;
    if revision > latest.revision {
        return Err(AppError::blocked("workflow historical revision 범위 오류"));
    }
    let path = paths::project_workflow_snapshot_file(workflow_id, revision);
    let body = read_regular_file_bounded(
        &path,
        MAX_WORKFLOW_SNAPSHOT_BYTES,
        "historical workflow snapshot",
    )?;
    let record = parse_workflow_snapshot(&path, &body)?;
    if record.workflow_id != workflow_id
        || record.revision != revision
        || record.project_id != identity.project_id
        || render_workflow(&record) != body
    {
        return Err(corrupt_workflow(&path));
    }
    Ok(record)
}

fn load_workflow_under_transition(workflow_id: &str) -> Result<WorkflowRecord, AppError> {
    validate_workflow_id(workflow_id)?;
    recover_workflow_transaction(workflow_id)?;
    let pointer_path = paths::project_workflow_file(workflow_id);
    let pointer = read_regular_file_bounded(
        &pointer_path,
        MAX_WORKFLOW_POINTER_BYTES,
        "committed workflow pointer",
    )?;
    let pointer = parse_workflow_pointer(&pointer_path, &pointer)?;
    if pointer.workflow_id != workflow_id || pointer.committed_revision == 0 {
        return Err(corrupt_workflow(&pointer_path));
    }
    let record = validate_workflow_chain(
        workflow_id,
        pointer.committed_revision,
        pointer.schema_version,
    )?;
    let identity = ledger::validated_current_identity()?;
    if record.artifact_hash != pointer.artifact_hash || record.project_id != identity.project_id {
        return Err(corrupt_workflow(&pointer_path));
    }
    Ok(record)
}

pub fn active_workflow_id() -> Result<Option<String>, AppError> {
    let identity = ledger::validated_current_identity()?;
    let transition_guard = transition::TransitionGuard::acquire_for(
        &identity.project_id,
        transition::CurrentStateIntent::RepairWorkflowPointer,
    )?;
    active_workflow_id_under_transition(&transition_guard)
}

fn active_workflow_id_under_transition(
    transition_guard: &transition::TransitionGuard,
) -> Result<Option<String>, AppError> {
    let discovered = discover_active_workflow()?;
    let path = paths::current_state_file();
    if !path.exists() {
        if let Some(workflow_id) = discovered.as_deref() {
            let workflow = load_workflow_under_transition(workflow_id)?;
            let identity = workflow_identity(&workflow);
            let event = ledger::new_event_for(
                &identity,
                "workflow.pointer.recovered",
                "active workflow pointer recovered",
                &format!("workflow_id={workflow_id} predecessor=missing"),
            );
            let intent_id = internal_transition_intent_id(&event);
            transition_project_current_state_under_guard(
                transition_guard,
                StateTransitionRequest {
                    intent_id: &intent_id,
                    intent: transition::CurrentStateIntent::RepairWorkflowPointer,
                    identity: &identity,
                    event: &event,
                    resume_source: Some("workflow-pointer-recovery"),
                    active_workflow: Some(&workflow),
                    previous: None,
                    workflow: None,
                },
            )?;
        }
        return Ok(discovered);
    }
    let body = fs::read_to_string(&path).map_err(|err| {
        AppError::runtime(format!(
            "current-state를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    let snapshot = parse_current_state(&body, "current-state")?;
    if snapshot.project_id != ledger::fresh_identity().project_id {
        return Err(AppError::blocked(
            "current-state schema/project binding이 손상되었습니다.",
        ));
    }
    let pointer = snapshot
        .active_workflow
        .as_ref()
        .map(|workflow| workflow.workflow_id.clone());
    match (pointer, discovered) {
        (None, None) => Ok(None),
        (None, Some(workflow_id)) => {
            let workflow = load_workflow_under_transition(&workflow_id)?;
            let identity = workflow_identity(&workflow);
            let event = ledger::new_event_for(
                &identity,
                "workflow.pointer.recovered",
                "active workflow pointer recovered",
                &format!("workflow_id={workflow_id} predecessor=null"),
            );
            let intent_id = internal_transition_intent_id(&event);
            transition_project_current_state_under_guard(
                transition_guard,
                StateTransitionRequest {
                    intent_id: &intent_id,
                    intent: transition::CurrentStateIntent::RepairWorkflowPointer,
                    identity: &identity,
                    event: &event,
                    resume_source: Some("workflow-pointer-recovery"),
                    active_workflow: Some(&workflow),
                    previous: Some(&snapshot),
                    workflow: None,
                },
            )?;
            Ok(Some(workflow_id))
        }
        (Some(pointer), Some(workflow_id)) if pointer == workflow_id => Ok(Some(workflow_id)),
        (Some(pointer), None) => {
            let workflow = load_workflow_under_transition(&pointer)?;
            if !workflow.is_terminal() {
                return Err(AppError::blocked(
                    "workflow resume 차단\n- 이유: current pointer와 전체 artifact scan이 충돌합니다.",
                ));
            }
            Ok(Some(pointer))
        }
        _ => Err(AppError::blocked(
            "workflow resume 차단\n- 이유: current pointer와 non-terminal artifact가 충돌합니다.\n- 동작: fail-closed; backend와 side effect를 실행하지 않습니다.",
        )),
    }
}

pub(crate) fn clear_terminal_workflow_pointer(workflow: &WorkflowRecord) -> Result<(), AppError> {
    let transition_guard = transition::TransitionGuard::acquire_for(
        &workflow.project_id,
        transition::CurrentStateIntent::ClearTerminalWorkflow,
    )?;
    clear_terminal_workflow_pointer_under_transition(&transition_guard, workflow)
}

pub(crate) fn clear_terminal_workflow_pointer_under_transition(
    transition_guard: &transition::TransitionGuard,
    workflow: &WorkflowRecord,
) -> Result<(), AppError> {
    if !workflow.is_terminal() {
        return Err(AppError::blocked(
            "terminal workflow pointer cleanup 차단: workflow가 terminal이 아닙니다.",
        ));
    }
    let path = paths::current_state_file();
    let body = fs::read_to_string(&path).map_err(|err| {
        AppError::blocked(format!("terminal pointer current-state 읽기 실패: {err}"))
    })?;
    let snapshot = parse_current_state(&body, "current-state")?;
    match snapshot.active_workflow.as_ref() {
        Some(binding) if binding.workflow_id == workflow.workflow_id => {}
        None => return Ok(()),
        _ => {
            return Err(AppError::blocked(
                "terminal workflow pointer cleanup 차단: current pointer conflict",
            ))
        }
    }
    let identity = workflow_identity(workflow);
    let event = ledger::new_event_for(
        &identity,
        "workflow.pointer.cleared",
        "terminal workflow pointer cleared",
        &format!(
            "workflow_id={} revision={} artifact_hash={}",
            workflow.workflow_id, workflow.revision, workflow.artifact_hash
        ),
    );
    let intent_id = internal_transition_intent_id(&event);
    transition_project_current_state_under_guard(
        transition_guard,
        StateTransitionRequest {
            intent_id: &intent_id,
            intent: transition::CurrentStateIntent::ClearTerminalWorkflow,
            identity: &identity,
            event: &event,
            resume_source: Some("terminal-pointer-cleanup"),
            active_workflow: None,
            previous: Some(&snapshot),
            workflow: None,
        },
    )
    .map(|_| ())
}

pub(crate) fn record_tui_workflow_resume_receipt_under_transition(
    transition_guard: &transition::TransitionGuard,
    workflow: &WorkflowRecord,
    intent_id: &str,
    active_workflow: Option<&WorkflowRecord>,
) -> Result<String, AppError> {
    let previous = read_valid_current_for_transition()?
        .ok_or_else(|| AppError::blocked("workflow resume current-state 누락"))?;
    if previous.project_id != workflow.project_id
        || previous.session_id != workflow.session_id
        || active_workflow.is_some_and(|active| active.workflow_id != workflow.workflow_id)
    {
        return Err(AppError::blocked(
            "workflow resume receipt current/workflow binding 불일치",
        ));
    }
    let identity = workflow_identity(workflow);
    let event = ledger::new_event_for(
        &identity,
        "workflow.resume.accepted",
        "TUI workflow resume accepted",
        &format!("intent_id={intent_id} workflow_id={}", workflow.workflow_id),
    );
    transition_project_current_state_under_guard(
        transition_guard,
        StateTransitionRequest {
            intent_id,
            intent: transition::CurrentStateIntent::Resume,
            identity: &identity,
            event: &event,
            resume_source: Some("interactive-tui"),
            active_workflow,
            previous: Some(&previous),
            workflow: None,
        },
    )?;
    Ok(event.event_id)
}

pub(crate) fn record_workflow_event_under_transition(
    transition_guard: &transition::TransitionGuard,
    workflow: &WorkflowRecord,
    event_type: &str,
    summary: &str,
    details: &str,
) -> Result<String, AppError> {
    let previous = read_valid_current_for_transition()?
        .ok_or_else(|| AppError::blocked("workflow event current-state 누락"))?;
    let Some(active) = previous.active_workflow.as_ref() else {
        return Err(AppError::blocked(
            "workflow event current/workflow binding 누락",
        ));
    };
    if previous.project_id != workflow.project_id
        || previous.session_id != workflow.session_id
        || active.workflow_id != workflow.workflow_id
        || active.revision != workflow.revision
        || active.artifact_hash != workflow.artifact_hash
    {
        return Err(AppError::blocked(
            "workflow event current/workflow binding 불일치",
        ));
    }
    let identity = workflow_identity(workflow);
    let event = ledger::new_event_for(&identity, event_type, summary, details);
    let intent_id = internal_transition_intent_id(&event);
    transition_project_current_state_under_guard(
        transition_guard,
        StateTransitionRequest {
            intent_id: &intent_id,
            intent: transition::CurrentStateIntent::RecordEvent,
            identity: &identity,
            event: &event,
            resume_source: Some("workflow-event-recovery"),
            active_workflow: Some(workflow),
            previous: Some(&previous),
            workflow: None,
        },
    )?;
    Ok(event.event_id)
}

fn discover_active_workflow() -> Result<Option<String>, AppError> {
    let entries = match fs::read_dir(paths::project_workflows_dir()) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(AppError::runtime(format!(
                "workflow directory를 읽지 못했습니다: {err}"
            )))
        }
    };
    let mut workflow_ids = std::collections::BTreeSet::new();
    for entry in entries {
        let path = entry
            .map_err(|err| AppError::runtime(format!("workflow entry read 실패: {err}")))?
            .path();
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| corrupt_workflow(&path))?;
        let workflow_id = name
            .strip_suffix(".json")
            .or_else(|| name.strip_suffix(".txn"))
            .or_else(|| name.strip_suffix(".snapshots"));
        let Some(workflow_id) = workflow_id else {
            continue;
        };
        validate_workflow_id(workflow_id)?;
        workflow_ids.insert(workflow_id.to_string());
    }
    let mut active = Vec::new();
    for workflow_id in workflow_ids {
        if !paths::project_workflow_file(&workflow_id).exists()
            && !paths::project_workflow_transaction_file(&workflow_id).exists()
        {
            return Err(AppError::blocked(format!(
                "workflow scan 차단\n- 이유: committed pointer와 transaction이 없는 snapshot artifact\n- workflow id: {workflow_id}"
            )));
        }
        let workflow = load_workflow_under_transition(&workflow_id)?;
        if !workflow.is_terminal() {
            active.push(workflow.workflow_id);
        }
    }
    match active.as_slice() {
        [] => Ok(None),
        [workflow_id] => Ok(Some(workflow_id.clone())),
        _ => Err(AppError::blocked(
            "workflow resume 차단\n- 이유: 여러 non-terminal canonical workflow가 충돌합니다.\n- 동작: fail-closed; backend와 side effect를 실행하지 않습니다.",
        )),
    }
}

pub fn sha256_text(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateInit {
    pub identity: RuntimeIdentity,
    pub created_paths: Vec<PathBuf>,
    pub store: StoreStatus,
}

pub fn initialize() -> Result<StateInit, AppError> {
    let identity = ledger::validated_current_identity()?;
    let created_paths = ensure_layout()?;
    ensure_runtime_evidence_file()?;
    if !paths::current_state_file().exists() {
        let event = ledger::new_event_for(
            &identity,
            "runtime.init",
            "runtime state 초기화",
            "app/project state layout 생성 또는 확인",
        );
        let intent_id = internal_transition_intent_id(&event);
        commit_state_event(
            &intent_id,
            transition::CurrentStateIntent::Bootstrap,
            &identity,
            &event,
            None,
            None,
        )?;
    }

    let store = observability::initialize(&identity)?;

    Ok(StateInit {
        identity,
        created_paths,
        store,
    })
}

pub fn status_report() -> Result<String, AppError> {
    let active = active_workflow_id()?.unwrap_or_else(|| "없음".to_string());
    let current_state = read_current_state_summary()?;
    let store = observability::status()?;
    let recovered = store
        .recovered_from
        .as_ref()
        .map(|path| format!("\n- recovered corrupt db: {}", path.display()))
        .unwrap_or_default();

    Ok(format!(
        "state 상태\n- app state dir: {}\n- project state dir: {}\n- runtime ledger: {}\n- project session ledger: {}\n- current state: {}\n- observability db: {}\n- schema migration: v{}\n- ledger events: {}\n- sessions: {}\n- workflows: {}\n- transcript records: {}\n- active workflow: {}\n- transcript parent/branch pointer: current-state schema에 null로 보존\n- evidence stale policy: {}{}",
        paths::state_dir().display(),
        paths::project_state_dir().display(),
        paths::runtime_ledger_file().display(),
        paths::project_session_ledger_file().display(),
        current_state,
        store.path.display(),
        store.migration_version,
        store.ledger_events,
        store.sessions,
        store.workflows,
        store.transcript_records,
        active,
        crate::evidence::stale_policy_summary(),
        recovered
    ))
}

pub fn reconcile_report() -> Result<String, AppError> {
    ensure_layout()?;
    let identity = match ledger::validated_current_identity() {
        Ok(identity) => identity,
        Err(_) => ledger::fresh_identity(),
    };
    let transition_guard = transition::TransitionGuard::acquire_for(
        &identity.project_id,
        transition::CurrentStateIntent::Reconcile,
    )?;
    let status = current_state_status(&identity)?;
    let (outcome, event_id) = match status {
        CurrentStateStatus::CleanNoActiveWorkflow | CurrentStateStatus::CleanActiveWorkflow => {
            (ReconcileOutcome::Clean, "없음".to_string())
        }
        CurrentStateStatus::Missing => {
            let event = ledger::new_event_for(
                &identity,
                "state.reconcile.created",
                "current-state 생성",
                "current-state reconcile 완료",
            );
            let intent_id = internal_transition_intent_id(&event);
            transition_project_current_state_under_guard(
                &transition_guard,
                StateTransitionRequest {
                    intent_id: &intent_id,
                    intent: transition::CurrentStateIntent::Reconcile,
                    identity: &identity,
                    event: &event,
                    resume_source: Some("state-reconcile"),
                    active_workflow: None,
                    previous: None,
                    workflow: None,
                },
            )?;
            (ReconcileOutcome::Created, event.event_id)
        }
        CurrentStateStatus::Corrupt | CurrentStateStatus::StaleProject => {
            let before = fs::read_to_string(paths::current_state_file()).map_err(|err| {
                AppError::blocked(format!(
                    "reconcile preserved current-state 읽기 실패: {err}"
                ))
            })?;
            let reason = if status == CurrentStateStatus::Corrupt {
                "corrupt"
            } else {
                "stale"
            };
            let (event, backup) = reconcile_invalid_current_under_guard(
                &transition_guard,
                &identity,
                reason,
                &before,
            )?;
            let outcome = if reason == "corrupt" {
                ReconcileOutcome::RecoveredCorrupt(backup)
            } else {
                ReconcileOutcome::RecoveredStale(backup)
            };
            (outcome, event.event_id)
        }
    };
    let summary = outcome.summary();
    observability::initialize(&identity)?;

    Ok(format!(
        "state reconcile 결과\n- outcome: {}\n- current state: {}\n- ledger event: {}\n- 동작: stale/corrupt current-state를 발견하면 기존 파일을 보존 이동하고 새 current-state를 기록합니다.",
        summary,
        paths::current_state_file().display(),
        event_id
    ))
}

pub fn resume_report() -> Result<String, AppError> {
    ensure_layout()?;
    if let Some(workflow_id) = active_workflow_id()? {
        return crate::patch::resume_workflow_report(&workflow_id);
    }
    let identity = ledger::validated_current_identity()?;
    observability::initialize(&identity)?;
    let status = current_state_status(&identity)?;
    let (event_type, summary, action) = match status {
        CurrentStateStatus::CleanNoActiveWorkflow => (
            "workflow.resume.noop",
            "active workflow 없는 resume 요청",
            "재개할 workflow가 없어 no-op event만 기록했습니다.",
        ),
        CurrentStateStatus::CleanActiveWorkflow => (
            "workflow.resume.detected",
            "resume 대상 감지",
            "active workflow pointer를 발견했습니다. agent loop resume은 후속 phase에서 실행됩니다.",
        ),
        CurrentStateStatus::Missing => (
            "workflow.resume.blocked",
            "current-state 누락으로 resume 차단",
            "current-state가 없어 먼저 state reconcile이 필요합니다.",
        ),
        CurrentStateStatus::Corrupt => (
            "workflow.resume.blocked",
            "current-state 손상으로 resume 차단",
            "current-state가 손상되어 먼저 state reconcile이 필요합니다.",
        ),
        CurrentStateStatus::StaleProject => (
            "workflow.resume.blocked",
            "다른 project current-state로 resume 차단",
            "current-state project id가 현재 project와 달라 먼저 state reconcile이 필요합니다.",
        ),
    };

    let event = ledger::new_event_for(&identity, event_type, summary, action);
    let intent_id = internal_transition_intent_id(&event);
    commit_state_event(
        &intent_id,
        transition::CurrentStateIntent::Resume,
        &identity,
        &event,
        None,
        None,
    )?;

    Ok(format!(
        "state resume 결과\n- outcome: {}\n- ledger event: {}\n- 동작: {}",
        summary, event.event_id, action
    ))
}

pub fn cancel_report() -> Result<String, AppError> {
    ensure_layout()?;
    if let Some(workflow_id) = active_workflow_id()? {
        return crate::patch::cancel_workflow_report(&workflow_id);
    }
    let identity = ledger::validated_current_identity()?;
    observability::initialize(&identity)?;
    let event = ledger::new_event_for(
        &identity,
        "workflow.cancel.noop",
        "active workflow 없는 cancel 요청",
        "active_workflow=null",
    );
    let intent_id = internal_transition_intent_id(&event);
    commit_state_event(
        &intent_id,
        transition::CurrentStateIntent::Cancel,
        &identity,
        &event,
        None,
        None,
    )?;

    Ok(format!(
        "cancel 결과\n- active workflow: 없음\n- ledger event: {}\n- ledger: {}\n- 동작: 취소할 실행이 없어 no-op event만 기록했습니다.",
        event.event_id,
        paths::runtime_ledger_file().display()
    ))
}

pub fn session_list_report() -> Result<String, AppError> {
    let identity = ledger::validated_current_identity()?;
    ensure_layout()?;
    let sessions = observability::session_history(20)?;
    if sessions.is_empty() {
        return Ok(format!(
            "session history\n- project: {}\n- sessions: 없음\n- 다음 단계: `rpotato init` 또는 `rpotato session new`로 세션을 시작하세요.",
            identity.project_root
        ));
    }

    let rows = sessions
        .iter()
        .map(format_session_row)
        .collect::<Vec<_>>()
        .join("\n");

    Ok(format!(
        "session history\n- project: {}\n- current session: {}\n- resume: `rpotato session resume <session-id>` 또는 `rpotato resume <session-id>`\n{}",
        identity.project_root, identity.session_id, rows
    ))
}

pub fn session_new_report() -> Result<String, AppError> {
    session_new_report_for_intent(&new_tui_intent_id())
}

pub(crate) fn session_new_report_for_intent(intent_id: &str) -> Result<String, AppError> {
    if !intent_id.starts_with("intent-")
        || !intent_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
    {
        return Err(AppError::blocked("session new intent id 형식 불일치"));
    }
    ensure_layout()?;
    let current_identity = ledger::validated_current_identity()?;
    let observed = read_valid_current_for_transition()?;
    ensure_runtime_evidence_file()?;
    let transition_guard = transition::TransitionGuard::acquire_for(
        &current_identity.project_id,
        transition::CurrentStateIntent::StartSession,
    )?;
    if let Some(existing) = ledger::read_runtime_events()?.into_iter().find(|event| {
        event.event_type == "session.new"
            && tui_detail_value(&event.details, "intent_id") == Some(intent_id)
    }) {
        return Ok(session_new_success_report(
            &existing.session_id,
            &existing.event_id,
        ));
    }
    let previous = read_valid_current_for_transition()?;
    let same_predecessor = match (&observed, &previous) {
        (None, None) => true,
        (Some(observed), Some(previous)) => {
            previous.revision == observed.revision
                && previous.artifact_hash == observed.artifact_hash
                && previous.session_id == observed.session_id
        }
        _ => false,
    };
    if !same_predecessor {
        return Err(AppError::blocked(
            "session new stale predecessor 차단: current-state가 선택 이후 변경되었습니다.",
        ));
    }
    let identity = RuntimeIdentity {
        project_id: current_identity.project_id,
        session_id: format!(
            "session-{}",
            &sha256_text(&format!("rpotato.session-new/v1\0{intent_id}"))[..24]
        ),
        project_root: current_identity.project_root,
    };
    let event = ledger::new_event_for(
        &identity,
        "session.new",
        "새 session 시작",
        &format!(
            "intent_id={intent_id} predecessor_revision={} predecessor_hash={}",
            previous.as_ref().map_or(0, |snapshot| snapshot.revision),
            previous
                .as_ref()
                .map_or("missing", |snapshot| snapshot.artifact_hash.as_str())
        ),
    );
    transition_project_current_state_under_guard(
        &transition_guard,
        StateTransitionRequest {
            intent_id,
            intent: transition::CurrentStateIntent::StartSession,
            identity: &identity,
            event: &event,
            resume_source: None,
            active_workflow: None,
            previous: previous.as_ref(),
            workflow: None,
        },
    )?;
    observability::initialize(&identity)?;

    Ok(session_new_success_report(
        &identity.session_id,
        &event.event_id,
    ))
}

fn session_new_success_report(session_id: &str, event_id: &str) -> String {
    format!(
        "session new 결과\n- session id: {}\n- current state: {}\n- ledger event: {}\n- 동작: 이후 명령은 이 session id로 ledger와 SQLite projection에 이어 기록됩니다.",
        session_id,
        paths::current_state_file().display(),
        event_id
    )
}

pub fn session_resume_preflight(session_id: &str) -> Result<Option<String>, AppError> {
    ensure_layout()?;
    let identity = ledger::validated_current_identity()?;
    let _transition_guard = transition::TransitionGuard::acquire_for(
        &identity.project_id,
        transition::CurrentStateIntent::SelectSession,
    )?;
    session_resume_preflight_under_transition(session_id, &identity)
}

fn session_resume_preflight_under_transition(
    session_id: &str,
    identity: &RuntimeIdentity,
) -> Result<Option<String>, AppError> {
    let canonical_session = ledger::read_runtime_events()?
        .into_iter()
        .any(|event| event.project_id == identity.project_id && event.session_id == session_id);
    if !canonical_session {
        return snapshot_domain::validate_session_resume_target(session_id, false, false, None);
    }
    let projected_session = observability::session_entry(session_id)?.is_some();
    if !projected_session {
        return snapshot_domain::validate_session_resume_target(session_id, true, false, None);
    }
    let active_workflow = discover_active_workflow()?
        .map(|workflow_id| load_workflow_under_transition(&workflow_id))
        .transpose()?;
    snapshot_domain::validate_session_resume_target(
        session_id,
        canonical_session,
        projected_session,
        active_workflow.as_ref(),
    )
}

pub fn session_resume_report(session_id: &str) -> Result<String, AppError> {
    session_resume_report_with_precondition(session_id, None, None)?
        .ok_or_else(|| AppError::blocked("internal session resume precondition unexpectedly stale"))
}

pub(crate) fn session_resume_report_for_tui(
    session_id: &str,
    intent_id: &str,
    lease: &SelectionLease,
) -> Result<Option<String>, AppError> {
    session_resume_report_with_precondition(session_id, Some(intent_id), Some(lease))
}

fn session_resume_report_with_precondition(
    session_id: &str,
    supplied_intent_id: Option<&str>,
    lease: Option<&SelectionLease>,
) -> Result<Option<String>, AppError> {
    let project_id = match lease {
        Some(lease) => lease.project_id.clone(),
        None => ledger::validated_current_identity()?.project_id,
    };
    let transition_guard = transition::TransitionGuard::acquire_for(
        &project_id,
        transition::CurrentStateIntent::SelectSession,
    )?;
    let identity = ledger::validated_current_identity()?;
    if let Some(intent_id) = supplied_intent_id {
        if let Some(event_id) = existing_session_selection_receipt(intent_id, session_id)? {
            let session = observability::session_entry(session_id)?
                .ok_or_else(|| AppError::blocked("committed session selection projection 누락"))?;
            return Ok(Some(render_session_resume_report(&session, &event_id)));
        }
    }
    if let Some(lease) = lease {
        if !selection_lease_matches_under_transition(session_id, lease, &identity)? {
            return Ok(None);
        }
    }
    session_resume_preflight_under_transition(session_id, &identity)?;
    let Some(session) = observability::session_entry(session_id)? else {
        return Err(AppError::blocked(format!(
            "session resume 차단\n- session id: {}\n- 이유: session projection을 찾지 못했습니다.",
            session_id
        )));
    };
    let active_workflow = discover_active_workflow()?
        .map(|workflow_id| load_workflow_under_transition(&workflow_id))
        .transpose()?;

    let resumed = RuntimeIdentity {
        project_id: identity.project_id,
        session_id: session.session_id.clone(),
        project_root: identity.project_root,
    };
    let event = ledger::new_event_for(
        &resumed,
        "session.resume.selected",
        "session history에서 resume target 선택",
        &format!(
            "selected_session_id={} intent_id={}",
            session.session_id,
            supplied_intent_id.unwrap_or("internal")
        ),
    );
    let intent_id = supplied_intent_id
        .map(str::to_string)
        .unwrap_or_else(|| internal_transition_intent_id(&event));
    let previous = read_valid_current_for_transition()?
        .ok_or_else(|| AppError::blocked("session resume current-state 누락"))?;
    transition_project_current_state_under_guard(
        &transition_guard,
        StateTransitionRequest {
            intent_id: &intent_id,
            intent: transition::CurrentStateIntent::SelectSession,
            identity: &resumed,
            event: &event,
            resume_source: Some("session-history"),
            active_workflow: active_workflow.as_ref(),
            previous: Some(&previous),
            workflow: None,
        },
    )?;
    let committed_session = observability::session_entry(session_id)?
        .ok_or_else(|| AppError::blocked("committed session selection projection 누락"))?;

    Ok(Some(render_session_resume_report(
        &committed_session,
        &event.event_id,
    )))
}

fn existing_session_selection_receipt(
    intent_id: &str,
    session_id: &str,
) -> Result<Option<String>, AppError> {
    let intent_marker = format!("intent_id={intent_id}");
    let selected_marker = format!("selected_session_id={session_id}");
    let mut matching_intent = None;
    for event in ledger::read_runtime_events()?
        .into_iter()
        .filter(|event| event.event_type == "session.resume.selected")
    {
        let fields = event.details.split_ascii_whitespace().collect::<Vec<_>>();
        if fields.contains(&intent_marker.as_str()) {
            if !fields.contains(&selected_marker.as_str()) || matching_intent.is_some() {
                return Err(AppError::blocked(
                    "session selection intent receipt binding 충돌",
                ));
            }
            matching_intent = Some(event.event_id);
        }
    }
    Ok(matching_intent)
}

fn selection_lease_matches_under_transition(
    session_id: &str,
    lease: &SelectionLease,
    identity: &RuntimeIdentity,
) -> Result<bool, AppError> {
    let Some(current) = read_valid_current_for_transition()? else {
        return Ok(false);
    };
    if lease.project_id != identity.project_id
        || lease.project_id != current.project_id
        || lease.session_id != current.session_id
        || lease.active_session_id != current.session_id
        || lease.selected_object_id != session_id
        || lease.current_revision != current.revision
        || lease.current_hash != current.artifact_hash
    {
        return Ok(false);
    }
    let observed = current
        .active_workflow
        .as_ref()
        .map(|binding| ObservedWorkflow {
            workflow_id: binding.workflow_id.clone(),
            revision: binding.revision,
            hash: binding.artifact_hash.clone(),
        });
    if observed != lease.active_workflow {
        return Ok(false);
    }
    if let Some(binding) = current.active_workflow {
        let workflow = load_workflow_under_transition(&binding.workflow_id)?;
        if workflow.revision != binding.revision || workflow.artifact_hash != binding.artifact_hash
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn render_session_resume_report(session: &SessionHistoryEntry, event_id: &str) -> String {
    format!(
        "session resume 결과\n- selected session: {}\n- events: {}\n- last event: {}\n- current state: {}\n- ledger event: {}\n- 동작: 선택한 session id를 기록했습니다. Runtime wrapper는 검증된 같은-session workflow checkpoint만 계속하며 새 model turn은 자동 생성하지 않습니다.",
        session.session_id,
        session.event_count,
        session
            .last_summary
            .clone()
            .unwrap_or_else(|| "없음".to_string()),
        paths::current_state_file().display(),
        event_id
    )
}

pub fn record_event(event_type: &str, summary: &str, details: &str) -> Result<String, AppError> {
    ensure_layout()?;
    if !paths::current_state_file().exists() {
        initialize()?;
    }
    let identity = ledger::validated_current_identity()?;
    let event = ledger::new_event_for(&identity, event_type, summary, details);
    let event_id = event.event_id.clone();
    let active_workflow = read_valid_current_for_transition()?
        .and_then(|snapshot| snapshot.active_workflow)
        .map(|binding| binding.workflow_id);
    let intent_id = internal_transition_intent_id(&event);
    commit_state_event(
        &intent_id,
        transition::CurrentStateIntent::RecordEvent,
        &identity,
        &event,
        None,
        active_workflow.as_deref(),
    )?;
    Ok(event_id)
}

pub fn workflow_ownership_summary() -> &'static str {
    "active workflow는 current-state가 소유하고 skill/plugin/TUI는 parent workflow pointer를 받아야 합니다."
}

fn ensure_layout() -> Result<Vec<PathBuf>, AppError> {
    let directories = [
        paths::config_dir(),
        paths::backends_dir(),
        paths::models_dir(),
        paths::model_registry_dir(),
        paths::downloads_dir(),
        paths::manifests_dir(),
        paths::logs_dir(),
        paths::state_dir(),
        paths::plugins_dir(),
        paths::imported_plugins_dir(),
        paths::plugin_data_dir(),
        paths::cache_dir(),
        paths::project_state_dir(),
        paths::project_evidence_dir(),
        paths::project_approval_requests_dir(),
        paths::project_workflows_dir(),
    ];

    let mut created = Vec::new();
    for directory in directories {
        if !directory.exists() {
            created.push(directory.clone());
        }
        fs::create_dir_all(&directory).map_err(|err| {
            AppError::runtime(format!(
                "state 디렉터리를 만들지 못했습니다: {} ({err})",
                directory.display()
            ))
        })?;
    }

    Ok(created)
}

fn parse_current_state(body: &str, context: &str) -> Result<CurrentStateSnapshot, AppError> {
    let value = strict_json::parse_value(body, context)?;
    let strict_json::Value::Object(root) = &value else {
        return Err(AppError::blocked(format!(
            "{context} 차단\n- 이유: root must be object"
        )));
    };
    let schema = strict_json::number(root, "schema_version", context)?;
    match schema {
        1 => parse_current_state_v1(body, value, context),
        2 => parse_current_state_v2(body, context),
        _ => Err(AppError::blocked(format!(
            "{context} 차단\n- 이유: unsupported schema version"
        ))),
    }
}

pub(crate) fn validated_identity_from_current_state(
    body: &str,
    fresh: &RuntimeIdentity,
) -> Result<RuntimeIdentity, AppError> {
    let snapshot = parse_current_state(body, "current-state identity")?;
    if snapshot.project_id != fresh.project_id || snapshot.project_root != fresh.project_root {
        return Err(AppError::blocked(
            "current-state identity project binding 불일치",
        ));
    }
    Ok(RuntimeIdentity {
        project_id: snapshot.project_id,
        session_id: snapshot.session_id,
        project_root: snapshot.project_root,
    })
}

pub(crate) fn current_state_lease_view() -> Result<CurrentStateLeaseView, AppError> {
    let identity = ledger::validated_current_identity()?;
    let _transition_guard = transition::TransitionGuard::acquire_for(
        &identity.project_id,
        transition::CurrentStateIntent::RecoverWorkflow,
    )?;
    current_state_lease_view_under_transition()
}

pub(crate) fn tui_state_snapshot_read_only(
    max_ledger_events: usize,
) -> Result<TuiStateSnapshot, AppError> {
    with_validation_gap_writes_suppressed(|| {
        let path = paths::current_state_file();
        let body = read_regular_file_bounded(&path, 128 * 1024, "TUI current-state")?;
        let snapshot = parse_current_state(&body, "TUI current-state read-only")?;
        if snapshot.schema_version != 2 {
            return Err(AppError::blocked(
                "TUI read-only current-state는 schema v2 canonical image가 필요합니다.",
            ));
        }
        let fresh = ledger::fresh_identity();
        let identity = snapshot_domain::validated_tui_identity(&snapshot, &fresh)?;
        let ledger_tail =
            ledger::read_runtime_tail_read_only(max_ledger_events.max(1), 2 * 1024 * 1024)?;
        let current_ledger_binding_stale = snapshot.ledger_binding != ledger_tail.binding;
        snapshot_domain::validate_ledger_ancestor(
            &snapshot.ledger_binding,
            &ledger_tail.binding,
            &ledger_tail.events,
        )?;
        let active_workflow = snapshot
            .active_workflow
            .as_ref()
            .map(|binding| load_workflow_read_only(binding, &identity, &ledger_tail.events))
            .transpose()?;
        Ok(TuiStateSnapshot {
            identity,
            current_revision: snapshot.revision,
            current_hash: snapshot.artifact_hash,
            ledger_binding: ledger_tail.binding,
            ledger_events: ledger_tail.events,
            active_workflow,
            ledger_tail_truncated: ledger_tail.truncated,
            current_ledger_binding_stale,
        })
    })
}

fn load_workflow_read_only(
    binding: &CurrentWorkflowBinding,
    identity: &RuntimeIdentity,
    ledger_events: &[ledger::ParsedLedgerEvent],
) -> Result<WorkflowRecord, AppError> {
    validate_workflow_id(&binding.workflow_id)?;
    let transaction = paths::project_workflow_transaction_file(&binding.workflow_id);
    if transaction.exists() {
        return Err(AppError::blocked(
            "TUI workflow read-only view는 pending recovery transaction을 실행하지 않습니다.",
        ));
    }
    let pointer_path = paths::project_workflow_file(&binding.workflow_id);
    let pointer_body = read_regular_file_bounded(&pointer_path, 64 * 1024, "TUI workflow pointer")?;
    let pointer = parse_workflow_pointer(&pointer_path, &pointer_body)?;
    snapshot_domain::validate_read_only_pointer(binding, &pointer)?;
    let snapshot_path =
        paths::project_workflow_snapshot_file(&binding.workflow_id, binding.revision);
    let snapshot_body =
        read_regular_file_bounded(&snapshot_path, 512 * 1024, "TUI workflow snapshot")?;
    if workflow_snapshot_schema(&snapshot_path, &snapshot_body)? != pointer.schema_version {
        return Err(AppError::blocked(
            "TUI workflow pointer/snapshot schema binding 불일치",
        ));
    }
    let workflow = parse_workflow_snapshot(&snapshot_path, &snapshot_body)?;
    snapshot_domain::validate_read_only_workflow(binding, identity, &workflow, ledger_events)?;
    Ok(workflow)
}

pub(crate) fn read_regular_file_bounded(
    path: &std::path::Path,
    max_bytes: u64,
    label: &str,
) -> Result<String, AppError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|err| AppError::blocked(format!("{label} metadata 실패: {err}")))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > max_bytes {
        return Err(AppError::blocked(format!(
            "{label} regular-file/byte budget 불일치"
        )));
    }
    let mut file =
        File::open(path).map_err(|err| AppError::blocked(format!("{label} 열기 실패: {err}")))?;
    validate_open_read_identity(path, &file, label)?;
    let bytes = read_open_file_bounded(&mut file, max_bytes, label)?;
    validate_open_read_identity(path, &file, label)?;
    String::from_utf8(bytes).map_err(|_| AppError::blocked(format!("{label} UTF-8 불일치")))
}

fn read_open_file_bounded(
    file: &mut File,
    max_bytes: u64,
    label: &str,
) -> Result<Vec<u8>, AppError> {
    let metadata = file
        .metadata()
        .map_err(|err| AppError::blocked(format!("{label} handle metadata 실패: {err}")))?;
    if !metadata.is_file() || metadata.len() > max_bytes {
        return Err(AppError::blocked(format!(
            "{label} regular-file/byte budget 불일치"
        )));
    }
    let mut bytes = Vec::with_capacity(
        usize::try_from(metadata.len())
            .unwrap_or(usize::MAX)
            .min(usize::try_from(max_bytes).unwrap_or(usize::MAX)),
    );
    Read::by_ref(file)
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|err| AppError::blocked(format!("{label} 읽기 실패: {err}")))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max_bytes {
        return Err(AppError::blocked(format!(
            "{label} byte budget 초과; 증거를 보존했습니다."
        )));
    }
    let after = file
        .metadata()
        .map_err(|err| AppError::blocked(format!("{label} handle 재검증 실패: {err}")))?;
    if !after.is_file() || after.len() > max_bytes {
        return Err(AppError::blocked(format!(
            "{label} read 중 byte budget 변경; 증거를 보존했습니다."
        )));
    }
    Ok(bytes)
}

#[cfg(unix)]
fn validate_open_read_identity(
    path: &std::path::Path,
    file: &File,
    label: &str,
) -> Result<(), AppError> {
    use std::os::unix::fs::MetadataExt;

    let path_metadata = fs::symlink_metadata(path)
        .map_err(|err| AppError::blocked(format!("{label} 경로 재검증 실패: {err}")))?;
    let file_metadata = file
        .metadata()
        .map_err(|err| AppError::blocked(format!("{label} handle 검증 실패: {err}")))?;
    if path_metadata.file_type().is_symlink()
        || !path_metadata.is_file()
        || path_metadata.dev() != file_metadata.dev()
        || path_metadata.ino() != file_metadata.ino()
    {
        return Err(AppError::blocked(format!(
            "{label} path/handle identity 불일치; 증거를 보존했습니다."
        )));
    }
    Ok(())
}

#[cfg(windows)]
fn validate_open_read_identity(
    path: &std::path::Path,
    file: &File,
    label: &str,
) -> Result<(), AppError> {
    let path_metadata = fs::symlink_metadata(path)
        .map_err(|err| AppError::blocked(format!("{label} 경로 재검증 실패: {err}")))?;
    let same_file = windows_replace::path_refers_to_open_file(path, file)
        .map_err(|err| AppError::blocked(format!("{label} handle 검증 실패: {err}")))?;
    if path_metadata.file_type().is_symlink() || !path_metadata.is_file() || !same_file {
        return Err(AppError::blocked(format!(
            "{label} path/handle identity 불일치; 증거를 보존했습니다."
        )));
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn validate_open_read_identity(
    path: &std::path::Path,
    file: &File,
    label: &str,
) -> Result<(), AppError> {
    let path_metadata = fs::symlink_metadata(path)
        .map_err(|err| AppError::blocked(format!("{label} 경로 재검증 실패: {err}")))?;
    let file_metadata = file
        .metadata()
        .map_err(|err| AppError::blocked(format!("{label} handle 검증 실패: {err}")))?;
    if path_metadata.file_type().is_symlink()
        || !path_metadata.is_file()
        || path_metadata.len() != file_metadata.len()
    {
        return Err(AppError::blocked(format!(
            "{label} path/handle identity 불일치; 증거를 보존했습니다."
        )));
    }
    Ok(())
}

fn tui_detail_value<'a>(details: &'a str, key: &str) -> Option<&'a str> {
    details.split_ascii_whitespace().find_map(|part| {
        let (candidate, value) = part.split_once('=')?;
        (candidate == key).then_some(value)
    })
}

fn with_validation_gap_writes_suppressed<T>(
    action: impl FnOnce() -> Result<T, AppError>,
) -> Result<T, AppError> {
    SUPPRESS_VALIDATION_GAP_WRITES.with(|flag| {
        let previous = flag.replace(true);
        let result = action();
        flag.set(previous);
        result
    })
}

pub(crate) fn current_state_lease_view_under_transition() -> Result<CurrentStateLeaseView, AppError>
{
    let path = paths::current_state_file();
    let body = fs::read_to_string(&path)
        .map_err(|err| AppError::blocked(format!("current-state lease 읽기 실패: {err}")))?;
    let snapshot = parse_current_state(&body, "current-state lease")?;
    if snapshot.schema_version == 1 {
        promote_current_state_v1()?;
        return current_state_lease_view_under_transition();
    }
    let current_ledger = ledger::validated_ledger_binding()?;
    if snapshot.ledger_binding != current_ledger {
        return snapshot_domain::validate_current_lease(&snapshot, &current_ledger, None);
    }
    let active_workflow = snapshot
        .active_workflow
        .as_ref()
        .map(|binding| load_workflow_under_transition(&binding.workflow_id))
        .transpose()?;
    snapshot_domain::validate_current_lease(&snapshot, &current_ledger, active_workflow.as_ref())
}

pub(crate) fn selection_observation_under_transition() -> Result<SelectionObservation, AppError> {
    let identity = ledger::validated_current_identity()?;
    let lease = current_state_lease_view_under_transition()?;
    let body = fs::read_to_string(paths::current_state_file())
        .map_err(|err| AppError::blocked(format!("selection current-state 읽기 실패: {err}")))?;
    let snapshot = parse_current_state(&body, "selection current-state")?;
    snapshot_domain::validate_snapshot_identity(&snapshot, &identity)?;
    let active = snapshot
        .active_workflow
        .as_ref()
        .map(|binding| load_workflow_under_transition(&binding.workflow_id))
        .transpose()?;
    Ok(SelectionObservation {
        project_id: identity.project_id,
        session_id: identity.session_id,
        current_revision: lease.revision,
        current_hash: lease.artifact_hash,
        active_workflow: active.map(|workflow| ObservedWorkflow {
            workflow_id: workflow.workflow_id,
            revision: workflow.revision,
            hash: workflow.artifact_hash,
        }),
    })
}

pub(crate) fn tui_lease_matches_workflow_under_transition(
    lease: &SelectionLease,
    workflow_id: &str,
) -> Result<bool, AppError> {
    let observation = selection_observation_under_transition()?;
    Ok(lease_matches_active_workflow(
        lease,
        workflow_id,
        &observation,
    ))
}

pub(crate) fn tui_lease_matches_terminal_selection_under_transition(
    lease: &SelectionLease,
    workflow_id: &str,
) -> Result<bool, AppError> {
    let observation = selection_observation_under_transition()?;
    Ok(lease_matches_terminal_selection(
        lease,
        workflow_id,
        &observation,
    ))
}

fn promote_current_state_v1() -> Result<(), AppError> {
    let _transition = lease::RecoverableLease::acquire_with_wait(
        paths::current_state_transition_lock(),
        "current-state v1 promotion",
        Duration::from_secs(5),
    )?;
    let path = paths::current_state_file();
    let temporary = paths::current_state_v2_promotion_temp();
    let current_body = fs::read_to_string(&path)
        .map_err(|err| AppError::blocked(format!("current-state promotion 읽기 실패: {err}")))?;
    let current = parse_current_state(&current_body, "current-state promotion source")?;

    if current.schema_version == 2 {
        if temporary.exists() {
            let temp_body = fs::read_to_string(&temporary).map_err(|err| {
                AppError::blocked(format!("current-state promotion temp 읽기 실패: {err}"))
            })?;
            parse_current_state_v2(&temp_body, "current-state promotion redundant temp")?;
            if temp_body != current_body {
                return Err(AppError::blocked(
                    "current-state promotion 차단\n- 이유: v2 current-state와 promotion temp가 다릅니다.\n- 동작: 둘 다 보존했습니다.",
                ));
            }
            fs::remove_file(&temporary).map_err(|err| {
                AppError::runtime(format!("redundant promotion temp 제거 실패: {err}"))
            })?;
            sync_parent(&temporary)?;
        }
        return Ok(());
    }

    if current.schema_version != 1 {
        return Err(AppError::blocked(
            "current-state promotion 차단: exact schema v1이 아닙니다.",
        ));
    }
    let previous_artifact_hash = current
        .legacy_canonical_hash
        .clone()
        .ok_or_else(|| AppError::blocked("legacy current-state canonical hash 누락"))?;
    let active_workflow = current
        .active_workflow
        .as_ref()
        .map(|binding| load_workflow_under_transition(&binding.workflow_id))
        .transpose()?
        .map(|workflow| CurrentWorkflowBinding {
            workflow_id: workflow.workflow_id,
            revision: workflow.revision,
            artifact_hash: workflow.artifact_hash,
        });
    let mut promoted = CurrentStateSnapshot {
        schema_version: 2,
        revision: 1,
        previous_artifact_hash,
        project_id: current.project_id,
        project_root: current.project_root,
        session_id: current.session_id,
        active_workflow,
        parent_session_id: current.parent_session_id,
        branch_from_event_id: current.branch_from_event_id,
        compaction_boundary: current.compaction_boundary,
        resume_source: current.resume_source,
        // Schema v1 did not persist a ledger binding. Keep parsing/classification
        // independent of the ambient ledger; promotion binds the freshly
        // validated ledger when it constructs the schema-v2 image.
        ledger_binding: ledger::LedgerBinding {
            event_count: 0,
            event_id: None,
            event_hash: "root".to_string(),
        },
        artifact_hash: String::new(),
        legacy_canonical_hash: None,
    };
    promoted.artifact_hash = sha256_text(&render_current_state_v2_payload(&promoted));
    let prepared = render_current_state_v2(&promoted);

    if temporary.exists() {
        let temp_body = fs::read_to_string(&temporary).map_err(|err| {
            AppError::blocked(format!("current-state promotion temp 읽기 실패: {err}"))
        })?;
        let temp = parse_current_state_v2(&temp_body, "current-state promotion temp")?;
        if temp_body != prepared {
            if same_v1_promotion_except_ledger(&temp, &promoted)
                && temp.ledger_binding != promoted.ledger_binding
            {
                preserve_stale_promotion_temp(&temporary, &temp_body)?;
            } else {
                return Err(AppError::blocked(
                    "current-state promotion 차단\n- 이유: promotion temp가 현재 v1에서 파생되지 않았습니다.\n- 동작: current-state와 temp를 변경하지 않았습니다.",
                ));
            }
        }
    }

    if !temporary.exists() {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary).map_err(|err| {
            AppError::runtime(format!("current-state promotion temp 생성 실패: {err}"))
        })?;
        if let Ok(metadata) = fs::metadata(&path) {
            file.set_permissions(metadata.permissions())
                .map_err(|err| {
                    AppError::runtime(format!(
                        "current-state promotion permission 복사 실패: {err}"
                    ))
                })?;
        }
        file.write_all(prepared.as_bytes()).map_err(|err| {
            AppError::runtime(format!("current-state promotion temp write 실패: {err}"))
        })?;
        file.sync_all().map_err(|err| {
            AppError::runtime(format!("current-state promotion temp sync 실패: {err}"))
        })?;
        drop(file);
        promotion_fault("after-temp-sync")?;
    }

    replace_file(&temporary, &path).map_err(|err| {
        AppError::runtime(format!(
            "current-state promotion replace 실패: {} -> {} ({err})",
            temporary.display(),
            path.display()
        ))
    })?;
    promotion_fault("after-rename")?;
    sync_parent(&path)?;
    promotion_fault("after-parent-sync")?;

    let installed = fs::read_to_string(&path).map_err(|err| {
        AppError::blocked(format!("promoted current-state 재검증 읽기 실패: {err}"))
    })?;
    if installed != prepared {
        return Err(AppError::blocked(
            "current-state promotion 재검증 차단: 설치된 bytes 불일치",
        ));
    }
    let installed = parse_current_state_v2(&installed, "promoted current-state")?;
    if installed != promoted {
        return Err(AppError::blocked(
            "current-state promotion 재검증 차단: 설치된 binding 불일치",
        ));
    }
    Ok(())
}

fn same_v1_promotion_except_ledger(
    left: &CurrentStateSnapshot,
    right: &CurrentStateSnapshot,
) -> bool {
    left.schema_version == 2
        && left.revision == 1
        && left.previous_artifact_hash == right.previous_artifact_hash
        && left.project_id == right.project_id
        && left.project_root == right.project_root
        && left.session_id == right.session_id
        && left.active_workflow == right.active_workflow
        && left.parent_session_id == right.parent_session_id
        && left.branch_from_event_id == right.branch_from_event_id
        && left.compaction_boundary == right.compaction_boundary
        && left.resume_source == right.resume_source
}

fn preserve_stale_promotion_temp(path: &std::path::Path, bytes: &str) -> Result<(), AppError> {
    let diagnostic = path.with_file_name(format!(
        "current-state.json.v2-promote.tmp.stale-{}.diagnostic",
        sha256_text(bytes)
    ));
    if diagnostic.exists() {
        let existing = fs::read_to_string(&diagnostic)
            .map_err(|err| AppError::blocked(format!("promotion diagnostic 읽기 실패: {err}")))?;
        if existing != bytes {
            return Err(AppError::blocked(
                "current-state promotion diagnostic hash 충돌로 차단",
            ));
        }
        fs::remove_file(path)
            .map_err(|err| AppError::runtime(format!("stale promotion temp 제거 실패: {err}")))?;
    } else {
        fs::rename(path, &diagnostic).map_err(|err| {
            AppError::runtime(format!("stale promotion temp 보존 이동 실패: {err}"))
        })?;
    }
    sync_parent(&diagnostic)
}

fn parse_current_state_v1(
    body: &str,
    value: strict_json::Value,
    context: &str,
) -> Result<CurrentStateSnapshot, AppError> {
    let object = strict_json::parse_object(body, CURRENT_STATE_V1_KEYS, context)?;
    require_exact_key_set(&object, CURRENT_STATE_V1_KEYS, context)?;
    validate_terminal_states(object.get("terminal_states"), context)?;
    let active_workflow = match object.get("active_workflow") {
        Some(strict_json::Value::Null) => None,
        Some(strict_json::Value::String(workflow_id)) => {
            validate_current_id(workflow_id, "workflow_id", context)?;
            Some(CurrentWorkflowBinding {
                workflow_id: workflow_id.clone(),
                revision: 0,
                artifact_hash: String::new(),
            })
        }
        _ => return Err(current_state_field_error(context, "active_workflow")),
    };
    let project_id = strict_json::string(&object, "project_id", context)?;
    let session_id = strict_json::string(&object, "session_id", context)?;
    validate_current_id(&project_id, "project_id", context)?;
    validate_current_id(&session_id, "session_id", context)?;
    let canonical = strict_json::render_compact(&value);
    Ok(CurrentStateSnapshot {
        schema_version: 1,
        revision: 0,
        previous_artifact_hash: String::new(),
        project_id,
        project_root: strict_json::string(&object, "project_root", context)?,
        session_id,
        active_workflow,
        parent_session_id: optional_string(&object, "parent_session_id", context)?,
        branch_from_event_id: optional_string(&object, "branch_from_event_id", context)?,
        compaction_boundary: optional_string(&object, "compaction_boundary", context)?,
        resume_source: optional_string(&object, "resume_source", context)?,
        ledger_binding: ledger::validated_ledger_binding()?,
        artifact_hash: String::new(),
        legacy_canonical_hash: Some(sha256_text(&canonical)),
    })
}

fn parse_current_state_v2(body: &str, context: &str) -> Result<CurrentStateSnapshot, AppError> {
    let canonical = strict_json::parse_canonical_object(body, CURRENT_STATE_V2_KEYS, context)?;
    if strict_json::canonical_u64(&canonical, "schema_version", context)? != 2 {
        return Err(current_state_field_error(context, "schema_version"));
    }
    let canonical_revision = strict_json::canonical_u64(&canonical, "revision", context)?;
    let object = strict_json::parse_object_exact_order(body, CURRENT_STATE_V2_KEYS, context)?;
    let revision = strict_json::number(&object, "revision", context)?;
    if revision == 0 || revision != canonical_revision {
        return Err(current_state_field_error(context, "revision"));
    }
    let previous_artifact_hash = strict_json::string(&object, "previous_artifact_hash", context)?;
    if previous_artifact_hash != "none" && !is_sha256(&previous_artifact_hash) {
        return Err(current_state_field_error(context, "previous_artifact_hash"));
    }
    let project_id = strict_json::string(&object, "project_id", context)?;
    let session_id = strict_json::string(&object, "session_id", context)?;
    validate_current_id(&project_id, "project_id", context)?;
    validate_current_id(&session_id, "session_id", context)?;
    let active_workflow = parse_current_workflow(object.get("active_workflow"), context)?;
    validate_terminal_states(object.get("terminal_states"), context)?;
    let ledger_binding = parse_current_ledger_binding(object.get("ledger_binding"), context)?;
    let artifact_hash = strict_json::string(&object, "artifact_hash", context)?;
    if !is_sha256(&artifact_hash) {
        return Err(current_state_field_error(context, "artifact_hash"));
    }
    let snapshot = CurrentStateSnapshot {
        schema_version: 2,
        revision,
        previous_artifact_hash,
        project_id,
        project_root: strict_json::string(&object, "project_root", context)?,
        session_id,
        active_workflow,
        parent_session_id: optional_string(&object, "parent_session_id", context)?,
        branch_from_event_id: optional_string(&object, "branch_from_event_id", context)?,
        compaction_boundary: optional_string(&object, "compaction_boundary", context)?,
        resume_source: optional_string(&object, "resume_source", context)?,
        ledger_binding,
        artifact_hash,
        legacy_canonical_hash: None,
    };
    let payload = render_current_state_v2_payload(&snapshot);
    if sha256_text(&payload) != snapshot.artifact_hash || render_current_state_v2(&snapshot) != body
    {
        return Err(AppError::blocked(format!(
            "{context} 차단\n- 이유: artifact hash 또는 canonical re-render 불일치"
        )));
    }
    Ok(snapshot)
}

fn render_current_state_v2(snapshot: &CurrentStateSnapshot) -> String {
    let payload = render_current_state_v2_payload(snapshot);
    format!(
        "{},\"artifact_hash\":\"{}\"}}",
        payload
            .strip_suffix('}')
            .expect("current-state payload object"),
        snapshot.artifact_hash
    )
}

fn render_current_state_v2_payload(snapshot: &CurrentStateSnapshot) -> String {
    let active_workflow = snapshot
        .active_workflow
        .as_ref()
        .map(|workflow| {
            format!(
                "{{\"workflow_id\":\"{}\",\"revision\":{},\"artifact_hash\":\"{}\"}}",
                ledger::json_string(&workflow.workflow_id),
                workflow.revision,
                workflow.artifact_hash
            )
        })
        .unwrap_or_else(|| "null".to_string());
    let event_id = snapshot
        .ledger_binding
        .event_id
        .as_ref()
        .map(|value| format!("\"{}\"", ledger::json_string(value)))
        .unwrap_or_else(|| "null".to_string());
    format!(
        "{{\"schema_version\":2,\"revision\":{},\"previous_artifact_hash\":\"{}\",\"project_id\":\"{}\",\"project_root\":\"{}\",\"session_id\":\"{}\",\"active_workflow\":{},\"parent_session_id\":{},\"branch_from_event_id\":{},\"compaction_boundary\":{},\"resume_source\":{},\"terminal_states\":[\"complete\",\"failed\",\"cancelled\"],\"ledger_binding\":{{\"event_count\":{},\"event_id\":{},\"event_hash\":\"{}\"}}}}",
        snapshot.revision,
        snapshot.previous_artifact_hash,
        ledger::json_string(&snapshot.project_id),
        ledger::json_string(&snapshot.project_root),
        ledger::json_string(&snapshot.session_id),
        active_workflow,
        render_optional_string(snapshot.parent_session_id.as_deref()),
        render_optional_string(snapshot.branch_from_event_id.as_deref()),
        render_optional_string(snapshot.compaction_boundary.as_deref()),
        render_optional_string(snapshot.resume_source.as_deref()),
        snapshot.ledger_binding.event_count,
        event_id,
        snapshot.ledger_binding.event_hash,
    )
}

fn render_optional_string(value: Option<&str>) -> String {
    value
        .map(|value| format!("\"{}\"", ledger::json_string(value)))
        .unwrap_or_else(|| "null".to_string())
}

fn parse_current_workflow(
    value: Option<&strict_json::Value>,
    context: &str,
) -> Result<Option<CurrentWorkflowBinding>, AppError> {
    match value {
        Some(strict_json::Value::Null) => Ok(None),
        Some(strict_json::Value::Object(object)) => {
            let expected = ["workflow_id", "revision", "artifact_hash"];
            require_exact_key_order(object, &expected, context)?;
            let workflow_id = strict_json::string(object, "workflow_id", context)?;
            validate_current_id(&workflow_id, "workflow_id", context)?;
            let revision = strict_json::number(object, "revision", context)?;
            let artifact_hash = strict_json::string(object, "artifact_hash", context)?;
            if revision == 0 || !is_sha256(&artifact_hash) {
                return Err(current_state_field_error(context, "active_workflow"));
            }
            Ok(Some(CurrentWorkflowBinding {
                workflow_id,
                revision,
                artifact_hash,
            }))
        }
        _ => Err(current_state_field_error(context, "active_workflow")),
    }
}

fn parse_current_ledger_binding(
    value: Option<&strict_json::Value>,
    context: &str,
) -> Result<ledger::LedgerBinding, AppError> {
    let Some(strict_json::Value::Object(object)) = value else {
        return Err(current_state_field_error(context, "ledger_binding"));
    };
    let expected = ["event_count", "event_id", "event_hash"];
    require_exact_key_order(object, &expected, context)?;
    let event_count = strict_json::number(object, "event_count", context)?;
    let event_id = optional_string(object, "event_id", context)?;
    if let Some(event_id) = event_id.as_deref() {
        validate_current_id(event_id, "event_id", context)?;
    }
    let event_hash = strict_json::string(object, "event_hash", context)?;
    if (event_count == 0 && (event_id.is_some() || event_hash != "root"))
        || (event_count > 0 && (event_id.is_none() || !is_sha256(&event_hash)))
    {
        return Err(current_state_field_error(context, "ledger_binding"));
    }
    Ok(ledger::LedgerBinding {
        event_count,
        event_id,
        event_hash,
    })
}

fn optional_string(
    object: &strict_json::Object,
    key: &str,
    context: &str,
) -> Result<Option<String>, AppError> {
    match object.get(key) {
        Some(strict_json::Value::Null) => Ok(None),
        Some(strict_json::Value::String(value)) => Ok(Some(value.clone())),
        _ => Err(current_state_field_error(context, key)),
    }
}

fn validate_terminal_states(
    value: Option<&strict_json::Value>,
    context: &str,
) -> Result<(), AppError> {
    let Some(strict_json::Value::Array(values)) = value else {
        return Err(current_state_field_error(context, "terminal_states"));
    };
    let actual = values
        .iter()
        .map(|value| match value {
            strict_json::Value::String(value) => Some(value.as_str()),
            _ => None,
        })
        .collect::<Option<Vec<_>>>();
    if actual.as_deref() == Some(["complete", "failed", "cancelled"].as_slice()) {
        Ok(())
    } else {
        Err(current_state_field_error(context, "terminal_states"))
    }
}

fn require_exact_key_set(
    object: &strict_json::Object,
    keys: &[&str],
    context: &str,
) -> Result<(), AppError> {
    if object.len() == keys.len() && keys.iter().all(|key| object.contains_key(key)) {
        Ok(())
    } else {
        Err(AppError::blocked(format!(
            "{context} 차단\n- 이유: exact key set 불일치"
        )))
    }
}

fn require_exact_key_order(
    object: &strict_json::Object,
    keys: &[&str],
    context: &str,
) -> Result<(), AppError> {
    let actual = object.keys().map(String::as_str).collect::<Vec<_>>();
    if actual == keys {
        Ok(())
    } else {
        Err(AppError::blocked(format!(
            "{context} 차단\n- 이유: exact nested key order 불일치"
        )))
    }
}

fn validate_current_id(value: &str, field: &str, context: &str) -> Result<(), AppError> {
    let valid = !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'));
    if valid {
        Ok(())
    } else {
        Err(current_state_field_error(context, field))
    }
}

fn current_state_field_error(context: &str, field: &str) -> AppError {
    AppError::blocked(format!(
        "{context} 차단\n- 이유: invalid current-state field\n- field: {field}"
    ))
}

fn format_session_row(session: &SessionHistoryEntry) -> String {
    let last_event = session
        .last_event_at_ms
        .map(|value| value.to_string())
        .unwrap_or_else(|| "없음".to_string());
    let summary = session.last_summary.as_deref().unwrap_or("이벤트 없음");

    format!(
        "- {} | started {} | last {} | events {} | {}",
        session.session_id, session.started_at_ms, last_event, session.event_count, summary
    )
}

fn ensure_runtime_evidence_file() -> Result<(), AppError> {
    let path = paths::runtime_evidence_file();
    if path.exists() {
        return Ok(());
    }
    fs::write(&path, "").map_err(|err| {
        AppError::runtime(format!(
            "runtime evidence store를 만들지 못했습니다: {} ({err})",
            path.display()
        ))
    })
}

fn read_current_state_summary() -> Result<String, AppError> {
    let path = paths::current_state_file();
    if !path.exists() {
        return Ok("미초기화".to_string());
    }

    let contents = fs::read_to_string(&path).map_err(|err| {
        AppError::runtime(format!(
            "current-state를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;

    let identity = ledger::fresh_identity();
    match classify_current_state(&contents, &identity) {
        CurrentStateStatus::CleanNoActiveWorkflow => {
            Ok("초기화됨, active_workflow 없음".to_string())
        }
        CurrentStateStatus::CleanActiveWorkflow => {
            Ok("초기화됨, active_workflow 확인 필요".to_string())
        }
        CurrentStateStatus::Missing => Ok("미초기화".to_string()),
        CurrentStateStatus::Corrupt => Ok("손상됨, state reconcile 필요".to_string()),
        CurrentStateStatus::StaleProject => {
            Ok("stale project state, state reconcile 필요".to_string())
        }
    }
}

fn current_state_status(identity: &RuntimeIdentity) -> Result<CurrentStateStatus, AppError> {
    let path = paths::current_state_file();
    if !path.exists() {
        return Ok(CurrentStateStatus::Missing);
    }

    let contents = fs::read_to_string(&path).map_err(|err| {
        AppError::runtime(format!(
            "current-state를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;

    Ok(classify_current_state(&contents, identity))
}

fn classify_current_state(contents: &str, identity: &RuntimeIdentity) -> CurrentStateStatus {
    let Ok(snapshot) = parse_current_state(contents, "current-state classification") else {
        return CurrentStateStatus::Corrupt;
    };
    if snapshot.project_id != identity.project_id || snapshot.project_root != identity.project_root
    {
        return CurrentStateStatus::StaleProject;
    }
    match snapshot.active_workflow {
        None => CurrentStateStatus::CleanNoActiveWorkflow,
        Some(_) => CurrentStateStatus::CleanActiveWorkflow,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CurrentStateStatus {
    Missing,
    Corrupt,
    StaleProject,
    CleanNoActiveWorkflow,
    CleanActiveWorkflow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ReconcileOutcome {
    Clean,
    Created,
    RecoveredCorrupt(PathBuf),
    RecoveredStale(PathBuf),
}

impl ReconcileOutcome {
    fn summary(&self) -> String {
        match self {
            Self::Clean => "current-state 정상".to_string(),
            Self::Created => "current-state 생성".to_string(),
            Self::RecoveredCorrupt(path) => {
                format!("손상된 current-state를 {} 로 보존 이동", path.display())
            }
            Self::RecoveredStale(path) => {
                format!("stale current-state를 {} 로 보존 이동", path.display())
            }
        }
    }
}

fn write_workflow_snapshot_bytes(record: &WorkflowRecord, rendered: &[u8]) -> Result<(), AppError> {
    let path = paths::project_workflow_snapshot_file(&record.workflow_id, record.revision);
    let parent = path
        .parent()
        .ok_or_else(|| AppError::runtime("workflow parent path 없음"))?;
    fs::create_dir_all(parent).map_err(|err| {
        AppError::runtime(format!(
            "workflow directory를 만들지 못했습니다: {} ({err})",
            parent.display()
        ))
    })?;
    if path.exists() {
        let existing = fs::read(&path).map_err(|err| {
            AppError::runtime(format!(
                "workflow snapshot read 실패: {} ({err})",
                path.display()
            ))
        })?;
        if existing == rendered {
            return Ok(());
        }
        return Err(AppError::blocked(format!(
            "workflow snapshot overwrite 차단\n- path: {}\n- 이유: immutable revision bytes conflict",
            path.display()
        )));
    }
    atomic_replace_bytes(&path, rendered)
}

fn write_workflow_pointer_for_schema(
    record: &WorkflowRecord,
    schema_version: u64,
) -> Result<(), AppError> {
    let body = render_workflow_pointer_bytes(record, schema_version)?;
    atomic_replace_bytes(
        &paths::project_workflow_file(&record.workflow_id),
        body.as_bytes(),
    )
}

fn render_workflow_pointer_bytes(
    record: &WorkflowRecord,
    schema_version: u64,
) -> Result<String, AppError> {
    crate::runtime_core::workflow::storage_compat::record::render_pointer(record, schema_version)
}

fn parse_workflow_pointer(path: &std::path::Path, body: &str) -> Result<WorkflowPointer, AppError> {
    crate::runtime_core::workflow::storage_compat::record::parse_pointer(
        path,
        body,
        corrupt_workflow,
    )
}

fn remove_workflow_transaction(workflow_id: &str) -> Result<(), AppError> {
    let path = paths::project_workflow_transaction_file(workflow_id);
    match fs::remove_file(&path) {
        Ok(()) => sync_parent(&path),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(AppError::runtime(format!(
            "workflow transaction cleanup 실패: {} ({err})",
            path.display()
        ))),
    }
}

fn recover_workflow_transaction(workflow_id: &str) -> Result<(), AppError> {
    workflow_recovery::recover_workflow_transaction(&StateWorkflowRecoveryPort, workflow_id)
}

struct StateWorkflowRecoveryPort;

impl WorkflowRecoveryPort for StateWorkflowRecoveryPort {
    fn load_transaction(
        &self,
        workflow_id: &str,
    ) -> Result<Option<PendingWorkflowTransaction>, AppError> {
        let path = paths::project_workflow_transaction_file(workflow_id);
        if !path.exists() {
            return Ok(None);
        }
        let body = read_regular_file_bounded(
            &path,
            MAX_WORKFLOW_SNAPSHOT_BYTES,
            "workflow recovery transaction",
        )?;
        let schema_version = workflow_snapshot_schema(&path, &body)?;
        let record = parse_workflow_snapshot(&path, &body)?;
        Ok(Some(PendingWorkflowTransaction {
            schema_version,
            record,
            body,
        }))
    }

    fn load_pointer(&self, workflow_id: &str) -> Result<Option<WorkflowPointer>, AppError> {
        let path = paths::project_workflow_file(workflow_id);
        if !path.exists() {
            return Ok(None);
        }
        let body = read_regular_file_bounded(
            &path,
            MAX_WORKFLOW_POINTER_BYTES,
            "workflow recovery pointer",
        )?;
        parse_workflow_pointer(&path, &body).map(Some)
    }

    fn checkpoints(&self, workflow_id: &str) -> Result<Vec<ledger::WorkflowCheckpoint>, AppError> {
        ledger::workflow_checkpoints(workflow_id)
    }

    fn validate_chain(
        &self,
        workflow_id: &str,
        committed_revision: u64,
        expected_latest_schema: u64,
    ) -> Result<WorkflowRecord, AppError> {
        validate_workflow_chain(workflow_id, committed_revision, expected_latest_schema)
    }

    fn validate_chain_with_checkpoints(
        &self,
        workflow_id: &str,
        committed_revision: u64,
        expected_latest_schema: u64,
        checkpoints: &[ledger::WorkflowCheckpoint],
    ) -> Result<WorkflowRecord, AppError> {
        validate_workflow_chain_with_checkpoints(
            workflow_id,
            committed_revision,
            expected_latest_schema,
            checkpoints,
        )
    }

    fn current_identity(&self) -> Result<RuntimeIdentity, AppError> {
        ledger::validated_current_identity()
    }

    fn checkpoint_exists(
        &self,
        workflow_id: &str,
        revision: u64,
        artifact_hash: &str,
    ) -> Result<bool, AppError> {
        ledger::workflow_checkpoint_exists(workflow_id, revision, artifact_hash)
    }

    fn install_snapshot(&self, record: &WorkflowRecord, body: &[u8]) -> Result<(), AppError> {
        write_workflow_snapshot_bytes(record, body)
    }

    fn install_pointer(
        &self,
        record: &WorkflowRecord,
        schema_version: u64,
    ) -> Result<(), AppError> {
        write_workflow_pointer_for_schema(record, schema_version)
    }

    fn remove_transaction(&self, workflow_id: &str) -> Result<(), AppError> {
        remove_workflow_transaction(workflow_id)
    }

    fn corrupt(&self, workflow_id: &str, artifact: RecoveryArtifact) -> AppError {
        let path = match artifact {
            RecoveryArtifact::Transaction => paths::project_workflow_transaction_file(workflow_id),
            RecoveryArtifact::Pointer => paths::project_workflow_file(workflow_id),
        };
        corrupt_workflow(&path)
    }
}

#[cfg(test)]
fn append_workflow_checkpoint_event(record: &WorkflowRecord) -> Result<(), AppError> {
    let event = workflow_checkpoint_event(record, &workflow_identity(record));
    ledger::append_event(&event)?;
    observability::project_event(&event)
}

fn validate_workflow_chain(
    workflow_id: &str,
    committed_revision: u64,
    expected_latest_schema: u64,
) -> Result<WorkflowRecord, AppError> {
    let checkpoints = ledger::workflow_checkpoints(workflow_id)?;
    if checkpoints.len() != committed_revision as usize {
        return Err(AppError::blocked(format!(
            "workflow chain 검증 차단\n- workflow id: {workflow_id}\n- committed revision: {committed_revision}\n- ledger checkpoints: {}",
            checkpoints.len()
        )));
    }
    validate_workflow_chain_with_checkpoints(
        workflow_id,
        committed_revision,
        expected_latest_schema,
        &checkpoints,
    )
}

fn validate_workflow_chain_with_checkpoints(
    workflow_id: &str,
    committed_revision: u64,
    expected_latest_schema: u64,
    checkpoints: &[ledger::WorkflowCheckpoint],
) -> Result<WorkflowRecord, AppError> {
    if checkpoints.len() != committed_revision as usize {
        return Err(corrupt_workflow(&paths::project_workflow_file(workflow_id)));
    }
    let mut previous_hash = "none".to_string();
    let mut previous_schema = None;
    let mut latest = None;
    for revision in 1..=committed_revision {
        let path = paths::project_workflow_snapshot_file(workflow_id, revision);
        let body = read_regular_file_bounded(
            &path,
            MAX_WORKFLOW_SNAPSHOT_BYTES,
            "workflow chain revision snapshot",
        )?;
        let schema = workflow_snapshot_schema(&path, &body)?;
        if previous_schema.is_some_and(|previous| schema < previous) {
            return Err(corrupt_workflow(&path));
        }
        let record = parse_workflow_snapshot(&path, &body)?;
        let checkpoint = &checkpoints[(revision - 1) as usize];
        if record.workflow_id != workflow_id
            || record.revision != revision
            || record.previous_hash != previous_hash
            || checkpoint.revision != revision
            || checkpoint.artifact_hash != record.artifact_hash
            || checkpoint.previous_hash != previous_hash
        {
            return Err(corrupt_workflow(&path));
        }
        previous_hash = record.artifact_hash.clone();
        previous_schema = Some(schema);
        latest = Some(record);
    }
    if previous_schema != Some(expected_latest_schema) {
        return Err(corrupt_workflow(&paths::project_workflow_file(workflow_id)));
    }
    latest.ok_or_else(|| corrupt_workflow(&paths::project_workflow_file(workflow_id)))
}

fn workflow_snapshot_schema(path: &std::path::Path, body: &str) -> Result<u64, AppError> {
    crate::runtime_core::workflow::storage_compat::record::snapshot_schema(
        path,
        body,
        corrupt_workflow,
    )
}

fn parse_workflow_snapshot(path: &std::path::Path, body: &str) -> Result<WorkflowRecord, AppError> {
    crate::runtime_core::workflow::storage_compat::record::parse_snapshot(
        path,
        body,
        corrupt_workflow,
    )
}

fn workflow_identity(record: &WorkflowRecord) -> RuntimeIdentity {
    RuntimeIdentity {
        project_id: record.project_id.clone(),
        session_id: record.session_id.clone(),
        project_root: paths::project_root().display().to_string(),
    }
}

fn workflow_checkpoint_event(
    record: &WorkflowRecord,
    identity: &RuntimeIdentity,
) -> ledger::LedgerEvent {
    ledger::new_event_for(
        identity,
        "workflow.checkpoint",
        "canonical workflow revision persisted",
        &workflow_checkpoint_event_details(record),
    )
}

fn workflow_checkpoint_event_details(record: &WorkflowRecord) -> String {
    format!(
        "workflow_id={} revision={} artifact_hash={} previous_hash={} phase={} workflow_kind={} active_skill_id={} skill_state={} action_id={} action_kind={} proposal_id={} evidence_id={}",
        record.workflow_id,
        record.revision,
        record.artifact_hash,
        record.previous_hash,
        record.phase,
        record.workflow_kind,
        record.active_skill_id,
        record.skill_state,
        record.action_id,
        record.action_kind,
        display_empty(&record.proposal_id),
        display_empty(&record.evidence_id)
    )
}

fn prepared_workflow_member_id(
    domain: &str,
    prefix: &str,
    workflow_id: &str,
    revision: u64,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    hasher.update([0]);
    hasher.update(workflow_id.as_bytes());
    hasher.update([0]);
    hasher.update(revision.to_string().as_bytes());
    let digest = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("{prefix}-{digest}")
}

pub(crate) fn atomic_replace_bytes(path: &std::path::Path, bytes: &[u8]) -> Result<(), AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::runtime("atomic write parent path 없음"))?;
    fs::create_dir_all(parent).map_err(|err| {
        AppError::runtime(format!(
            "atomic write directory 생성 실패: {} ({err})",
            parent.display()
        ))
    })?;
    let temporary = path.with_extension(format!("tmp.{}.{}", std::process::id(), now_ms()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary).map_err(|err| {
        AppError::runtime(format!(
            "atomic temp 생성 실패: {} ({err})",
            temporary.display()
        ))
    })?;
    if let Ok(metadata) = fs::metadata(path) {
        file.set_permissions(metadata.permissions())
            .map_err(|err| AppError::runtime(format!("atomic temp permission 복사 실패: {err}")))?;
    }
    file.write_all(bytes)
        .map_err(|err| AppError::runtime(format!("atomic temp write 실패: {err}")))?;
    file.sync_all()
        .map_err(|err| AppError::runtime(format!("atomic temp sync 실패: {err}")))?;
    drop(file);
    replace_file(&temporary, path).map_err(|err| {
        let _ = fs::remove_file(&temporary);
        AppError::runtime(format!(
            "atomic replace 실패: {} -> {} ({err})",
            temporary.display(),
            path.display()
        ))
    })?;
    sync_parent(path)
}

#[cfg(not(unix))]
pub(crate) fn install_prepared_source_bundle(
    _bundle: &transition::PreparedSourceBundle,
    _journal_path: &std::path::Path,
) -> Result<(), AppError> {
    Err(AppError::blocked(format!(
        "source install 차단\n- code: source-install.unsupported-platform\n- platform: {}\n- 지원 범위: v0.34.0 source installation은 Unix만 지원합니다.\n- 동작: journal/temp/guard/rollback/target 변경 없음",
        std::env::consts::OS
    )))
}

#[cfg(unix)]
pub(crate) fn install_prepared_source_bundle(
    bundle: &transition::PreparedSourceBundle,
    journal_path: &std::path::Path,
) -> Result<(), AppError> {
    let body = read_regular_file_bounded(
        journal_path,
        MAX_PREPARED_SOURCE_BUNDLE_BYTES,
        "prepared source journal",
    )?;
    if transition::parse_prepared_source_bundle(&body)? != *bundle {
        return Err(AppError::blocked(
            "prepared source journal/bundle binding 불일치",
        ));
    }
    recover_source_replace(journal_path)
}

#[cfg(unix)]
pub(crate) fn validate_prepared_source_parent(
    bundle: &transition::PreparedSourceBundle,
) -> Result<(), AppError> {
    let plan = bundle
        .source_install
        .as_ref()
        .ok_or_else(|| AppError::blocked("prepared source parent plan 누락"))?;
    PreparedSourceDir::open(plan)?;
    PreparedRollbackDir::preflight(plan)
}

#[cfg(unix)]
pub(crate) fn validate_source_install_initial_admission(
    plan: &transition::SourceInstallV1,
) -> Result<(), AppError> {
    let Some(directory) = PreparedRollbackDir::open(plan, false)? else {
        return Ok(());
    };
    if directory.open_existing()?.is_some() {
        return Err(AppError::blocked(
            "source rollback create-new admission 차단: rollback path가 journal commit 전에 이미 존재합니다.",
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn validate_prepared_source_parent(
    _bundle: &transition::PreparedSourceBundle,
) -> Result<(), AppError> {
    Err(AppError::blocked(format!(
        "source install 차단\n- code: source-install.unsupported-platform\n- platform: {}",
        std::env::consts::OS
    )))
}

#[cfg(unix)]
struct PreparedSourceDir {
    handle: File,
    target: String,
    temporary: String,
    guard: String,
}

#[cfg(unix)]
impl PreparedSourceDir {
    fn open(plan: &transition::SourceInstallV1) -> Result<Self, AppError> {
        use std::os::unix::fs::MetadataExt;

        if plan.target.parent != plan.install_temp.parent
            || plan.target.parent != plan.guard_path.parent
        {
            return Err(AppError::blocked(
                "prepared source sibling parent binding 불일치",
            ));
        }
        let root = paths::project_root().canonicalize().map_err(|err| {
            AppError::blocked(format!(
                "prepared source project root canonicalize 실패: {err}"
            ))
        })?;
        let mut handle = File::open(&root).map_err(|err| {
            AppError::blocked(format!("prepared source project root open 실패: {err}"))
        })?;
        for component in plan
            .target
            .parent
            .split('/')
            .filter(|value| !value.is_empty())
        {
            handle = openat_file(
                &handle,
                component,
                unix_open_flags::READ_DIRECTORY_NOFOLLOW,
                0,
                "prepared source parent traversal",
            )?;
        }
        let metadata = handle.metadata().map_err(|err| {
            AppError::blocked(format!("prepared source parent metadata 실패: {err}"))
        })?;
        if !metadata.is_dir() || metadata.dev() != plan.unix_metadata.before_dev {
            return Err(AppError::blocked(
                "prepared source parent directory/filesystem binding 불일치",
            ));
        }
        Ok(Self {
            handle,
            target: plan.target.basename.clone(),
            temporary: plan.install_temp.basename.clone(),
            guard: plan.guard_path.basename.clone(),
        })
    }

    fn open_existing(&self, name: &str) -> Result<Option<File>, AppError> {
        match openat_file(
            &self.handle,
            name,
            unix_open_flags::READ_FILE_NOFOLLOW,
            0,
            "prepared source stage open",
        ) {
            Ok(file) => Ok(Some(file)),
            Err(error) if error.message.ends_with("(not found)") => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn create_new(&self, name: &str, mode: u32) -> Result<File, AppError> {
        openat_file(
            &self.handle,
            name,
            unix_open_flags::WRITE_CREATE_NEW_NOFOLLOW,
            mode,
            "prepared source create-new",
        )
    }

    fn stage_hash(&self, name: &str) -> Result<Option<String>, AppError> {
        let Some(mut file) = self.open_existing(name)? else {
            return Ok(None);
        };
        if !file
            .metadata()
            .map_err(|err| AppError::blocked(format!("source stage metadata 실패: {err}")))?
            .is_file()
        {
            return Err(AppError::blocked("source stage type 불일치"));
        }
        let bytes = read_open_file_bounded(
            &mut file,
            transition::MAX_SOURCE_BLOB_BYTES as u64,
            "source stage reread",
        )?;
        Ok(Some(sha256_bytes(&bytes)))
    }

    fn validate_original(
        &self,
        name: &str,
        plan: &transition::SourceInstallV1,
    ) -> Result<(), AppError> {
        use std::os::unix::fs::MetadataExt;
        let file = self
            .open_existing(name)?
            .ok_or_else(|| AppError::blocked("source original 누락"))?;
        if self.stage_hash(name)?.as_deref() != Some(plan.before_sha256.as_str()) {
            return Err(AppError::blocked("source stage hash/type 불일치"));
        }
        let metadata = file
            .metadata()
            .map_err(|err| AppError::blocked(format!("source original metadata 실패: {err}")))?;
        validate_source_metadata(&metadata, plan, false)?;
        let identity =
            transition::source_identity_v1(metadata.dev(), metadata.ino(), &plan.before_sha256)?;
        if plan.target.expected_identity.as_deref() != Some(identity.as_str()) {
            return Err(AppError::blocked(
                "source original expected identity 불일치",
            ));
        }
        Ok(())
    }

    fn validate_installed(
        &self,
        name: &str,
        plan: &transition::SourceInstallV1,
    ) -> Result<(), AppError> {
        if self.stage_hash(name)?.as_deref() != Some(plan.proposed_sha256.as_str()) {
            return Err(AppError::blocked("source stage hash/type 불일치"));
        }
        let file = self
            .open_existing(name)?
            .ok_or_else(|| AppError::blocked("source installed 누락"))?;
        let metadata = file
            .metadata()
            .map_err(|err| AppError::blocked(format!("source installed metadata 실패: {err}")))?;
        validate_source_metadata(&metadata, plan, true)
    }

    fn validate_original_pair(&self, plan: &transition::SourceInstallV1) -> Result<(), AppError> {
        use std::os::unix::fs::MetadataExt;
        self.validate_original(&self.target, plan)?;
        self.validate_original(&self.guard, plan)?;
        let target = self.open_existing(&self.target)?.expect("validated target");
        let guard = self.open_existing(&self.guard)?.expect("validated guard");
        let target_metadata = target
            .metadata()
            .map_err(|err| AppError::blocked(format!("source target identity 실패: {err}")))?;
        let guard_metadata = guard
            .metadata()
            .map_err(|err| AppError::blocked(format!("source guard identity 실패: {err}")))?;
        if target_metadata.dev() != guard_metadata.dev()
            || target_metadata.ino() != guard_metadata.ino()
        {
            return Err(AppError::blocked(
                "source target/guard inode identity 불일치",
            ));
        }
        Ok(())
    }

    fn validate_installed_pair(&self, plan: &transition::SourceInstallV1) -> Result<(), AppError> {
        use std::os::unix::fs::MetadataExt;
        self.validate_installed(&self.target, plan)?;
        self.validate_installed(&self.temporary, plan)?;
        let target = self.open_existing(&self.target)?.expect("validated target");
        let temporary = self
            .open_existing(&self.temporary)?
            .expect("validated temporary");
        let target_metadata = target
            .metadata()
            .map_err(|err| AppError::blocked(format!("installed source identity 실패: {err}")))?;
        let temporary_metadata = temporary
            .metadata()
            .map_err(|err| AppError::blocked(format!("install temp identity 실패: {err}")))?;
        if target_metadata.dev() != temporary_metadata.dev()
            || target_metadata.ino() != temporary_metadata.ino()
        {
            return Err(AppError::blocked(
                "installed target/temp inode identity 불일치",
            ));
        }
        Ok(())
    }

    fn link(&self, from: &str, to: &str) -> Result<(), AppError> {
        dir_linkat(&self.handle, from, to)
    }

    fn unlink(&self, name: &str) -> Result<(), AppError> {
        dir_unlinkat(&self.handle, name)
    }

    fn sync(&self) -> Result<(), AppError> {
        self.handle
            .sync_all()
            .map_err(|err| AppError::runtime(format!("source parent fsync 실패: {err}")))
    }
}

#[cfg(unix)]
struct PreparedRollbackDir {
    handle: File,
    rollback: String,
}

#[cfg(unix)]
impl PreparedRollbackDir {
    fn preflight(plan: &transition::SourceInstallV1) -> Result<(), AppError> {
        let _ = Self::open(plan, false)?;
        Ok(())
    }

    fn open(
        plan: &transition::SourceInstallV1,
        create_missing: bool,
    ) -> Result<Option<Self>, AppError> {
        let root = paths::project_root().canonicalize().map_err(|err| {
            AppError::blocked(format!(
                "prepared rollback project root canonicalize 실패: {err}"
            ))
        })?;
        let mut handle = File::open(&root).map_err(|err| {
            AppError::blocked(format!("prepared rollback project root open 실패: {err}"))
        })?;
        for component in plan
            .rollback_final
            .parent
            .split('/')
            .filter(|value| !value.is_empty())
        {
            match openat_file(
                &handle,
                component,
                unix_open_flags::READ_DIRECTORY_NOFOLLOW,
                0,
                "prepared rollback parent traversal",
            ) {
                Ok(next) => handle = next,
                Err(error) if error.message.ends_with("(not found)") && !create_missing => {
                    return Ok(None);
                }
                Err(error) if error.message.ends_with("(not found)") => {
                    mkdirat_directory(&handle, component, 0o700)?;
                    handle = openat_file(
                        &handle,
                        component,
                        unix_open_flags::READ_DIRECTORY_NOFOLLOW,
                        0,
                        "prepared rollback created parent open",
                    )?;
                }
                Err(error) => return Err(error),
            }
        }
        let metadata = handle.metadata().map_err(|err| {
            AppError::blocked(format!("prepared rollback parent metadata 실패: {err}"))
        })?;
        if !metadata.is_dir() {
            return Err(AppError::blocked("prepared rollback parent type 불일치"));
        }
        Ok(Some(Self {
            handle,
            rollback: plan.rollback_final.basename.clone(),
        }))
    }

    fn open_existing(&self) -> Result<Option<File>, AppError> {
        match openat_file(
            &self.handle,
            &self.rollback,
            unix_open_flags::READ_FILE_NOFOLLOW,
            0,
            "prepared rollback open",
        ) {
            Ok(file) => Ok(Some(file)),
            Err(error) if error.message.ends_with("(not found)") => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn create_new(&self) -> Result<File, AppError> {
        openat_file(
            &self.handle,
            &self.rollback,
            unix_open_flags::WRITE_CREATE_NEW_NOFOLLOW,
            0o600,
            "prepared rollback create-new",
        )
    }

    fn validate(&self, plan: &transition::SourceInstallV1) -> Result<(), AppError> {
        let mut file = self
            .open_existing()?
            .ok_or_else(|| AppError::blocked("source rollback 누락"))?;
        let metadata = file
            .metadata()
            .map_err(|err| AppError::blocked(format!("source rollback metadata 실패: {err}")))?;
        if !metadata.is_file() {
            return Err(AppError::blocked("source rollback type 불일치"));
        }
        let bytes =
            read_open_file_bounded(&mut file, plan.before_byte_length, "source rollback read")?;
        if sha256_bytes(&bytes) != plan.before_sha256
            || u64::try_from(bytes.len()).ok() != Some(plan.before_byte_length)
        {
            return Err(AppError::blocked("source rollback hash/length 불일치"));
        }
        Ok(())
    }

    fn sync(&self) -> Result<(), AppError> {
        self.handle
            .sync_all()
            .map_err(|err| AppError::runtime(format!("source rollback parent fsync 실패: {err}")))
    }
}

#[cfg(unix)]
mod unix_open_flags {
    #[cfg(target_os = "macos")]
    pub const READ_DIRECTORY_NOFOLLOW: i32 = 0x0010_0000 | 0x0000_0100 | 0x0100_0000;
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    pub const READ_DIRECTORY_NOFOLLOW: i32 = 0x0000_4000 | 0x0000_8000 | 0x0008_0000;
    #[cfg(all(
        not(target_os = "macos"),
        not(all(target_os = "linux", target_arch = "aarch64"))
    ))]
    pub const READ_DIRECTORY_NOFOLLOW: i32 = 0x0001_0000 | 0x0002_0000 | 0x0008_0000;
    #[cfg(target_os = "macos")]
    pub const READ_FILE_NOFOLLOW: i32 = 0x0000_0100 | 0x0100_0000;
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    pub const READ_FILE_NOFOLLOW: i32 = 0x0000_8000 | 0x0008_0000;
    #[cfg(all(
        not(target_os = "macos"),
        not(all(target_os = "linux", target_arch = "aarch64"))
    ))]
    pub const READ_FILE_NOFOLLOW: i32 = 0x0002_0000 | 0x0008_0000;
    #[cfg(target_os = "macos")]
    pub const WRITE_CREATE_NEW_NOFOLLOW: i32 =
        0x0000_0001 | 0x0000_0200 | 0x0000_0800 | 0x0000_0100 | 0x0100_0000;
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    pub const WRITE_CREATE_NEW_NOFOLLOW: i32 =
        0x0000_0001 | 0x0000_0040 | 0x0000_0080 | 0x0000_8000 | 0x0008_0000;
    #[cfg(all(
        not(target_os = "macos"),
        not(all(target_os = "linux", target_arch = "aarch64"))
    ))]
    pub const WRITE_CREATE_NEW_NOFOLLOW: i32 =
        0x0000_0001 | 0x0000_0040 | 0x0000_0080 | 0x0002_0000 | 0x0008_0000;
}

#[cfg(unix)]
fn openat_file(
    directory: &File,
    name: &str,
    flags: i32,
    mode: u32,
    context: &str,
) -> Result<File, AppError> {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd};
    unsafe extern "C" {
        fn openat(directory_fd: i32, path: *const std::ffi::c_char, flags: i32, mode: u32) -> i32;
    }
    let name =
        CString::new(name).map_err(|_| AppError::blocked(format!("{context} NUL path 차단")))?;
    // SAFETY: directory is an owned live descriptor, name is NUL-terminated, and mode is
    // supplied for both creating and non-creating calls (ignored by the latter).
    let descriptor = unsafe { openat(directory.as_raw_fd(), name.as_ptr(), flags, mode) };
    if descriptor < 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(2) {
            return Err(AppError::blocked(format!("{context} (not found)")));
        }
        return Err(AppError::blocked(format!("{context} 실패: {error}")));
    }
    // SAFETY: openat returned a new owned descriptor.
    Ok(unsafe { File::from_raw_fd(descriptor) })
}

#[cfg(unix)]
fn mkdirat_directory(directory: &File, name: &str, mode: u32) -> Result<(), AppError> {
    use std::ffi::CString;
    use std::os::fd::AsRawFd;
    unsafe extern "C" {
        fn mkdirat(directory_fd: i32, path: *const std::ffi::c_char, mode: u32) -> i32;
    }
    let name = CString::new(name)
        .map_err(|_| AppError::blocked("prepared rollback mkdir NUL path 차단"))?;
    // SAFETY: the path is NUL-terminated and resolved beneath the retained directory fd.
    if unsafe { mkdirat(directory.as_raw_fd(), name.as_ptr(), mode) } != 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(17) {
            return Err(AppError::blocked(format!(
                "prepared rollback parent create 실패: {error}"
            )));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn dir_linkat(directory: &File, from: &str, to: &str) -> Result<(), AppError> {
    use std::ffi::CString;
    use std::os::fd::AsRawFd;
    unsafe extern "C" {
        fn linkat(
            old_directory_fd: i32,
            old_path: *const std::ffi::c_char,
            new_directory_fd: i32,
            new_path: *const std::ffi::c_char,
            flags: i32,
        ) -> i32;
    }
    let from = CString::new(from).map_err(|_| AppError::blocked("source link NUL path 차단"))?;
    let to = CString::new(to).map_err(|_| AppError::blocked("source link NUL path 차단"))?;
    // SAFETY: both paths are NUL-terminated and resolved relative to the same live directory.
    if unsafe {
        linkat(
            directory.as_raw_fd(),
            from.as_ptr(),
            directory.as_raw_fd(),
            to.as_ptr(),
            0,
        )
    } != 0
    {
        return Err(AppError::blocked(format!(
            "source recovery create-new link 실패: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(())
}

#[cfg(unix)]
fn dir_unlinkat(directory: &File, name: &str) -> Result<(), AppError> {
    use std::ffi::CString;
    use std::os::fd::AsRawFd;
    unsafe extern "C" {
        fn unlinkat(directory_fd: i32, path: *const std::ffi::c_char, flags: i32) -> i32;
    }
    let name = CString::new(name).map_err(|_| AppError::blocked("source unlink NUL path 차단"))?;
    // SAFETY: the path is NUL-terminated and resolved under the retained directory descriptor.
    if unsafe { unlinkat(directory.as_raw_fd(), name.as_ptr(), 0) } != 0 {
        return Err(AppError::blocked(format!(
            "source recovery unlink 실패: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(())
}

#[cfg(unix)]
fn recover_source_replace(transaction_path: &std::path::Path) -> Result<(), AppError> {
    if !transaction_path.exists() {
        return Ok(());
    }
    let body = read_regular_file_bounded(
        transaction_path,
        MAX_PREPARED_SOURCE_BUNDLE_BYTES,
        "source recovery transaction",
    )?;
    let bundle = transition::parse_prepared_source_bundle(&body)?;
    let plan = bundle
        .source_install
        .as_ref()
        .ok_or_else(|| AppError::blocked("source transaction source_install_v1 누락"))?;
    let proposed_bytes = bundle
        .proposed_bytes
        .as_deref()
        .ok_or_else(|| AppError::blocked("source transaction proposed bytes 누락"))?;
    let source_dir = PreparedSourceDir::open(plan)?;
    let original_hash = plan.before_sha256.as_str();
    let replacement_hash = plan.proposed_sha256.as_str();

    let mut target_hash = source_dir.stage_hash(&source_dir.target)?;
    let rollback_dir = PreparedRollbackDir::open(plan, false)?;
    let rollback_exists = match rollback_dir.as_ref() {
        Some(directory) => directory.open_existing()?.is_some(),
        None => false,
    };
    if rollback_exists {
        rollback_dir
            .as_ref()
            .expect("checked rollback directory")
            .validate(plan)?;
    } else if target_hash.as_deref() == Some(original_hash) {
        source_dir.validate_original(&source_dir.target, plan)?;
        install_prepared_rollback(plan, &source_dir)?;
    } else {
        return Err(AppError::blocked(
            "source recovery rollback evidence가 누락되었습니다.",
        ));
    }
    if source_dir.stage_hash(&source_dir.temporary)?.is_none()
        && target_hash.as_deref() != Some(replacement_hash)
    {
        install_prepared_temp(plan, proposed_bytes.as_bytes(), &source_dir)?;
    }
    let guard_hash = source_dir.stage_hash(&source_dir.guard)?;
    let temporary_hash = source_dir.stage_hash(&source_dir.temporary)?;
    if temporary_hash
        .as_deref()
        .is_some_and(|hash| hash != replacement_hash)
        || guard_hash
            .as_deref()
            .is_some_and(|hash| hash != original_hash)
        || target_hash
            .as_deref()
            .is_some_and(|hash| hash != original_hash && hash != replacement_hash)
    {
        return Err(AppError::blocked(
            "source transaction recovery conflict; 외부 source를 덮어쓰지 않았습니다.",
        ));
    }

    if target_hash.as_deref() == Some(original_hash) && guard_hash.is_none() {
        if temporary_hash.as_deref() != Some(replacement_hash) {
            return Err(AppError::blocked("source transaction proposed temp 누락"));
        }
        source_dir.validate_original(&source_dir.target, plan)?;
        source_dir.link(&source_dir.target, &source_dir.guard)?;
        source_dir.validate_original_pair(plan)?;
        source_replace_fault("after-guard")?;
    }
    if source_dir.stage_hash(&source_dir.target)?.as_deref() == Some(original_hash) {
        source_dir.validate_original_pair(plan)?;
        source_dir.sync()?;
        source_dir.validate_original_pair(plan)?;
        source_dir.unlink(&source_dir.target)?;
    }
    if source_dir.stage_hash(&source_dir.target)?.is_none()
        && source_dir.stage_hash(&source_dir.guard)?.is_some()
    {
        source_dir.validate_original(&source_dir.guard, plan)?;
        if source_dir.stage_hash(&source_dir.temporary)?.as_deref() != Some(replacement_hash) {
            return Err(AppError::blocked("source recovery install temp 누락"));
        }
        source_dir.link(&source_dir.temporary, &source_dir.target)?;
        source_dir.sync()?;
        source_replace_fault("after-install")?;
    }
    target_hash = source_dir.stage_hash(&source_dir.target)?;
    if target_hash.as_deref() != Some(replacement_hash) {
        if target_hash.is_none() && source_dir.stage_hash(&source_dir.guard)?.is_none() {
            return Err(AppError::blocked("source transaction recovery bytes 누락"));
        }
        return Err(AppError::blocked("source transaction recovery bytes 누락"));
    }
    source_dir.validate_installed(&source_dir.target, plan)?;
    if source_dir.stage_hash(&source_dir.temporary)?.is_some() {
        source_dir.validate_installed_pair(plan)?;
    }
    if source_dir.stage_hash(&source_dir.temporary)?.is_some() {
        source_dir.unlink(&source_dir.temporary)?;
    }
    if source_dir.stage_hash(&source_dir.guard)?.is_some() {
        source_dir.unlink(&source_dir.guard)?;
    }
    source_dir.sync()
}

#[cfg(unix)]
fn install_prepared_temp(
    plan: &transition::SourceInstallV1,
    proposed: &[u8],
    source_dir: &PreparedSourceDir,
) -> Result<(), AppError> {
    if sha256_bytes(proposed) != plan.proposed_sha256
        || u64::try_from(proposed.len()).ok() != Some(plan.proposed_byte_length)
    {
        return Err(AppError::blocked(
            "source install temp proposed bytes binding 불일치",
        ));
    }
    let mut file = source_dir.create_new(&source_dir.temporary, 0o600)?;
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::PermissionsExt;
    unsafe extern "C" {
        fn fchown(fd: i32, owner: u32, group: u32) -> i32;
    }
    // SAFETY: `file` owns a valid open descriptor and the uid/gid were capability-checked
    // before the transition journal was committed.
    if unsafe {
        fchown(
            file.as_raw_fd(),
            plan.unix_metadata.install_uid,
            plan.unix_metadata.install_gid,
        )
    } != 0
    {
        return Err(AppError::runtime(format!(
            "source install ownership 적용 실패: {}",
            std::io::Error::last_os_error()
        )));
    }
    file.write_all(proposed)
        .map_err(|err| AppError::runtime(format!("source install temp write 실패: {err}")))?;
    file.set_permissions(fs::Permissions::from_mode(plan.unix_metadata.install_mode))
        .map_err(|err| AppError::runtime(format!("source install metadata 적용 실패: {err}")))?;
    file.sync_all()
        .map_err(|err| AppError::runtime(format!("source install temp fsync 실패: {err}")))?;
    drop(file);
    source_dir.validate_installed(&source_dir.temporary, plan)
}

#[cfg(unix)]
fn install_prepared_rollback(
    plan: &transition::SourceInstallV1,
    source_dir: &PreparedSourceDir,
) -> Result<(), AppError> {
    let rollback_dir = PreparedRollbackDir::open(plan, true)?
        .ok_or_else(|| AppError::blocked("source rollback parent 누락"))?;
    if rollback_dir.open_existing()?.is_some() {
        return rollback_dir.validate(plan);
    }
    let mut target = source_dir
        .open_existing(&source_dir.target)?
        .ok_or_else(|| AppError::blocked("source rollback original 누락"))?;
    let target_metadata = target
        .metadata()
        .map_err(|err| AppError::blocked(format!("source target metadata 실패: {err}")))?;
    let original = read_open_file_bounded(
        &mut target,
        plan.before_byte_length,
        "source rollback original",
    )?;
    if sha256_bytes(&original) != plan.before_sha256
        || u64::try_from(original.len()).ok() != Some(plan.before_byte_length)
    {
        return Err(AppError::blocked(
            "source rollback before blob binding 불일치",
        ));
    }
    let mut file = rollback_dir.create_new()?;
    file.set_permissions(target_metadata.permissions())
        .map_err(|err| AppError::runtime(format!("source rollback permission 적용 실패: {err}")))?;
    file.write_all(&original)
        .map_err(|err| AppError::runtime(format!("source rollback write 실패: {err}")))?;
    file.sync_all()
        .map_err(|err| AppError::runtime(format!("source rollback fsync 실패: {err}")))?;
    drop(file);
    rollback_dir.sync()?;
    rollback_dir.validate(plan)
}

#[cfg(unix)]
fn validate_source_metadata(
    metadata: &fs::Metadata,
    plan: &transition::SourceInstallV1,
    installed: bool,
) -> Result<(), AppError> {
    use std::os::unix::fs::MetadataExt;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(AppError::blocked("source stage type 불일치"));
    }
    let (mode, uid, gid, readonly) = if installed {
        (
            plan.unix_metadata.install_mode,
            plan.unix_metadata.install_uid,
            plan.unix_metadata.install_gid,
            plan.permissions.install_readonly,
        )
    } else {
        (
            plan.unix_metadata.before_mode,
            plan.unix_metadata.before_uid,
            plan.unix_metadata.before_gid,
            plan.permissions.before_readonly,
        )
    };
    if metadata.dev() != plan.unix_metadata.before_dev
        || metadata.mode() != mode
        || metadata.uid() != uid
        || metadata.gid() != gid
        || metadata.permissions().readonly() != readonly
    {
        return Err(AppError::blocked(
            "source stage metadata/parent binding 불일치",
        ));
    }
    if !installed
        && (metadata.dev() != plan.unix_metadata.before_dev
            || metadata.ino() != plan.unix_metadata.before_ino)
    {
        return Err(AppError::blocked("source original dev/ino binding 불일치"));
    }
    Ok(())
}

fn source_replace_fault(point: &str) -> Result<(), AppError> {
    if cfg!(debug_assertions)
        && std::env::var("RPOTATO_TEST_SOURCE_REPLACE_FAULT").as_deref() == Ok(point)
    {
        return Err(AppError::runtime(format!(
            "injected source replacement fault: {point}"
        )));
    }
    Ok(())
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

#[cfg(not(windows))]
fn replace_file(source: &std::path::Path, target: &std::path::Path) -> std::io::Result<()> {
    fs::rename(source, target)
}

#[cfg(windows)]
fn replace_file(source: &std::path::Path, target: &std::path::Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    type Bool = i32;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> Bool;
    }
    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    let source = canonical_windows_parent_join(source)?
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let target = canonical_windows_parent_join(target)?
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // SAFETY: both pointers reference NUL-terminated buffers that remain alive for the call.
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            target.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn canonical_windows_parent_join(path: &std::path::Path) -> std::io::Result<PathBuf> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    let file_name = path.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "replacement path has no file name",
        )
    })?;
    Ok(fs::canonicalize(parent)?.join(file_name))
}

#[cfg(not(windows))]
fn sync_parent(path: &std::path::Path) -> Result<(), AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::runtime("sync parent path 없음"))?;
    File::open(parent)
        .and_then(|file| file.sync_all())
        .map_err(|err| {
            AppError::runtime(format!(
                "parent directory sync 실패: {} ({err})",
                parent.display()
            ))
        })
}

#[cfg(windows)]
fn sync_parent(_path: &std::path::Path) -> Result<(), AppError> {
    Ok(())
}

fn checkpoint_fault(point: &str) -> Result<(), AppError> {
    if cfg!(debug_assertions)
        && std::env::var("RPOTATO_TEST_CHECKPOINT_FAULT").as_deref() == Ok(point)
    {
        return Err(AppError::runtime(format!(
            "injected checkpoint fault: {point}"
        )));
    }
    Ok(())
}

fn state_transition_fault(point: &str) -> Result<(), AppError> {
    #[cfg(debug_assertions)]
    if std::env::var("RPOTATO_TEST_STATE_TRANSITION_FAULT").as_deref() == Ok(point) {
        return Err(AppError::runtime(format!(
            "injected state transition fault: {point}"
        )));
    }
    #[cfg(not(debug_assertions))]
    let _ = point;
    Ok(())
}

fn promotion_fault(point: &str) -> Result<(), AppError> {
    if cfg!(debug_assertions)
        && std::env::var("RPOTATO_TEST_CURRENT_STATE_PROMOTION_FAULT").as_deref() == Ok(point)
    {
        return Err(AppError::runtime(format!(
            "injected current-state promotion fault: {point}"
        )));
    }
    Ok(())
}

fn validate_workflow_id(workflow_id: &str) -> Result<(), AppError> {
    if workflow_id.starts_with("workflow-")
        && workflow_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
    {
        Ok(())
    } else {
        Err(AppError::blocked("workflow id 형식이 안전하지 않습니다."))
    }
}

fn corrupt_workflow(path: &std::path::Path) -> AppError {
    let suppress = SUPPRESS_VALIDATION_GAP_WRITES.with(Cell::get);
    let persistence = if suppress {
        String::new()
    } else {
        record_validation_gap("corrupt-workflow", &path.display().to_string())
            .err()
            .map(|err| format!("\n- validation-gap 저장 실패: {}", err.message))
            .unwrap_or_default()
    };
    AppError::blocked(format!(
        "workflow 읽기 차단\n- 이유: canonical workflow artifact가 손상되었거나 ledger checkpoint와 충돌합니다.\n- path: {}\n- 동작: fail-closed; backend와 side effect를 실행하지 않습니다.{}",
        path.display(), persistence
    ))
}

pub fn record_validation_gap(kind: &str, artifact: &str) -> Result<(), AppError> {
    let path = paths::validation_gaps_file();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            AppError::runtime(format!("validation gap directory 생성 실패: {err}"))
        })?;
    }
    let line = format!(
        "{{\"schema_version\":1,\"kind\":\"{}\",\"artifact_hash\":\"{}\",\"recorded_at_ms\":{}}}",
        ledger::json_string(kind),
        sha256_text(artifact),
        now_ms()
    );
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|err| AppError::runtime(format!("validation gap open 실패: {err}")))?;
    writeln!(file, "{line}")
        .map_err(|err| AppError::runtime(format!("validation gap append 실패: {err}")))?;
    file.sync_all()
        .map_err(|err| AppError::runtime(format!("validation gap sync 실패: {err}")))
}

fn display_empty(value: &str) -> &str {
    if value.is_empty() {
        "none"
    } else {
        value
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn atomic_replace_supports_long_new_and_existing_windows_targets() {
        let root = workflow_test_root("atomic-long-windows");
        let mut parent = root.clone();
        for index in 0..4 {
            parent.push(format!("segment-{index}-{}", "x".repeat(48)));
        }
        fs::create_dir_all(&parent).unwrap();
        let target = parent.join(format!("artifact-{}.json", "y".repeat(48)));
        assert!(target.as_os_str().len() > 260);

        atomic_replace_bytes(&target, b"first").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"first");
        atomic_replace_bytes(&target, b"second").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"second");

        let _ = fs::remove_dir_all(root);
    }

    fn workflow_test_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "rpotato-{name}-{}-{}",
            std::process::id(),
            now_ms()
        ))
    }

    fn with_workflow_env<T>(name: &str, test: impl FnOnce(&PathBuf) -> T) -> T {
        let root = workflow_test_root(name);
        let project = root.join("project");
        fs::create_dir_all(&project).unwrap();
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
        initialize().unwrap();
        let result = test(&root);
        std::env::remove_var("RPOTATO_TEST_CHECKPOINT_FAULT");
        std::env::remove_var("RPOTATO_TEST_STATE_TRANSITION_FAULT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        let _ = fs::remove_dir_all(root);
        result
    }

    #[test]
    fn current_state_summary_handles_missing_file_as_uninitialized() {
        let summary = read_current_state_summary().unwrap();
        assert!(summary == "미초기화" || summary.contains("초기화됨"));
    }

    #[test]
    fn classifies_corrupt_current_state() {
        let identity = RuntimeIdentity {
            project_id: "project-a".to_string(),
            session_id: "session-a".to_string(),
            project_root: ".".to_string(),
        };

        assert_eq!(
            classify_current_state("not-json", &identity),
            CurrentStateStatus::Corrupt
        );
    }

    #[test]
    fn classifies_stale_project_current_state() {
        let identity = RuntimeIdentity {
            project_id: "project-a".to_string(),
            session_id: "session-a".to_string(),
            project_root: ".".to_string(),
        };
        let contents = "{\n  \"schema_version\": 1,\n  \"project_id\": \"project-b\",\n  \"project_root\": \".\",\n  \"session_id\": \"session-a\",\n  \"active_workflow\": null,\n  \"parent_session_id\": null,\n  \"branch_from_event_id\": null,\n  \"compaction_boundary\": null,\n  \"resume_source\": null,\n  \"terminal_states\": [\"complete\", \"failed\", \"cancelled\"]\n}\n";

        assert_eq!(
            classify_current_state(contents, &identity),
            CurrentStateStatus::StaleProject
        );
    }

    #[test]
    fn prepared_workflow_pair_and_single_current_image_are_deterministic() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        with_workflow_env("prepared-workflow-pair", |_| {
            let workflow = create_workflow("prepared workflow pair").unwrap();
            let guard = WorkflowCheckpointGuard::acquire(&workflow.workflow_id).unwrap();
            let current = guard.load_current().unwrap();
            let mut approved = current.clone();
            approved.phase = "approved".to_string();
            approved.approval_state = "approved".to_string();
            let r1 = guard.prepare_revision(&current, approved).unwrap();
            let mut pending = r1.record.clone();
            pending.phase = "pending-verification-approval".to_string();
            pending.approval_state = "applied".to_string();
            pending.verification_approval_state = "pending".to_string();
            let r2 = guard.prepare_revision(&r1.record, pending).unwrap();

            assert_eq!(r1.record.revision, current.revision + 1);
            assert_eq!(r2.record.revision, current.revision + 2);
            assert!(r1.pointer_bytes.ends_with("}\n"));
            assert!(r2
                .pointer_bytes
                .contains(&format!("\"committed_revision\": {}", r2.record.revision)));
            assert_ne!(r1.pointer_member_id, r2.pointer_member_id);
            assert_ne!(r1.snapshot_member_id, r2.snapshot_member_id);

            let before = ledger::validated_ledger_binding().unwrap();
            let final_binding = ledger::LedgerBinding {
                event_count: before.event_count + 10,
                event_id: Some("event-final-prepared".to_string()),
                event_hash: "f".repeat(64),
            };
            let current_image = prepare_current_image(&r2.record, &final_binding).unwrap();
            assert_eq!(
                current_image.revision,
                current_state_lease_view().unwrap().revision + 1
            );
            assert!(current_image.bytes.contains("\"schema_version\":2"));
            assert!(current_image
                .bytes
                .contains(&format!("\"revision\":{}", current_image.revision)));
            assert!(current_image.bytes.contains("event-final-prepared"));
        });
    }

    #[test]
    fn prepared_current_image_rejects_same_revision_different_hash() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        with_workflow_env("prepared-current-cas", |_| {
            let path = paths::current_state_file();
            let body = fs::read_to_string(&path).unwrap();
            let before = parse_current_state(&body, "prepared current CAS before").unwrap();
            let mut forged = before.clone();
            forged.resume_source = Some("concurrent-valid-state".to_string());
            forged.artifact_hash = sha256_text(&render_current_state_v2_payload(&forged));
            let forged_body = render_current_state_v2(&forged);
            fs::write(&path, &forged_body).unwrap();
            let prepared = PreparedCurrentImage {
                path: path.clone(),
                stored_path: "state/current-state.json".to_string(),
                artifact_id: "current-image-future".to_string(),
                bytes: body,
                revision: before.revision + 1,
            };

            let error = install_current_image(&prepared, before.revision, &before.artifact_hash)
                .unwrap_err();

            assert!(error.message.contains("exact CAS conflict"));
            assert_eq!(fs::read_to_string(path).unwrap(), forged_body);
        });
    }

    #[test]
    fn current_state_v2_has_exact_order_hash_and_ledger_binding() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = workflow_test_root("current-state-v2");
        let project = root.join("project");
        fs::create_dir_all(&project).unwrap();
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project);

        initialize().unwrap();
        let body = fs::read_to_string(paths::current_state_file()).unwrap();
        let snapshot = parse_current_state(&body, "current-state v2 fixture").unwrap();
        let lease = current_state_lease_view().unwrap();

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        let _ = fs::remove_dir_all(root);
        assert_eq!(snapshot.schema_version, 2);
        assert_eq!(snapshot.revision, 1);
        assert_eq!(snapshot.previous_artifact_hash, "none");
        assert_eq!(snapshot.ledger_binding.event_count, 1);
        assert_eq!(lease.artifact_hash, snapshot.artifact_hash);
        assert_eq!(body, render_current_state_v2(&snapshot));
    }

    #[test]
    fn exact_v1_is_promoted_once_before_lease() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = workflow_test_root("current-state-v1-promotion");
        let project = root.join("project");
        fs::create_dir_all(&project).unwrap();
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
        ensure_layout().unwrap();
        let identity = ledger::fresh_identity();
        let legacy = format!(
            "{{\n  \"schema_version\": 1,\n  \"project_id\": \"{}\",\n  \"project_root\": \"{}\",\n  \"session_id\": \"{}\",\n  \"active_workflow\": null,\n  \"parent_session_id\": null,\n  \"branch_from_event_id\": null,\n  \"compaction_boundary\": null,\n  \"resume_source\": null,\n  \"terminal_states\": [\"complete\", \"failed\", \"cancelled\"]\n}}\n",
            identity.project_id, identity.project_root, identity.session_id
        );
        fs::write(paths::current_state_file(), &legacy).unwrap();
        let legacy_value = strict_json::parse_value(&legacy, "legacy").unwrap();
        let legacy_hash = sha256_text(&strict_json::render_compact(&legacy_value));

        let first = current_state_lease_view().unwrap();
        let first_body = fs::read_to_string(paths::current_state_file()).unwrap();
        let second = current_state_lease_view().unwrap();
        let second_body = fs::read_to_string(paths::current_state_file()).unwrap();
        let promoted = parse_current_state(&first_body, "promoted").unwrap();

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        let _ = fs::remove_dir_all(root);
        assert_eq!(promoted.schema_version, 2);
        assert_eq!(promoted.revision, 1);
        assert_eq!(promoted.previous_artifact_hash, legacy_hash);
        assert_eq!(first, second);
        assert_eq!(first_body, second_body);
    }

    #[test]
    fn current_state_v1_promotion_crash_matrix_is_idempotent() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        for point in ["after-temp-sync", "after-rename", "after-parent-sync"] {
            let root = workflow_test_root(&format!("current-state-v1-promotion-{point}"));
            let project = root.join("project");
            fs::create_dir_all(&project).unwrap();
            std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
            std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
            ensure_layout().unwrap();
            let identity = ledger::fresh_identity();
            let legacy = format!(
                "{{\"schema_version\":1,\"project_id\":\"{}\",\"project_root\":\"{}\",\"session_id\":\"{}\",\"active_workflow\":null,\"parent_session_id\":null,\"branch_from_event_id\":null,\"compaction_boundary\":null,\"resume_source\":null,\"terminal_states\":[\"complete\",\"failed\",\"cancelled\"]}}",
                identity.project_id, identity.project_root, identity.session_id
            );
            fs::write(paths::current_state_file(), &legacy).unwrap();
            std::env::set_var("RPOTATO_TEST_CURRENT_STATE_PROMOTION_FAULT", point);

            let error = current_state_lease_view().unwrap_err();
            assert!(error
                .message
                .contains("injected current-state promotion fault"));
            std::env::remove_var("RPOTATO_TEST_CURRENT_STATE_PROMOTION_FAULT");

            let first = current_state_lease_view().unwrap();
            let first_body = fs::read_to_string(paths::current_state_file()).unwrap();
            let second = current_state_lease_view().unwrap();
            let second_body = fs::read_to_string(paths::current_state_file()).unwrap();
            let promoted = parse_current_state_v2(&first_body, "promoted restart").unwrap();

            assert_eq!(promoted.revision, 1, "fault point {point}");
            assert_eq!(first, second, "fault point {point}");
            assert_eq!(first_body, second_body, "fault point {point}");
            assert!(!paths::current_state_v2_promotion_temp().exists());
            assert!(!paths::runtime_ledger_file().exists());

            std::env::remove_var("RPOTATO_DATA_HOME");
            std::env::remove_var("RPOTATO_PROJECT_ROOT");
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn corrupt_current_state_blocks_canonical_mutation() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = workflow_test_root("corrupt-state-mutation");
        let project = root.join("project");
        fs::create_dir_all(&project).unwrap();
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
        fs::create_dir_all(paths::state_dir()).unwrap();
        fs::write(paths::current_state_file(), b"not-json").unwrap();

        let event_error = record_event("test.mutation", "blocked", "safe").unwrap_err();
        let workflow_error = create_workflow("must not start").unwrap_err();
        let ledger_exists = paths::runtime_ledger_file().exists();

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        let _ = fs::remove_dir_all(root);
        assert_eq!(event_error.code, 3);
        assert_eq!(workflow_error.code, 3);
        assert!(!ledger_exists);
    }

    #[test]
    fn sqlite_only_session_is_removed_and_cannot_resume() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        with_workflow_env("sqlite-session-authority", |_| {
            let identity = ledger::validated_current_identity().unwrap();
            let connection = rusqlite::Connection::open(paths::observability_db_file()).unwrap();
            connection
                .execute(
                    "INSERT INTO sessions (session_id, project_id, project_root, started_at_ms) VALUES (?1, ?2, ?3, 1)",
                    rusqlite::params!["session-sqlite-only", identity.project_id, identity.project_root],
                )
                .unwrap();
            drop(connection);

            let sessions = observability::session_history(20).unwrap();
            assert!(sessions
                .iter()
                .all(|session| session.session_id != "session-sqlite-only"));
            let error = session_resume_report("session-sqlite-only").unwrap_err();
            assert_eq!(error.code, 3);
            assert!(error.message.contains("canonical runtime ledger"));

            let connection = rusqlite::Connection::open(paths::observability_db_file()).unwrap();
            let count: i64 = connection
                .query_row(
                    "SELECT COUNT(*) FROM sessions WHERE session_id = 'session-sqlite-only'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 0);
        });
    }

    #[test]
    fn session_list_does_not_create_current_state_when_history_is_empty() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-session-list-empty-test-{}",
            std::process::id()
        ));
        let project_root = root.join("project");
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);

        let report = session_list_report().unwrap();
        let current_state_exists = paths::current_state_file().exists();

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");

        assert!(report.contains("sessions: 없음"));
        assert!(!current_state_exists);
    }

    #[test]
    fn session_resume_selects_existing_history_entry() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-session-resume-test-{}",
            std::process::id()
        ));
        let project_root = root.join("project");
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);

        let new_report = session_new_report().unwrap();
        let session_id = new_report
            .lines()
            .find_map(|line| line.strip_prefix("- session id: "))
            .unwrap()
            .to_string();
        let list_report = session_list_report().unwrap();
        let resume_report = session_resume_report(&session_id).unwrap();
        let current_state = fs::read_to_string(paths::current_state_file()).unwrap();

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");

        assert!(list_report.contains(&session_id));
        assert!(resume_report.contains("session resume 결과"));
        assert!(current_state.contains(&format!("\"session_id\":\"{session_id}\"")));
        assert!(current_state.contains("\"resume_source\":\"session-history\""));
    }

    #[test]
    fn tui_session_selection_revalidates_lease_under_lock_and_reuses_receipt() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        with_workflow_env("tui-session-selection-lease", |_| {
            let initial = ledger::validated_current_identity().unwrap();
            session_new_report().unwrap();
            let intent_id = "intent-session-select-exact-0001";
            let lease = crate::tui::canonical_selection_lease(&initial.session_id).unwrap();

            let first = session_resume_report_for_tui(&initial.session_id, intent_id, &lease)
                .unwrap()
                .unwrap();
            let after_first = fs::read_to_string(paths::current_state_file()).unwrap();
            let events_after_first = ledger::read_runtime_events().unwrap();
            let first_receipts = events_after_first
                .iter()
                .filter(|event| {
                    event.event_type == "session.resume.selected"
                        && event.details.contains(&format!("intent_id={intent_id}"))
                })
                .count();

            let retry = session_resume_report_for_tui(&initial.session_id, intent_id, &lease)
                .unwrap()
                .unwrap();
            let after_retry = fs::read_to_string(paths::current_state_file()).unwrap();
            let retry_receipts = ledger::read_runtime_events()
                .unwrap()
                .into_iter()
                .filter(|event| {
                    event.event_type == "session.resume.selected"
                        && event.details.contains(&format!("intent_id={intent_id}"))
                })
                .count();

            assert_eq!(first, retry);
            assert_eq!(after_first, after_retry);
            assert_eq!(first_receipts, 1);
            assert_eq!(retry_receipts, 1);

            let stale_lease = crate::tui::canonical_selection_lease(&initial.session_id).unwrap();
            record_event("test.selection.predecessor", "advance predecessor", "safe").unwrap();
            let before_stale_events = ledger::read_runtime_events().unwrap().len();
            assert!(session_resume_report_for_tui(
                &initial.session_id,
                "intent-session-select-stale-0002",
                &stale_lease,
            )
            .unwrap()
            .is_none());
            assert_eq!(
                ledger::read_runtime_events().unwrap().len(),
                before_stale_events
            );
        });
    }

    #[test]
    fn bootstrap_creation_crash_matrix_is_idempotent() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        for point in [
            "after-journal",
            "after-artifacts",
            "after-ledger",
            "after-current",
            "after-projection",
        ] {
            let root = workflow_test_root(&format!("bootstrap-writer-{point}"));
            let project = root.join("project");
            fs::create_dir_all(&project).unwrap();
            std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
            std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
            std::env::set_var("RPOTATO_TEST_STATE_TRANSITION_FAULT", point);

            let error = initialize().unwrap_err();
            assert!(error.message.contains(point));
            std::env::remove_var("RPOTATO_TEST_STATE_TRANSITION_FAULT");
            let first = initialize().unwrap();
            let first_current = fs::read(paths::current_state_file()).unwrap();
            let first_events = ledger::read_runtime_events().unwrap();
            let second = initialize().unwrap();

            assert_eq!(first.identity.project_id, second.identity.project_id);
            assert_eq!(
                fs::read(paths::current_state_file()).unwrap(),
                first_current
            );
            assert_eq!(ledger::read_runtime_events().unwrap(), first_events);
            assert_eq!(
                first_events
                    .iter()
                    .filter(|event| event.event_type == "runtime.init")
                    .count(),
                1,
                "fault point: {point}"
            );
            assert_eq!(current_state_lease_view().unwrap().revision, 1);

            std::env::remove_var("RPOTATO_DATA_HOME");
            std::env::remove_var("RPOTATO_PROJECT_ROOT");
            let _ = fs::remove_dir_all(root);
        }

        let root = workflow_test_root("bootstrap-writer-race");
        let project = root.join("project");
        fs::create_dir_all(&project).unwrap();
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
        let first = std::thread::spawn(initialize);
        let second = std::thread::spawn(initialize);
        first.join().unwrap().unwrap();
        second.join().unwrap().unwrap();
        let events = ledger::read_runtime_events().unwrap();
        assert_eq!(
            events
                .iter()
                .filter(|event| event.event_type == "runtime.init")
                .count(),
            1
        );
        assert_eq!(current_state_lease_view().unwrap().revision, 1);
        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn session_new_crash_race_restart_is_single_commit() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        for point in [
            "after-journal",
            "after-artifacts",
            "after-ledger",
            "after-current",
            "after-projection",
        ] {
            with_workflow_env(&format!("session-new-writer-{point}"), |_| {
                let before = current_state_lease_view().unwrap();
                let before_events = ledger::read_runtime_events().unwrap();
                let intent_id = format!("intent-session-new-crash-{point}");
                std::env::set_var("RPOTATO_TEST_STATE_TRANSITION_FAULT", point);
                let error = session_new_report_for_intent(&intent_id).unwrap_err();
                assert!(error.message.contains(point));
                std::env::remove_var("RPOTATO_TEST_STATE_TRANSITION_FAULT");

                let first = session_new_report_for_intent(&intent_id).unwrap();
                let current = fs::read(paths::current_state_file()).unwrap();
                let events = ledger::read_runtime_events().unwrap();
                let retry = session_new_report_for_intent(&intent_id).unwrap();

                assert_eq!(first, retry, "fault point: {point}");
                assert_eq!(fs::read(paths::current_state_file()).unwrap(), current);
                assert_eq!(ledger::read_runtime_events().unwrap(), events);
                assert_eq!(
                    current_state_lease_view().unwrap().revision,
                    before.revision + 1
                );
                assert_eq!(events.len(), before_events.len() + 1);
                assert_eq!(
                    events
                        .iter()
                        .filter(|event| {
                            event.event_type == "session.new"
                                && tui_detail_value(&event.details, "intent_id")
                                    == Some(intent_id.as_str())
                        })
                        .count(),
                    1
                );
            });
        }

        with_workflow_env("session-new-writer-race", |_| {
            let identity = ledger::validated_current_identity().unwrap();
            let transition = transition::TransitionGuard::acquire_for(
                &identity.project_id,
                transition::CurrentStateIntent::RecordEvent,
            )
            .unwrap();
            let first = std::thread::spawn(|| {
                session_new_report_for_intent("intent-session-new-race-first")
            });
            let second = std::thread::spawn(|| {
                session_new_report_for_intent("intent-session-new-race-second")
            });
            std::thread::sleep(Duration::from_millis(100));
            drop(transition);
            let results = [first.join().unwrap(), second.join().unwrap()];
            assert_eq!(
                results.iter().filter(|result| result.is_ok()).count(),
                1,
                "session new race results: {results:?}"
            );
            assert_eq!(
                results
                    .iter()
                    .filter(|result| result
                        .as_ref()
                        .is_err_and(|error| error.message.contains("stale predecessor")))
                    .count(),
                1,
                "session new race results: {results:?}"
            );
            assert_eq!(
                ledger::read_runtime_events()
                    .unwrap()
                    .iter()
                    .filter(|event| event.event_type == "session.new")
                    .count(),
                1
            );
        });
    }

    #[test]
    fn session_resume_transaction_never_exposes_current_before_ledger() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        for point in [
            "after-journal",
            "after-artifacts",
            "after-ledger",
            "after-current",
            "after-projection",
        ] {
            with_workflow_env(&format!("session-resume-writer-{point}"), |_| {
                let target = ledger::validated_current_identity().unwrap();
                session_new_report_for_intent(&format!("intent-session-new-before-{point}"))
                    .unwrap();
                let before_current = fs::read(paths::current_state_file()).unwrap();
                let intent_id = format!("intent-session-resume-crash-{point}");
                let lease = crate::tui::canonical_selection_lease(&target.session_id).unwrap();
                std::env::set_var("RPOTATO_TEST_STATE_TRANSITION_FAULT", point);

                let error = session_resume_report_for_tui(&target.session_id, &intent_id, &lease)
                    .unwrap_err();
                assert!(error.message.contains(point));
                std::env::remove_var("RPOTATO_TEST_STATE_TRANSITION_FAULT");
                let events_after_fault = ledger::read_runtime_events().unwrap();
                let event_is_durable = events_after_fault.iter().any(|event| {
                    event.event_type == "session.resume.selected"
                        && tui_detail_value(&event.details, "intent_id") == Some(intent_id.as_str())
                });
                if !event_is_durable {
                    assert_eq!(
                        fs::read(paths::current_state_file()).unwrap(),
                        before_current
                    );
                }

                let first = session_resume_report_for_tui(&target.session_id, &intent_id, &lease)
                    .unwrap()
                    .unwrap();
                let committed_current = fs::read(paths::current_state_file()).unwrap();
                let committed_events = ledger::read_runtime_events().unwrap();
                let retry = session_resume_report_for_tui(&target.session_id, &intent_id, &lease)
                    .unwrap()
                    .unwrap();
                let snapshot = parse_current_state(
                    std::str::from_utf8(&committed_current).unwrap(),
                    "session resume committed current",
                )
                .unwrap();

                assert_eq!(first, retry);
                assert_eq!(snapshot.session_id, target.session_id);
                assert_eq!(
                    fs::read(paths::current_state_file()).unwrap(),
                    committed_current
                );
                assert_eq!(ledger::read_runtime_events().unwrap(), committed_events);
                assert_eq!(
                    committed_events
                        .iter()
                        .filter(|event| {
                            event.event_type == "session.resume.selected"
                                && tui_detail_value(&event.details, "intent_id")
                                    == Some(intent_id.as_str())
                        })
                        .count(),
                    1
                );
            });
        }
    }

    #[test]
    fn low_level_writer_recovery_is_idempotent() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        for point in [
            "after-journal",
            "after-artifacts",
            "after-ledger",
            "after-current",
            "after-projection",
        ] {
            with_workflow_env(&format!("ordinary-state-transition-{point}"), |_| {
                let before_current = current_state_lease_view().unwrap();
                let before_events = ledger::read_runtime_events().unwrap();
                std::env::set_var("RPOTATO_TEST_STATE_TRANSITION_FAULT", point);

                let error = record_event(
                    "test.state-transition.crash",
                    "state transition crash matrix",
                    &format!("point={point}"),
                )
                .unwrap_err();

                std::env::remove_var("RPOTATO_TEST_STATE_TRANSITION_FAULT");
                assert!(error.message.contains(point));
                let identity = ledger::validated_current_identity().unwrap();
                let journal_dir = paths::project_transition_journal_dir(&identity.project_id);
                assert_eq!(
                    fs::read_dir(&journal_dir)
                        .unwrap()
                        .filter_map(Result::ok)
                        .filter(|entry| {
                            entry
                                .file_name()
                                .to_str()
                                .is_some_and(|name| name.ends_with(".prepared.json"))
                        })
                        .count(),
                    1,
                    "point: {point}"
                );

                assert_eq!(
                    transition::recover_pending_source_bundles().unwrap(),
                    1,
                    "point: {point}"
                );
                let after_current = current_state_lease_view().unwrap();
                let after_events = ledger::read_runtime_events().unwrap();
                assert_eq!(after_current.revision, before_current.revision + 1);
                assert_eq!(after_events.len(), before_events.len() + 1);
                assert_eq!(
                    after_events
                        .iter()
                        .filter(|event| event.event_type == "test.state-transition.crash")
                        .count(),
                    1
                );
                assert_eq!(transition::recover_pending_source_bundles().unwrap(), 0);
                assert_eq!(current_state_lease_view().unwrap(), after_current);
                assert_eq!(ledger::read_runtime_events().unwrap(), after_events);
            });
        }
    }

    #[test]
    fn workflow_checkpoint_writer_crash_matrix() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        for point in [
            "after-transaction",
            "after-snapshot",
            "after-ledger",
            "after-pointer",
        ] {
            with_workflow_env(point, |_| {
                std::env::set_var("RPOTATO_TEST_CHECKPOINT_FAULT", point);
                let error = create_workflow("recover me").unwrap_err();
                assert!(
                    error.message.contains("injected checkpoint fault"),
                    "fault point {point}: {}",
                    error.message
                );
                std::env::remove_var("RPOTATO_TEST_CHECKPOINT_FAULT");

                let workflow_id = active_workflow_id().unwrap().unwrap();
                let workflow = load_workflow(&workflow_id).unwrap();
                let checkpoints = ledger::workflow_checkpoints(&workflow_id).unwrap();
                let pointer = fs::read(paths::project_workflow_file(&workflow_id)).unwrap();
                let current = fs::read(paths::current_state_file()).unwrap();
                let events = ledger::read_runtime_events().unwrap();
                assert_eq!(workflow.revision, 1, "fault point: {point}");
                assert_eq!(checkpoints.len(), 1, "fault point: {point}");
                assert!(!paths::project_workflow_transaction_file(&workflow_id).exists());
                assert_eq!(active_workflow_id().unwrap(), Some(workflow_id.clone()));
                assert_eq!(load_workflow(&workflow_id).unwrap(), workflow);
                assert_eq!(
                    fs::read(paths::project_workflow_file(&workflow_id)).unwrap(),
                    pointer
                );
                assert_eq!(fs::read(paths::current_state_file()).unwrap(), current);
                assert_eq!(ledger::read_runtime_events().unwrap(), events);
            });
        }
    }

    #[test]
    fn workflow_recovery_replays_only_prepared_suffix() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        for point in [
            "after-transaction",
            "after-snapshot",
            "after-ledger",
            "after-pointer",
        ] {
            with_workflow_env(&format!("workflow-replay-{point}"), |_| {
                let first = create_workflow("prepared suffix replay").unwrap();
                let mut next = first.clone();
                next.result_summary = format!("prepared-{point}");
                std::env::set_var("RPOTATO_TEST_CHECKPOINT_FAULT", point);
                let error = checkpoint_workflow(next, first.revision).unwrap_err();
                assert!(error.message.contains(point));
                std::env::remove_var("RPOTATO_TEST_CHECKPOINT_FAULT");

                let recovered = load_workflow(&first.workflow_id).unwrap();
                let pointer = fs::read(paths::project_workflow_file(&first.workflow_id)).unwrap();
                let snapshot = fs::read(paths::project_workflow_snapshot_file(
                    &first.workflow_id,
                    recovered.revision,
                ))
                .unwrap();
                let events = ledger::read_runtime_events().unwrap();
                assert_eq!(recovered.revision, 2);
                assert_eq!(recovered.result_summary, format!("prepared-{point}"));
                assert_eq!(load_workflow(&first.workflow_id).unwrap(), recovered);
                assert_eq!(
                    fs::read(paths::project_workflow_file(&first.workflow_id)).unwrap(),
                    pointer
                );
                assert_eq!(
                    fs::read(paths::project_workflow_snapshot_file(
                        &first.workflow_id,
                        recovered.revision
                    ))
                    .unwrap(),
                    snapshot
                );
                assert_eq!(ledger::read_runtime_events().unwrap(), events);
                assert!(!paths::project_workflow_transaction_file(&first.workflow_id).exists());
            });
        }

        with_workflow_env("workflow-replay-tamper", |_| {
            let first = create_workflow("tampered prepared suffix").unwrap();
            let mut next = first.clone();
            next.result_summary = "must-not-install".to_string();
            std::env::set_var("RPOTATO_TEST_CHECKPOINT_FAULT", "after-transaction");
            checkpoint_workflow(next, first.revision).unwrap_err();
            std::env::remove_var("RPOTATO_TEST_CHECKPOINT_FAULT");
            let identity = ledger::validated_current_identity().unwrap();
            let journal_dir = paths::project_transition_journal_dir(&identity.project_id);
            let journal = fs::read_dir(&journal_dir)
                .unwrap()
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .find(|path| {
                    path.file_name()
                        .and_then(|value| value.to_str())
                        .is_some_and(|name| name.ends_with(".prepared.json"))
                })
                .unwrap();
            let mut bytes = fs::read(&journal).unwrap();
            let index = bytes.len() / 2;
            bytes[index] ^= 1;
            fs::write(&journal, &bytes).unwrap();
            let before_events = ledger::read_runtime_events().unwrap();
            let pointer = fs::read(paths::project_workflow_file(&first.workflow_id)).unwrap();

            assert!(load_workflow(&first.workflow_id).is_err());
            assert_eq!(ledger::read_runtime_events().unwrap(), before_events);
            assert_eq!(
                fs::read(paths::project_workflow_file(&first.workflow_id)).unwrap(),
                pointer
            );
            assert!(journal.exists());
        });
    }

    #[test]
    fn active_workflow_pointer_recovery_is_single_and_idempotent() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        for point in [
            "after-journal",
            "after-artifacts",
            "after-ledger",
            "after-current",
            "after-projection",
        ] {
            with_workflow_env(&format!("active-pointer-recovery-{point}"), |_| {
                let workflow = create_workflow("recover active pointer").unwrap();
                let current_path = paths::current_state_file();
                let body = fs::read_to_string(&current_path).unwrap();
                let mut detached = parse_current_state(&body, "detached active pointer").unwrap();
                detached.active_workflow = None;
                detached.artifact_hash = sha256_text(&render_current_state_v2_payload(&detached));
                fs::write(&current_path, render_current_state_v2(&detached)).unwrap();
                std::env::set_var("RPOTATO_TEST_STATE_TRANSITION_FAULT", point);

                let error = active_workflow_id().unwrap_err();
                assert!(error.message.contains(point));
                std::env::remove_var("RPOTATO_TEST_STATE_TRANSITION_FAULT");
                assert_eq!(
                    active_workflow_id().unwrap(),
                    Some(workflow.workflow_id.clone())
                );
                let current = fs::read(&current_path).unwrap();
                let events = ledger::read_runtime_events().unwrap();
                assert_eq!(
                    active_workflow_id().unwrap(),
                    Some(workflow.workflow_id.clone())
                );
                assert_eq!(fs::read(&current_path).unwrap(), current);
                assert_eq!(ledger::read_runtime_events().unwrap(), events);
                assert_eq!(
                    events
                        .iter()
                        .filter(|event| event.event_type == "workflow.pointer.recovered")
                        .count(),
                    1
                );
            });
        }

        with_workflow_env("active-pointer-recovery-zero", |_| {
            let before = ledger::read_runtime_events().unwrap();
            assert_eq!(active_workflow_id().unwrap(), None);
            assert_eq!(ledger::read_runtime_events().unwrap(), before);
        });
    }

    #[test]
    fn terminal_pointer_cleanup_crash_race_restart_is_idempotent() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        for point in [
            "after-journal",
            "after-artifacts",
            "after-ledger",
            "after-current",
            "after-projection",
        ] {
            with_workflow_env(&format!("terminal-cleanup-{point}"), |_| {
                let first = create_workflow("terminal cleanup").unwrap();
                let mut terminal = first.clone();
                terminal.phase = "cancelled".to_string();
                terminal.failure_reason = "cancelled-before-side-effect".to_string();
                let terminal = checkpoint_workflow(terminal, first.revision).unwrap();
                std::env::set_var("RPOTATO_TEST_STATE_TRANSITION_FAULT", point);

                let error = clear_terminal_workflow_pointer(&terminal).unwrap_err();
                assert!(error.message.contains(point));
                std::env::remove_var("RPOTATO_TEST_STATE_TRANSITION_FAULT");
                clear_terminal_workflow_pointer(&terminal).unwrap();
                let current = fs::read(paths::current_state_file()).unwrap();
                let events = ledger::read_runtime_events().unwrap();
                clear_terminal_workflow_pointer(&terminal).unwrap();
                let snapshot = parse_current_state(
                    std::str::from_utf8(&current).unwrap(),
                    "terminal cleanup committed current",
                )
                .unwrap();

                assert!(snapshot.active_workflow.is_none());
                assert_eq!(fs::read(paths::current_state_file()).unwrap(), current);
                assert_eq!(ledger::read_runtime_events().unwrap(), events);
                assert_eq!(
                    events
                        .iter()
                        .filter(|event| event.event_type == "workflow.pointer.cleared")
                        .count(),
                    1
                );
                assert!(clear_terminal_workflow_pointer(&first).is_err());
            });
        }

        with_workflow_env("terminal-cleanup-race", |_| {
            let first = create_workflow("terminal cleanup race").unwrap();
            let mut terminal = first.clone();
            terminal.phase = "cancelled".to_string();
            terminal.failure_reason = "cancelled-before-side-effect".to_string();
            let terminal = checkpoint_workflow(terminal, first.revision).unwrap();
            let identity = ledger::validated_current_identity().unwrap();
            let transition = transition::TransitionGuard::acquire_for(
                &identity.project_id,
                transition::CurrentStateIntent::RecordEvent,
            )
            .unwrap();
            let cleanup = std::thread::spawn(move || clear_terminal_workflow_pointer(&terminal));
            let create = std::thread::spawn(|| create_workflow("new workflow after terminal"));
            std::thread::sleep(Duration::from_millis(100));
            drop(transition);
            let cleanup_result = cleanup.join().unwrap();
            let created = create.join().unwrap().unwrap();
            let active = active_workflow_id().unwrap();
            assert_eq!(active, Some(created.workflow_id));
            if let Err(error) = cleanup_result {
                assert!(error.message.contains("pointer conflict"));
            }
        });
    }

    #[test]
    fn reconcile_writer_crash_matrix_preserves_evidence() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        for point in [
            "after-journal",
            "after-artifacts",
            "after-ledger",
            "after-current",
            "after-projection",
        ] {
            with_workflow_env(&format!("reconcile-writer-{point}"), |_| {
                let corrupt = format!("corrupt-current-evidence-{point}\n");
                fs::write(paths::current_state_file(), &corrupt).unwrap();
                std::env::set_var("RPOTATO_TEST_STATE_TRANSITION_FAULT", point);

                let error = reconcile_report().unwrap_err();
                assert!(error.message.contains(point));
                std::env::remove_var("RPOTATO_TEST_STATE_TRANSITION_FAULT");
                reconcile_report().unwrap();
                let current = fs::read(paths::current_state_file()).unwrap();
                let events = ledger::read_runtime_events().unwrap();
                let backups = fs::read_dir(paths::state_dir())
                    .unwrap()
                    .filter_map(Result::ok)
                    .filter(|entry| {
                        entry
                            .file_name()
                            .to_str()
                            .is_some_and(|name| name.starts_with("current-state.json.corrupt."))
                    })
                    .collect::<Vec<_>>();

                assert_eq!(backups.len(), 1, "fault point: {point}");
                assert_eq!(fs::read_to_string(backups[0].path()).unwrap(), corrupt);
                assert_eq!(
                    events
                        .iter()
                        .filter(|event| event.event_type == "state.reconcile.corrupt_recovered")
                        .count(),
                    1
                );
                reconcile_report().unwrap();
                assert_eq!(fs::read(paths::current_state_file()).unwrap(), current);
                assert_eq!(ledger::read_runtime_events().unwrap(), events);
            });
        }

        let root = workflow_test_root("reconcile-writer-missing");
        let project = root.join("project");
        fs::create_dir_all(&project).unwrap();
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
        let first = reconcile_report().unwrap();
        let current = fs::read(paths::current_state_file()).unwrap();
        let events = ledger::read_runtime_events().unwrap();
        let second = reconcile_report().unwrap();
        assert!(first.contains("created"));
        assert!(second.contains("current-state 정상"));
        assert_eq!(fs::read(paths::current_state_file()).unwrap(), current);
        assert_eq!(ledger::read_runtime_events().unwrap(), events);
        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_v2_chain_is_preserved_and_next_checkpoint_appends_v4() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        with_workflow_env("workflow-v2-upgrade", |_| {
            let mut legacy =
                WorkflowRecord::new(&ledger::fresh_identity(), "legacy pending workflow");
            legacy.revision = 1;
            legacy.previous_hash = "none".to_string();
            legacy.phase = "pending-approval".to_string();
            legacy.approval_state = "pending".to_string();
            legacy.artifact_hash = sha256_text(&workflow_payload_v2(&legacy));
            let snapshot = paths::project_workflow_snapshot_file(&legacy.workflow_id, 1);
            atomic_replace_bytes(&snapshot, render_workflow_v2(&legacy).as_bytes()).unwrap();
            append_workflow_checkpoint_event(&legacy).unwrap();
            write_workflow_pointer_for_schema(&legacy, LEGACY_WORKFLOW_SCHEMA_VERSION).unwrap();
            let legacy_bytes = fs::read(&snapshot).unwrap();

            let mut loaded = load_workflow(&legacy.workflow_id).unwrap();
            assert_eq!(loaded.revision, 1);
            assert_eq!(loaded.verification_approval_state, "not-issued");
            loaded.result_summary = "v2 workflow upgraded".to_string();
            let upgraded = checkpoint_workflow(loaded.clone(), loaded.revision).unwrap();

            assert_eq!(upgraded.revision, 2);
            assert_eq!(upgraded.previous_hash, legacy.artifact_hash);
            assert_eq!(fs::read(&snapshot).unwrap(), legacy_bytes);
            let pointer =
                fs::read_to_string(paths::project_workflow_file(&legacy.workflow_id)).unwrap();
            assert!(pointer.contains("\"schema_version\": 4"));
            assert!(pointer.contains("workflow-commit-v4"));
            let v4 = fs::read_to_string(paths::project_workflow_snapshot_file(
                &legacy.workflow_id,
                2,
            ))
            .unwrap();
            assert!(v4.contains("\"artifact_version\": \"workflow-v4\""));
            assert_eq!(load_workflow(&legacy.workflow_id).unwrap(), upgraded);
        });
    }

    #[test]
    fn v3_loads_without_rewrite_and_next_checkpoint_persists_skill_state_as_v4() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        with_workflow_env("workflow-v3-upgrade", |_| {
            let mut v3 = WorkflowRecord::new(&ledger::fresh_identity(), "v3 workflow");
            v3.revision = 1;
            v3.previous_hash = "none".to_string();
            v3.phase = "model-pending".to_string();
            v3.artifact_hash = sha256_text(&workflow_payload_v3(&v3));
            let snapshot = paths::project_workflow_snapshot_file(&v3.workflow_id, 1);
            let v3_bytes = render_workflow_v3(&v3);
            atomic_replace_bytes(&snapshot, v3_bytes.as_bytes()).unwrap();
            append_workflow_checkpoint_event(&v3).unwrap();
            write_workflow_pointer_for_schema(&v3, PREVIOUS_WORKFLOW_SCHEMA_VERSION).unwrap();

            let mut loaded = load_workflow(&v3.workflow_id).unwrap();
            assert_eq!(fs::read_to_string(&snapshot).unwrap(), v3_bytes);
            assert!(loaded.active_skill_id.is_empty());
            assert!(loaded.skill_state.is_empty());

            loaded.active_skill_id = "built-in-plan".to_string();
            loaded.skill_invocation = "$plan --consensus".to_string();
            loaded.skill_state = "running".to_string();
            loaded.skill_completed_hooks = "session-start,preflight".to_string();
            loaded.skill_evidence = "artifact:plan-v1".to_string();
            loaded.skill_stop_criteria = "verified".to_string();
            let checkpointed = checkpoint_workflow(loaded.clone(), loaded.revision).unwrap();
            let restarted = load_workflow(&v3.workflow_id).unwrap();

            assert_eq!(restarted, checkpointed);
            assert_eq!(restarted.active_skill_id, "built-in-plan");
            assert_eq!(restarted.skill_invocation, "$plan --consensus");
            assert_eq!(restarted.skill_state, "running");
            assert_eq!(restarted.skill_completed_hooks, "session-start,preflight");
            assert_eq!(restarted.skill_evidence, "artifact:plan-v1");
            assert_eq!(restarted.skill_stop_criteria, "verified");
            assert_eq!(fs::read_to_string(&snapshot).unwrap(), v3_bytes);
            let pointer =
                fs::read_to_string(paths::project_workflow_file(&v3.workflow_id)).unwrap();
            assert!(pointer.contains("workflow-commit-v4"));
        });
    }

    #[test]
    fn legacy_v2_complete_maps_split_approval_evidence_without_rewriting() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        with_workflow_env("workflow-v2-complete-map", |root| {
            let mut non_mutating = WorkflowRecord::new(
                &ledger::fresh_identity(),
                "legacy read-only complete workflow",
            );
            non_mutating.revision = 1;
            non_mutating.previous_hash = "none".to_string();
            non_mutating.phase = "complete".to_string();
            non_mutating.action_kind = "inspect-sources".to_string();
            non_mutating.approval_state = "not-required".to_string();
            non_mutating.artifact_hash = sha256_text(&workflow_payload_v2(&non_mutating));
            let parsed_non_mutating = parse_workflow_snapshot(
                &root.join("non-mutating-v2.json"),
                &render_workflow_v2(&non_mutating),
            )
            .unwrap();
            assert_eq!(parsed_non_mutating.approval_state, "not-required");
            assert_eq!(
                parsed_non_mutating.verification_approval_state,
                "not-issued"
            );

            let mut legacy =
                WorkflowRecord::new(&ledger::fresh_identity(), "legacy complete workflow");
            legacy.revision = 1;
            legacy.previous_hash = "none".to_string();
            legacy.phase = "complete".to_string();
            legacy.action_kind = "patch-proposal".to_string();
            legacy.approval_state = "approved".to_string();
            legacy.proposal_id = "patch-proposal-legacy".to_string();
            legacy.source_path = "src/lib.rs".to_string();
            legacy.after_hash = "a".repeat(64);
            legacy.evidence_id = "evidence-legacy".to_string();
            legacy.artifact_hash = sha256_text(&workflow_payload_v2(&legacy));
            let snapshot = paths::project_workflow_snapshot_file(&legacy.workflow_id, 1);
            let bytes = render_workflow_v2(&legacy);
            atomic_replace_bytes(&snapshot, bytes.as_bytes()).unwrap();
            append_workflow_checkpoint_event(&legacy).unwrap();
            write_workflow_pointer_for_schema(&legacy, LEGACY_WORKFLOW_SCHEMA_VERSION).unwrap();

            let loaded = load_workflow(&legacy.workflow_id).unwrap();

            assert_eq!(loaded.phase, "complete");
            assert_eq!(loaded.approval_state, "applied");
            assert_eq!(loaded.verification_approval_state, "approved");
            assert_eq!(fs::read_to_string(snapshot).unwrap(), bytes);
        });
    }

    #[test]
    fn interrupted_legacy_v2_transaction_without_prepared_event_fails_closed() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        with_workflow_env("workflow-v2-transaction", |_| {
            let mut first =
                WorkflowRecord::new(&ledger::fresh_identity(), "legacy transaction workflow");
            first.revision = 1;
            first.previous_hash = "none".to_string();
            first.phase = "pending-approval".to_string();
            first.approval_state = "pending".to_string();
            first.artifact_hash = sha256_text(&workflow_payload_v2(&first));
            atomic_replace_bytes(
                &paths::project_workflow_snapshot_file(&first.workflow_id, 1),
                render_workflow_v2(&first).as_bytes(),
            )
            .unwrap();
            append_workflow_checkpoint_event(&first).unwrap();
            write_workflow_pointer_for_schema(&first, LEGACY_WORKFLOW_SCHEMA_VERSION).unwrap();

            let mut second = first.clone();
            second.revision = 2;
            second.previous_hash = first.artifact_hash.clone();
            second.phase = "verification-started".to_string();
            second.approval_state = "approved".to_string();
            second.proposal_id = "patch-proposal-legacy-transaction".to_string();
            second.artifact_hash = sha256_text(&workflow_payload_v2(&second));
            let transaction = render_workflow_v2(&second);
            atomic_replace_bytes(
                &paths::project_workflow_transaction_file(&second.workflow_id),
                transaction.as_bytes(),
            )
            .unwrap();

            let error = load_workflow(&second.workflow_id).unwrap_err();

            assert!(error.message.contains("exact prepared semantic event"));
            assert!(!paths::project_workflow_snapshot_file(&second.workflow_id, 2).exists());
            let pointer =
                fs::read_to_string(paths::project_workflow_file(&second.workflow_id)).unwrap();
            assert!(pointer.contains("workflow-commit-v2"));
            assert_eq!(
                fs::read_to_string(paths::project_workflow_transaction_file(
                    &second.workflow_id
                ))
                .unwrap(),
                transaction
            );
            assert_eq!(
                ledger::workflow_checkpoints(&second.workflow_id)
                    .unwrap()
                    .len(),
                1
            );
        });
    }

    #[test]
    fn workflow_recovery_rejects_unbound_previous_hash_before_append() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        with_workflow_env("workflow-recovery-binding", |_| {
            let mut first =
                WorkflowRecord::new(&ledger::fresh_identity(), "recovery binding workflow");
            first.revision = 1;
            first.previous_hash = "none".to_string();
            first.artifact_hash = sha256_text(&workflow_payload_v2(&first));
            atomic_replace_bytes(
                &paths::project_workflow_snapshot_file(&first.workflow_id, 1),
                render_workflow_v2(&first).as_bytes(),
            )
            .unwrap();
            append_workflow_checkpoint_event(&first).unwrap();
            write_workflow_pointer_for_schema(&first, LEGACY_WORKFLOW_SCHEMA_VERSION).unwrap();

            let mut forged = first.clone();
            forged.revision = 2;
            forged.previous_hash = "f".repeat(64);
            forged.artifact_hash = sha256_text(&workflow_payload(&forged));
            atomic_replace_bytes(
                &paths::project_workflow_transaction_file(&forged.workflow_id),
                render_workflow(&forged).as_bytes(),
            )
            .unwrap();

            let error = load_workflow(&forged.workflow_id).unwrap_err();

            assert_eq!(error.code, 3);
            assert!(!paths::project_workflow_snapshot_file(&forged.workflow_id, 2).exists());
            assert_eq!(
                ledger::workflow_checkpoints(&forged.workflow_id)
                    .unwrap()
                    .len(),
                1
            );
        });
    }

    #[test]
    fn workflow_chain_rejects_v3_to_v2_schema_downgrade() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        with_workflow_env("workflow-schema-downgrade", |_| {
            let mut first =
                WorkflowRecord::new(&ledger::fresh_identity(), "schema downgrade workflow");
            first.revision = 1;
            first.previous_hash = "none".to_string();
            first.artifact_hash = sha256_text(&workflow_payload_v3(&first));
            atomic_replace_bytes(
                &paths::project_workflow_snapshot_file(&first.workflow_id, 1),
                render_workflow_v3(&first).as_bytes(),
            )
            .unwrap();
            append_workflow_checkpoint_event(&first).unwrap();
            write_workflow_pointer_for_schema(&first, PREVIOUS_WORKFLOW_SCHEMA_VERSION).unwrap();
            let mut downgraded = first.clone();
            downgraded.revision = 2;
            downgraded.previous_hash = first.artifact_hash.clone();
            downgraded.artifact_hash = sha256_text(&workflow_payload_v2(&downgraded));
            atomic_replace_bytes(
                &paths::project_workflow_snapshot_file(&downgraded.workflow_id, 2),
                render_workflow_v2(&downgraded).as_bytes(),
            )
            .unwrap();
            append_workflow_checkpoint_event(&downgraded).unwrap();
            write_workflow_pointer_for_schema(&downgraded, LEGACY_WORKFLOW_SCHEMA_VERSION).unwrap();

            let error = load_workflow(&downgraded.workflow_id).unwrap_err();

            assert_eq!(error.code, 3);
            assert!(error.message.contains("fail-closed"));
        });
    }

    #[test]
    fn terminal_pointer_cleanup_revalidates_stop_gate_before_clear() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        with_workflow_env("terminal-pointer-cleanup", |_| {
            let mut workflow = create_workflow("finish me").unwrap();
            workflow.phase = "complete".to_string();
            std::env::set_var("RPOTATO_TEST_CHECKPOINT_FAULT", "after-pointer");
            checkpoint_workflow(workflow.clone(), workflow.revision).unwrap_err();
            std::env::remove_var("RPOTATO_TEST_CHECKPOINT_FAULT");

            assert_eq!(
                active_workflow_id().unwrap(),
                Some(workflow.workflow_id.clone())
            );
            let error = resume_report().unwrap_err();
            assert!(error.message.contains("proposal"));
            let current = fs::read_to_string(paths::current_state_file()).unwrap();
            assert!(current.contains(&workflow.workflow_id));
            assert!(load_workflow(&workflow.workflow_id).unwrap().is_terminal());
        });
    }

    #[test]
    fn all_artifacts_are_scanned_and_multiple_active_workflows_fail_closed() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        with_workflow_env("multi-active", |_| {
            let first = create_workflow("first").unwrap();
            let second = create_workflow("second").unwrap();
            assert_ne!(first.workflow_id, second.workflow_id);

            let error = active_workflow_id().unwrap_err();
            assert_eq!(error.code, 3);
            assert!(error.message.contains("여러 non-terminal"));
        });
    }

    #[test]
    fn state_status_reports_the_discovered_active_workflow() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        with_workflow_env("status-active", |_| {
            let workflow = create_workflow("status truth").unwrap();
            let report = status_report().unwrap();
            assert!(report.contains(&format!("active workflow: {}", workflow.workflow_id)));
        });
    }

    #[test]
    fn snapshot_tamper_fails_closed() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        with_workflow_env("snapshot-tamper", |_| {
            let workflow = create_workflow("tamper me").unwrap();
            let snapshot = paths::project_workflow_snapshot_file(&workflow.workflow_id, 1);
            let mut body = fs::read_to_string(&snapshot).unwrap();
            body = body.replace("model-pending", "approved");
            fs::write(&snapshot, body).unwrap();

            let error = load_workflow(&workflow.workflow_id).unwrap_err();
            assert_eq!(error.code, 3);
            assert!(error.message.contains("fail-closed"));
        });
    }

    #[test]
    fn workflow_recovery_bounds_transaction_pointer_and_revision_snapshot_reads() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        with_workflow_env("workflow-recovery-read-bounds", |_| {
            let workflow = create_workflow("bounded workflow recovery").unwrap();
            let transaction = paths::project_workflow_transaction_file(&workflow.workflow_id);
            fs::write(
                &transaction,
                vec![b'x'; usize::try_from(MAX_WORKFLOW_SNAPSHOT_BYTES).unwrap() + 1],
            )
            .unwrap();
            let transaction_error =
                recover_workflow_transaction(&workflow.workflow_id).unwrap_err();
            assert!(transaction_error
                .message
                .contains("regular-file/byte budget"));

            let pointer = paths::project_workflow_file(&workflow.workflow_id);
            let pointer_body = fs::read(&pointer).unwrap();
            let snapshot =
                paths::project_workflow_snapshot_file(&workflow.workflow_id, workflow.revision);
            fs::write(&transaction, fs::read(&snapshot).unwrap()).unwrap();
            fs::write(
                &pointer,
                vec![b'x'; usize::try_from(MAX_WORKFLOW_POINTER_BYTES).unwrap() + 1],
            )
            .unwrap();
            let pointer_error = recover_workflow_transaction(&workflow.workflow_id).unwrap_err();
            assert!(pointer_error.message.contains("regular-file/byte budget"));

            fs::remove_file(&transaction).unwrap();
            fs::write(&pointer, pointer_body).unwrap();
            fs::write(
                &snapshot,
                vec![b'x'; usize::try_from(MAX_WORKFLOW_SNAPSHOT_BYTES).unwrap() + 1],
            )
            .unwrap();
            let snapshot_error = validate_workflow_chain(
                &workflow.workflow_id,
                workflow.revision,
                WORKFLOW_SCHEMA_VERSION,
            )
            .unwrap_err();
            assert!(snapshot_error.message.contains("regular-file/byte budget"));
        });
    }

    #[cfg(unix)]
    #[test]
    fn source_recovery_rejects_oversized_transaction_before_parsing() {
        let root = workflow_test_root("source-recovery-read-bound");
        fs::create_dir_all(&root).unwrap();
        let transaction = root.join("oversized-source-transaction.json");
        fs::write(
            &transaction,
            vec![b'x'; usize::try_from(MAX_PREPARED_SOURCE_BUNDLE_BYTES).unwrap() + 1],
        )
        .unwrap();

        let error = recover_source_replace(&transaction).unwrap_err();

        assert!(error.message.contains("regular-file/byte budget"));
        assert!(transaction.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn source_recovery_rejects_artifacts_outside_target_parent() {
        let root = std::env::temp_dir().join(format!(
            "rpotato-source-recovery-parent-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let source_dir = root.join("source");
        let outside_dir = root.join("outside");
        fs::create_dir_all(&source_dir).unwrap();
        fs::create_dir_all(&outside_dir).unwrap();
        let target = source_dir.join("lib.rs");
        let victim = outside_dir.join("victim.rs");
        let temporary = source_dir.join(".lib.rs.rpotato-new-1.2");
        let transaction = root.join("legacy-source-record");
        fs::write(&target, b"original").unwrap();
        fs::write(&victim, b"must-survive").unwrap();
        fs::write(&temporary, b"replacement").unwrap();
        fs::write(
            &transaction,
            format!(
                "schema_version=1\nintent_id=intent-source-boundary\ntarget={}\nguard={}\ntemporary={}\nexpected_current_hash={}\nexpected_replacement_hash={}\noperations={}\n",
                target.display(),
                victim.display(),
                temporary.display(),
                sha256_bytes(b"original"),
                sha256_bytes(b"replacement"),
                transition::SOURCE_INSTALL_OPERATIONS.join(",")
            ),
        )
        .unwrap();

        let error = recover_source_replace(&transaction).unwrap_err();
        assert!(error.message.contains("strict JSON") || error.message.contains("source_install"));
        assert_eq!(fs::read(&victim).unwrap(), b"must-survive");
        assert!(transaction.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn source_recovery_rejects_mismatched_artifact_nonce() {
        let root = std::env::temp_dir().join(format!(
            "rpotato-source-recovery-nonce-{}-{}",
            std::process::id(),
            now_ms()
        ));
        fs::create_dir_all(&root).unwrap();
        let target = root.join("lib.rs");
        let guard = root.join(".lib.rs.rpotato-guard-1.2");
        let temporary = root.join(".lib.rs.rpotato-new-1.3");
        let transaction = root.join("legacy-source-record");
        fs::write(&target, b"original").unwrap();
        fs::write(&guard, b"must-survive").unwrap();
        fs::write(&temporary, b"replacement").unwrap();
        fs::write(
            &transaction,
            format!(
                "schema_version=1\nintent_id=intent-source-mismatch\ntarget={}\nguard={}\ntemporary={}\nexpected_current_hash={}\nexpected_replacement_hash={}\noperations={}\n",
                target.display(),
                guard.display(),
                temporary.display(),
                sha256_bytes(b"original"),
                sha256_bytes(b"replacement"),
                transition::SOURCE_INSTALL_OPERATIONS.join(",")
            ),
        )
        .unwrap();

        let error = recover_source_replace(&transaction).unwrap_err();
        assert!(error.message.contains("strict JSON") || error.message.contains("source_install"));
        assert_eq!(fs::read(&guard).unwrap(), b"must-survive");
        assert!(transaction.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn source_identity_v1_matches_independent_golden_and_rejects_tamper() {
        let content_hash = "473b0fef5f0626d3fe806f10b931f085d511ba15b1117c53d5f2ec27d5b9452e";
        assert_eq!(sha256_bytes(b"current source\n"), content_hash);
        assert_eq!(
            transition::source_identity_v1(
                0x0102_0304_0506_0708,
                0x1112_1314_1516_1718,
                content_hash,
            )
            .unwrap(),
            "2b3452be6ffa18621fcd39e56162e5b46ef9428657dd6cdc9e02847e521420d0"
        );
        assert!(transition::source_identity_v1(
            0x0102_0304_0506_0708,
            0x1112_1314_1516_1718,
            &content_hash.to_ascii_uppercase()
        )
        .is_err());
        assert_ne!(
            transition::source_identity_v1(
                0x0102_0304_0506_0709,
                0x1112_1314_1516_1718,
                content_hash,
            )
            .unwrap(),
            "2b3452be6ffa18621fcd39e56162e5b46ef9428657dd6cdc9e02847e521420d0"
        );
    }

    #[test]
    fn ledger_ahead_of_committed_pointer_fails_closed() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        with_workflow_env("ledger-ahead", |_| {
            let workflow = create_workflow("stale latest checkpoint").unwrap();
            let identity = workflow_identity(&workflow);
            let forged_hash = "d".repeat(64);
            let event = ledger::new_event_for(
                &identity,
                "workflow.checkpoint",
                "forged uncommitted checkpoint",
                &format!(
                    "workflow_id={} revision=2 artifact_hash={forged_hash} previous_hash={} phase=approved action_id={} proposal_id=none evidence_id=none",
                    workflow.workflow_id, workflow.artifact_hash, workflow.action_id
                ),
            );
            ledger::append_event(&event).unwrap();

            let error = load_workflow(&workflow.workflow_id).unwrap_err();
            assert_eq!(error.code, 3);
            assert!(error.message.contains("ledger checkpoints: 2"));
        });
    }

    #[test]
    fn state_writer_callgraph_is_closed_and_serialized_by_project_transition() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-current-writer-transition-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let project = root.join("project");
        let data = root.join("data");
        fs::create_dir_all(&project).unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
        std::env::set_var("RPOTATO_DATA_HOME", &data);
        let initialized = initialize().unwrap();
        let before = current_state_lease_view().unwrap();
        let transition = transition::TransitionGuard::acquire_for(
            &initialized.identity.project_id,
            transition::CurrentStateIntent::RecordEvent,
        )
        .unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let writer = std::thread::spawn(move || {
            sender.send(session_new_report()).unwrap();
        });
        assert!(receiver
            .recv_timeout(std::time::Duration::from_millis(100))
            .is_err());
        drop(transition);
        receiver
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
            .unwrap();
        writer.join().unwrap();
        let after = current_state_lease_view().unwrap();
        assert_eq!(after.revision, before.revision + 1);

        let source = include_str!("state.rs")
            .split("\n#[cfg(test)]\nmod tests {")
            .next()
            .unwrap();
        let patch_source = include_str!("patch.rs")
            .split("\n#[cfg(test)]\nmod tests {")
            .next()
            .unwrap();
        assert!(!source.contains("pub fn write_current_state("));
        assert!(!source.contains("pub(crate) fn write_current_state("));
        assert!(!source.contains("pub fn write_current_state_for_session("));
        assert!(!source.contains("pub(crate) fn write_current_state_for_session("));
        assert!(!source.contains("pub(crate) fn install_current_image("));
        assert!(!source.contains("pub(crate) fn install_snapshot("));
        assert!(!source.contains("pub(crate) fn install_pointer("));
        assert!(!patch_source.contains(".install_snapshot("));
        assert!(!patch_source.contains(".install_pointer("));
        assert!(!patch_source.contains("state::install_current_image("));
        assert!(!patch_source.contains("paths::current_state_file()"));

        let source_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let authority_primitives = [
            "install_current_image(",
            "write_workflow_snapshot_bytes(",
            "write_workflow_pointer_for_schema(",
            ".install_snapshot(",
            ".install_pointer(",
        ];
        for entry in fs::read_dir(&source_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("rs") {
                continue;
            }
            let production = fs::read_to_string(&path)
                .unwrap()
                .split("\n#[cfg(test)]\nmod tests {")
                .next()
                .unwrap()
                .to_string();
            for primitive in authority_primitives {
                if production.contains(primitive) {
                    assert_eq!(
                        path.file_name().and_then(|value| value.to_str()),
                        Some("state.rs"),
                        "authority primitive {primitive} escaped state.rs into {}",
                        path.display()
                    );
                }
            }
        }
        let allowed_patch_transitions = [
            "state::transition_project_current_state_prepared_approval(",
            "state::transition_project_current_state_prepared_verification(",
            "state::transition_project_current_state_prepared_terminal_action(",
        ];
        for call in allowed_patch_transitions {
            assert!(
                patch_source.contains(call),
                "missing allowlisted call: {call}"
            );
        }
        assert_eq!(
            patch_source
                .matches("state::transition_project_current_state_prepared_")
                .count(),
            5,
            "patch.rs semantic writer allowlist changed"
        );

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);
    }
}
