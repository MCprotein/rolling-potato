use super::*;

use crate::adapters::filesystem::atomic_write::{atomic_replace_bytes, sync_parent};

pub(super) fn write_workflow_snapshot_bytes(
    record: &WorkflowRecord,
    rendered: &[u8],
) -> Result<(), AppError> {
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

pub(super) fn write_workflow_pointer_for_schema(
    record: &WorkflowRecord,
    schema_version: u64,
) -> Result<(), AppError> {
    let body = render_workflow_pointer_bytes(record, schema_version)?;
    atomic_replace_bytes(
        &paths::project_workflow_file(&record.workflow_id),
        body.as_bytes(),
    )
}

pub(super) fn render_workflow_pointer_bytes(
    record: &WorkflowRecord,
    schema_version: u64,
) -> Result<String, AppError> {
    crate::runtime_core::workflow::storage_compat::record::render_pointer(record, schema_version)
}

pub(super) fn parse_workflow_pointer(
    path: &std::path::Path,
    body: &str,
) -> Result<WorkflowPointer, AppError> {
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

pub(super) fn recover_workflow_transaction(workflow_id: &str) -> Result<(), AppError> {
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
pub(super) fn append_workflow_checkpoint_event(record: &WorkflowRecord) -> Result<(), AppError> {
    let event = workflow_checkpoint_event(record, &workflow_identity(record));
    ledger::append_event(&event)?;
    observability::project_event(&event)
}

pub(super) fn validate_workflow_chain(
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

pub(super) fn workflow_snapshot_schema(
    path: &std::path::Path,
    body: &str,
) -> Result<u64, AppError> {
    crate::runtime_core::workflow::storage_compat::record::snapshot_schema(
        path,
        body,
        corrupt_workflow,
    )
}

pub(super) fn parse_workflow_snapshot(
    path: &std::path::Path,
    body: &str,
) -> Result<WorkflowRecord, AppError> {
    crate::runtime_core::workflow::storage_compat::record::parse_snapshot(
        path,
        body,
        corrupt_workflow,
    )
}

pub(super) fn workflow_identity(record: &WorkflowRecord) -> RuntimeIdentity {
    RuntimeIdentity {
        project_id: record.project_id.clone(),
        session_id: record.session_id.clone(),
        project_root: paths::project_root().display().to_string(),
    }
}

pub(super) fn workflow_checkpoint_event(
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

pub(super) fn workflow_checkpoint_event_details(record: &WorkflowRecord) -> String {
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

pub(super) fn prepared_workflow_member_id(
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
