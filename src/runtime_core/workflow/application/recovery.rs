//! Workflow transaction recovery ordering over explicit persistence ports.

use crate::foundation::error::AppError;
use crate::runtime_core::workflow::storage_compat::ledger::{RuntimeIdentity, WorkflowCheckpoint};
use crate::runtime_core::workflow::storage_compat::record::{WorkflowPointer, WorkflowRecord};

pub(crate) struct PendingWorkflowTransaction {
    pub schema_version: u64,
    pub record: WorkflowRecord,
    pub body: String,
}

#[derive(Clone, Copy)]
pub(crate) enum RecoveryArtifact {
    Transaction,
    Pointer,
}

pub(crate) trait WorkflowRecoveryPort {
    fn load_transaction(
        &self,
        workflow_id: &str,
    ) -> Result<Option<PendingWorkflowTransaction>, AppError>;

    fn load_pointer(&self, workflow_id: &str) -> Result<Option<WorkflowPointer>, AppError>;

    fn checkpoints(&self, workflow_id: &str) -> Result<Vec<WorkflowCheckpoint>, AppError>;

    fn validate_chain(
        &self,
        workflow_id: &str,
        committed_revision: u64,
        expected_latest_schema: u64,
    ) -> Result<WorkflowRecord, AppError>;

    fn validate_chain_with_checkpoints(
        &self,
        workflow_id: &str,
        committed_revision: u64,
        expected_latest_schema: u64,
        checkpoints: &[WorkflowCheckpoint],
    ) -> Result<WorkflowRecord, AppError>;

    fn current_identity(&self) -> Result<RuntimeIdentity, AppError>;

    fn checkpoint_exists(
        &self,
        workflow_id: &str,
        revision: u64,
        artifact_hash: &str,
    ) -> Result<bool, AppError>;

    fn install_snapshot(&self, record: &WorkflowRecord, body: &[u8]) -> Result<(), AppError>;

    fn install_pointer(&self, record: &WorkflowRecord, schema_version: u64)
        -> Result<(), AppError>;

    fn remove_transaction(&self, workflow_id: &str) -> Result<(), AppError>;

    fn corrupt(&self, workflow_id: &str, artifact: RecoveryArtifact) -> AppError;
}

