//! Facade for workflow transaction and prepared-state recovery.

mod contracts;
mod projection;
mod transaction;
mod validation;

use crate::foundation::error::AppError;

pub(crate) use contracts::{
    PendingWorkflowTransaction, PreparedStateRecoveryPort, RecoveryArtifact, WorkflowRecoveryPort,
};

pub(crate) fn recover_prepared_state_transition(
    port: &mut impl PreparedStateRecoveryPort,
) -> Result<(), AppError> {
    projection::recover_prepared_state_transition(port)
}

pub(crate) fn recover_workflow_transaction(
    port: &impl WorkflowRecoveryPort,
    workflow_id: &str,
) -> Result<(), AppError> {
    transaction::recover_workflow_transaction(port, workflow_id)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::runtime_core::workflow::storage_compat::ledger::{
        RuntimeIdentity, WorkflowCheckpoint,
    };
    use crate::runtime_core::workflow::storage_compat::record::{WorkflowPointer, WorkflowRecord};

    struct FakePort {
        transaction: Option<PendingWorkflowTransaction>,
        pointer: Option<WorkflowPointer>,
        checkpoints: Vec<WorkflowCheckpoint>,
        committed: WorkflowRecord,
        checkpoint_exists: bool,
        calls: RefCell<Vec<&'static str>>,
    }

    #[derive(Default)]
    struct FakeStateRecoveryPort {
        calls: Vec<&'static str>,
    }

    impl PreparedStateRecoveryPort for FakeStateRecoveryPort {
        fn install_reconcile_backup(&mut self) -> Result<(), AppError> {
            self.calls.push("install-reconcile-backup");
            Ok(())
        }

        fn install_workflow_snapshot(&mut self) -> Result<(), AppError> {
            self.calls.push("install-workflow-snapshot");
            Ok(())
        }

        fn append_event(&mut self) -> Result<(), AppError> {
            self.calls.push("append-event");
            Ok(())
        }

        fn install_workflow_pointer(&mut self) -> Result<(), AppError> {
            self.calls.push("install-workflow-pointer");
            Ok(())
        }

        fn finish_events(&mut self) -> Result<(), AppError> {
            self.calls.push("finish-events");
            Ok(())
        }

        fn validate_ledger_binding(&mut self) -> Result<(), AppError> {
            self.calls.push("validate-ledger-binding");
            Ok(())
        }

        fn install_current_state(&mut self) -> Result<(), AppError> {
            self.calls.push("install-current-state");
            Ok(())
        }

        fn converge_projections(&mut self) -> Result<(), AppError> {
            self.calls.push("converge-projections");
            Ok(())
        }
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

    #[test]
    fn prepared_state_recovery_order_is_application_owned() {
        let mut port = FakeStateRecoveryPort::default();

        recover_prepared_state_transition(&mut port).unwrap();

        assert_eq!(
            port.calls,
            [
                "install-reconcile-backup",
                "install-workflow-snapshot",
                "append-event",
                "install-workflow-pointer",
                "finish-events",
                "validate-ledger-binding",
                "install-current-state",
                "converge-projections",
            ]
        );
    }
}
