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

mod workflow_revision;

pub(crate) use workflow_revision::{
    checkpoint_workflow, checkpoint_workflow_under_transition, create_workflow,
    decode_prepared_workflow_revision, PreparedCurrentImage, PreparedTerminalSource,
    PreparedWorkflowRevision, WorkflowCheckpointGuard,
};

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

mod transaction;

pub(crate) use transaction::{
    recover_project_current_state_prepared_approval,
    recover_project_current_state_prepared_terminal_action,
    recover_project_current_state_prepared_verification,
    transition_project_current_state_prepared_approval,
    transition_project_current_state_prepared_terminal_action,
    transition_project_current_state_prepared_verification, PreparedApprovalTransition,
    PreparedVerificationTransition, TerminalActionRequest,
};

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

mod lifecycle;

#[cfg(test)]
use lifecycle::session_new_report_for_intent;
#[cfg(test)]
pub(crate) use lifecycle::StateInit;
pub(crate) use lifecycle::{
    cancel_report, initialize, reconcile_report, record_event, resume_report, session_list_report,
    session_new_report, session_resume_preflight, session_resume_report,
    session_resume_report_for_tui, status_report, workflow_ownership_summary,
};

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

mod current_snapshot;

#[cfg(test)]
use current_snapshot::{classify_current_state, parse_current_state_v2};
pub(crate) use current_snapshot::{
    current_state_lease_view, current_state_lease_view_under_transition, read_regular_file_bounded,
    tui_lease_matches_terminal_selection_under_transition,
    tui_lease_matches_workflow_under_transition, tui_state_snapshot_read_only,
    validated_identity_from_current_state,
};
use current_snapshot::{
    current_state_status, parse_current_state, promote_current_state_v1,
    read_current_state_summary, read_open_file_bounded, render_current_state_v2,
    render_current_state_v2_payload, tui_detail_value, CurrentStateStatus,
};

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

mod workflow_store;

#[cfg(test)]
use workflow_store::{append_workflow_checkpoint_event, write_workflow_pointer_for_schema};
use workflow_store::{
    parse_workflow_pointer, parse_workflow_snapshot, prepared_workflow_member_id,
    recover_workflow_transaction, render_workflow_pointer_bytes, validate_workflow_chain,
    workflow_checkpoint_event, workflow_checkpoint_event_details, workflow_identity,
    workflow_snapshot_schema, write_workflow_snapshot_bytes,
};
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

mod source_install;

#[cfg(test)]
use source_install::recover_source_replace;
pub(crate) use source_install::{
    install_prepared_source_bundle, validate_prepared_source_parent,
    validate_source_install_initial_admission,
};

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
#[path = "state/tests/mod.rs"]
mod tests;