pub(crate) fn recover_workflow_transaction(
    port: &impl WorkflowRecoveryPort,
    workflow_id: &str,
) -> Result<(), AppError> {
    let Some(transaction) = port.load_transaction(workflow_id)? else {
        return Ok(());
    };
    let record = &transaction.record;
    if record.workflow_id != workflow_id {
        return Err(port.corrupt(workflow_id, RecoveryArtifact::Transaction));
    }

    if let Some(pointer) = port.load_pointer(workflow_id)? {
        if pointer.workflow_id != workflow_id {
            return Err(port.corrupt(workflow_id, RecoveryArtifact::Pointer));
        }
        if pointer.committed_revision == record.revision
            && pointer.artifact_hash == record.artifact_hash
        {
            if pointer.schema_version != transaction.schema_version {
                return Err(port.corrupt(workflow_id, RecoveryArtifact::Transaction));
            }
            port.validate_chain(
                workflow_id,
                pointer.committed_revision,
                pointer.schema_version,
            )?;
            return port.remove_transaction(workflow_id);
        }

        let schema_transition_allowed = pointer.schema_version <= transaction.schema_version;
        if pointer.committed_revision.checked_add(1) != Some(record.revision)
            || record.previous_hash != pointer.artifact_hash
            || !schema_transition_allowed
        {
            return Err(port.corrupt(workflow_id, RecoveryArtifact::Transaction));
        }
        let checkpoints = port.checkpoints(workflow_id)?;
        if checkpoints.len() != pointer.committed_revision as usize
            && checkpoints.len() != record.revision as usize
        {
            return Err(port.corrupt(workflow_id, RecoveryArtifact::Transaction));
        }
        let committed = port.validate_chain_with_checkpoints(
            workflow_id,
            pointer.committed_revision,
            pointer.schema_version,
            &checkpoints[..pointer.committed_revision as usize],
        )?;
        if committed.artifact_hash != pointer.artifact_hash
            || committed.project_id != record.project_id
            || committed.session_id != record.session_id
            || committed.action_id != record.action_id
        {
            return Err(port.corrupt(workflow_id, RecoveryArtifact::Transaction));
        }
        if checkpoints.len() == record.revision as usize {
            let pending = &checkpoints[record.revision as usize - 1];
            if pending.revision != record.revision
                || pending.artifact_hash != record.artifact_hash
                || pending.previous_hash != record.previous_hash
            {
                return Err(port.corrupt(workflow_id, RecoveryArtifact::Transaction));
            }
        }
    } else {
        let checkpoints = port.checkpoints(workflow_id)?;
        if record.revision != 1
            || record.previous_hash != "none"
            || checkpoints.len() > 1
            || checkpoints.first().is_some_and(|checkpoint| {
                checkpoint.revision != record.revision
                    || checkpoint.artifact_hash != record.artifact_hash
                    || checkpoint.previous_hash != record.previous_hash
            })
        {
            return Err(port.corrupt(workflow_id, RecoveryArtifact::Transaction));
        }
        if record.project_id != port.current_identity()?.project_id {
            return Err(port.corrupt(workflow_id, RecoveryArtifact::Transaction));
        }
    }

    if !port.checkpoint_exists(workflow_id, record.revision, &record.artifact_hash)? {
        return Err(AppError::blocked(
            "legacy workflow transaction recovery 차단\n- 이유: exact prepared semantic event가 없습니다.\n- 동작: transaction 증거를 보존했습니다.",
        ));
    }
    port.install_snapshot(record, transaction.body.as_bytes())?;
    port.install_pointer(record, transaction.schema_version)?;
    port.remove_transaction(workflow_id)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    struct FakePort {
        transaction: Option<PendingWorkflowTransaction>,
        pointer: Option<WorkflowPointer>,
        checkpoints: Vec<WorkflowCheckpoint>,
        committed: WorkflowRecord,
        checkpoint_exists: bool,
        calls: RefCell<Vec<&'static str>>,
    }

    impl WorkflowRecoveryPort for FakePort {
        fn load_transaction(
            &self,
            _workflow_id: &str,
        ) -> Result<Option<PendingWorkflowTransaction>, AppError> {
            self.calls.borrow_mut().push("load-transaction");
            Ok(self
                .transaction
                .as_ref()
                .map(|transaction| PendingWorkflowTransaction {
                    schema_version: transaction.schema_version,
                    record: transaction.record.clone(),
                    body: transaction.body.clone(),
                }))
        }

        fn load_pointer(&self, _workflow_id: &str) -> Result<Option<WorkflowPointer>, AppError> {
            self.calls.borrow_mut().push("load-pointer");
            Ok(self.pointer.as_ref().map(|pointer| WorkflowPointer {
                schema_version: pointer.schema_version,
                workflow_id: pointer.workflow_id.clone(),
                committed_revision: pointer.committed_revision,
                artifact_hash: pointer.artifact_hash.clone(),
            }))
        }

        fn checkpoints(&self, _workflow_id: &str) -> Result<Vec<WorkflowCheckpoint>, AppError> {
            self.calls.borrow_mut().push("checkpoints");
            Ok(self.checkpoints.clone())
        }

        fn validate_chain(
            &self,
            _workflow_id: &str,
            _committed_revision: u64,
            _expected_latest_schema: u64,
        ) -> Result<WorkflowRecord, AppError> {
            self.calls.borrow_mut().push("validate-chain");
            Ok(self.committed.clone())
        }

        fn validate_chain_with_checkpoints(
            &self,
            _workflow_id: &str,
            _committed_revision: u64,
            _expected_latest_schema: u64,
            _checkpoints: &[WorkflowCheckpoint],
        ) -> Result<WorkflowRecord, AppError> {
            self.calls.borrow_mut().push("validate-chain-prefix");
            Ok(self.committed.clone())
        }

        fn current_identity(&self) -> Result<RuntimeIdentity, AppError> {
            self.calls.borrow_mut().push("current-identity");
            Ok(identity())
        }

        fn checkpoint_exists(
            &self,
            _workflow_id: &str,
            _revision: u64,
            _artifact_hash: &str,
        ) -> Result<bool, AppError> {
            self.calls.borrow_mut().push("checkpoint-exists");
            Ok(self.checkpoint_exists)
        }

        fn install_snapshot(&self, _record: &WorkflowRecord, _body: &[u8]) -> Result<(), AppError> {
            self.calls.borrow_mut().push("install-snapshot");
            Ok(())
        }

        fn install_pointer(
            &self,
            _record: &WorkflowRecord,
            _schema_version: u64,
        ) -> Result<(), AppError> {
            self.calls.borrow_mut().push("install-pointer");
            Ok(())
        }

        fn remove_transaction(&self, _workflow_id: &str) -> Result<(), AppError> {
            self.calls.borrow_mut().push("remove-transaction");
            Ok(())
        }

        fn corrupt(&self, _workflow_id: &str, _artifact: RecoveryArtifact) -> AppError {
            AppError::blocked("corrupt workflow fixture")
        }
    }

    fn identity() -> RuntimeIdentity {
        RuntimeIdentity {
            project_id: "project".to_owned(),
            session_id: "session".to_owned(),
            project_root: "/project".to_owned(),
        }
    }

    fn record(revision: u64, previous_hash: &str, artifact_hash: &str) -> WorkflowRecord {
        let mut record = WorkflowRecord::new(&identity(), "recovery fixture");
        record.workflow_id = "workflow-fixture".to_owned();
        record.revision = revision;
        record.previous_hash = previous_hash.to_owned();
        record.artifact_hash = artifact_hash.to_owned();
        record.action_id = "action-fixture".to_owned();
        record
    }

    fn transaction(record: WorkflowRecord) -> PendingWorkflowTransaction {
        PendingWorkflowTransaction {
            schema_version: 4,
            record,
            body: "canonical-record".to_owned(),
        }
    }

    #[test]
    fn replays_only_an_exact_prepared_suffix_in_install_order() {
        let committed = record(1, "none", "hash-1");
        let pending = record(2, "hash-1", "hash-2");
        let port = FakePort {
            transaction: Some(transaction(pending.clone())),
            pointer: Some(WorkflowPointer {
                schema_version: 4,
                workflow_id: pending.workflow_id.clone(),
                committed_revision: 1,
                artifact_hash: "hash-1".to_owned(),
            }),
            checkpoints: vec![
                WorkflowCheckpoint {
                    revision: 1,
                    artifact_hash: "hash-1".to_owned(),
                    previous_hash: "none".to_owned(),
                },
                WorkflowCheckpoint {
                    revision: 2,
                    artifact_hash: "hash-2".to_owned(),
                    previous_hash: "hash-1".to_owned(),
                },
            ],
            committed,
            checkpoint_exists: true,
            calls: RefCell::new(Vec::new()),
        };

        recover_workflow_transaction(&port, "workflow-fixture").unwrap();

        assert_eq!(
            *port.calls.borrow(),
            [
                "load-transaction",
                "load-pointer",
                "checkpoints",
                "validate-chain-prefix",
                "checkpoint-exists",
                "install-snapshot",
                "install-pointer",
                "remove-transaction",
            ]
        );
    }

    #[test]
    fn preserves_uncertain_transaction_without_install_or_cleanup() {
        let pending = record(1, "none", "hash-1");
        let port = FakePort {
            transaction: Some(transaction(pending.clone())),
            pointer: None,
            checkpoints: Vec::new(),
            committed: pending,
            checkpoint_exists: false,
            calls: RefCell::new(Vec::new()),
        };

        let error = recover_workflow_transaction(&port, "workflow-fixture").unwrap_err();

        assert!(error.message.contains("exact prepared semantic event"));
        assert_eq!(
            *port.calls.borrow(),
            [
                "load-transaction",
                "load-pointer",
                "checkpoints",
                "current-identity",
                "checkpoint-exists",
            ]
        );
    }
}
