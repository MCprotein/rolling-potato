use std::cell::Cell;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

mod current_transition;

use current_transition::{
    install_prepared_reconcile_backup, prepare_state_transition_current_image,
    prepare_terminal_current_image_after, prepared_workflow_member,
    state_transition_current_member, transition_project_current_state_under_guard,
    validate_state_transition_current_cas, StateTransitionRequest,
};
pub(crate) use current_transition::{
    prepare_current_image, prepare_current_image_after, recover_prepared_state_transition,
    validate_prepared_state_current_member,
};

mod transition_commit;

use transition_commit::{
    commit_state_event, install_current_image, internal_transition_intent_id,
    read_valid_current_for_transition, reconcile_invalid_current_under_guard,
};
pub(crate) use transition_commit::{
    decode_prepared_current_image, decode_prepared_terminal_current_image,
    validate_current_state_recovery_cas,
};

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

mod workflow_access;

pub use workflow_access::{active_workflow_id, load_workflow};
pub(crate) use workflow_access::{
    clear_terminal_workflow_pointer, clear_terminal_workflow_pointer_under_transition,
    load_workflow_revision, record_tui_workflow_resume_receipt_under_transition,
    record_workflow_event_under_transition,
};
use workflow_access::{discover_active_workflow, load_workflow_under_transition};

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
mod source_install;

#[cfg(all(test, unix))]
use source_install::recover_source_replace;
#[cfg(unix)]
pub(crate) use source_install::validate_source_install_initial_admission;
pub(crate) use source_install::{install_prepared_source_bundle, validate_prepared_source_parent};

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
