use super::*;

fn event(id: &str) -> LedgerEvent {
    LedgerEvent {
        event_id: id.to_owned(),
        ts_ms: 1,
        event_type: "workflow.test".to_owned(),
        project_id: "project".to_owned(),
        session_id: "session".to_owned(),
        summary: "summary".to_owned(),
        details: "details".to_owned(),
    }
}

fn planned(event: LedgerEvent, ordinal: u64) -> PlannedEvent {
    PlannedEvent {
        event,
        ordinal,
        previous_event_hash: "previous".to_owned(),
        event_hash: "current".to_owned(),
    }
}

#[derive(Default)]
struct FakeApprovalPort {
    calls: Vec<String>,
}

#[derive(Default)]
struct FakeVerificationPort {
    calls: Vec<String>,
}

#[derive(Default)]
struct FakeTerminalActionPort {
    calls: Vec<String>,
}

#[derive(Default)]
struct FakeStateTransitionPort {
    calls: Vec<String>,
}

#[derive(Default)]
struct FakeReconcilePort {
    calls: Vec<String>,
}

impl ReconcileTransactionPort for FakeReconcilePort {
    fn fault(&mut self, point: StateTransitionFault) -> Result<(), AppError> {
        self.calls.push(format!("fault:{point:?}"));
        Ok(())
    }

    fn install_backup(&mut self) -> Result<(), AppError> {
        self.calls.push("backup".to_owned());
        Ok(())
    }

    fn append_event(&mut self) -> Result<(), AppError> {
        self.calls.push("append".to_owned());
        Ok(())
    }

    fn finish_events(&mut self) -> Result<(), AppError> {
        self.calls.push("finish".to_owned());
        Ok(())
    }

    fn install_current(&mut self) -> Result<(), AppError> {
        self.calls.push("current".to_owned());
        Ok(())
    }

    fn converge(&mut self) -> Result<(), AppError> {
        self.calls.push("converge".to_owned());
        Ok(())
    }

    fn remove_journal(&mut self) -> Result<(), AppError> {
        self.calls.push("remove-journal".to_owned());
        Ok(())
    }
}

impl StateTransitionTransactionPort for FakeStateTransitionPort {
    fn fault(&mut self, point: StateTransitionFault) -> Result<(), AppError> {
        self.calls.push(format!("fault:{point:?}"));
        Ok(())
    }

    fn install_snapshot(&mut self) -> Result<(), AppError> {
        self.calls.push("snapshot".to_owned());
        Ok(())
    }

    fn append_event(&mut self) -> Result<(), AppError> {
        self.calls.push("append".to_owned());
        Ok(())
    }

    fn install_pointer(&mut self) -> Result<(), AppError> {
        self.calls.push("pointer".to_owned());
        Ok(())
    }

    fn finish_events(&mut self) -> Result<(), AppError> {
        self.calls.push("finish".to_owned());
        Ok(())
    }

    fn install_current(&mut self) -> Result<(), AppError> {
        self.calls.push("current".to_owned());
        Ok(())
    }

    fn converge(&mut self) -> Result<(), AppError> {
        self.calls.push("converge".to_owned());
        Ok(())
    }

    fn remove_journal(&mut self) -> Result<(), AppError> {
        self.calls.push("remove-journal".to_owned());
        Ok(())
    }
}

impl TerminalActionTransactionPort for FakeTerminalActionPort {
    fn fault(&mut self, point: TerminalActionFault) -> Result<(), AppError> {
        self.calls.push(format!("fault:{}", point.as_str()));
        Ok(())
    }

    fn append_event(&mut self, index: usize) -> Result<(), AppError> {
        self.calls.push(format!("append:{index}"));
        Ok(())
    }

    fn install_source(&mut self) -> Result<(), AppError> {
        self.calls.push("source".to_owned());
        Ok(())
    }

    fn install_snapshot(&mut self) -> Result<(), AppError> {
        self.calls.push("snapshot".to_owned());
        Ok(())
    }

    fn install_pointer(&mut self) -> Result<(), AppError> {
        self.calls.push("pointer".to_owned());
        Ok(())
    }

    fn finish_events(&mut self) -> Result<(), AppError> {
        self.calls.push("finish".to_owned());
        Ok(())
    }

    fn install_current(&mut self) -> Result<(), AppError> {
        self.calls.push("current".to_owned());
        Ok(())
    }

    fn converge(&mut self) -> Result<(), AppError> {
        self.calls.push("converge".to_owned());
        Ok(())
    }

    fn remove_journal(&mut self) -> Result<(), AppError> {
        self.calls.push("remove-journal".to_owned());
        Ok(())
    }
}

impl VerificationTransactionPort for FakeVerificationPort {
    fn fault(&mut self, point: VerificationFault) -> Result<(), AppError> {
        self.calls.push(format!("fault:{}", point.as_str()));
        Ok(())
    }

    fn append_event(&mut self, index: usize) -> Result<(), AppError> {
        self.calls.push(format!("append:{index}"));
        Ok(())
    }

    fn install_snapshot(&mut self) -> Result<(), AppError> {
        self.calls.push("snapshot".to_owned());
        Ok(())
    }

    fn install_pointer(&mut self) -> Result<(), AppError> {
        self.calls.push("pointer".to_owned());
        Ok(())
    }

    fn install_current(&mut self) -> Result<(), AppError> {
        self.calls.push("current".to_owned());
        Ok(())
    }

    fn finish_events(&mut self) -> Result<(), AppError> {
        self.calls.push("finish".to_owned());
        Ok(())
    }

    fn converge(&mut self) -> Result<(), AppError> {
        self.calls.push("converge".to_owned());
        Ok(())
    }

    fn remove_journal(&mut self) -> Result<(), AppError> {
        self.calls.push("remove-journal".to_owned());
        Ok(())
    }
}

impl ApprovalTransactionPort for FakeApprovalPort {
    fn fault(&mut self, point: ApprovalFault) -> Result<(), AppError> {
        self.calls.push(format!("fault:{}", point.as_str()));
        Ok(())
    }

    fn append_event(&mut self, index: usize) -> Result<(), AppError> {
        self.calls.push(format!("append:{index}"));
        Ok(())
    }

    fn install_snapshot(&mut self, revision: ApprovalRevision) -> Result<(), AppError> {
        self.calls.push(format!("snapshot:{revision:?}"));
        Ok(())
    }

    fn install_pointer(&mut self, revision: ApprovalRevision) -> Result<(), AppError> {
        self.calls.push(format!("pointer:{revision:?}"));
        Ok(())
    }

    fn install_source(&mut self) -> Result<(), AppError> {
        self.calls.push("source".to_owned());
        Ok(())
    }

    fn install_transcript(&mut self) -> Result<(), AppError> {
        self.calls.push("transcript".to_owned());
        Ok(())
    }

    fn install_current(&mut self) -> Result<(), AppError> {
        self.calls.push("current".to_owned());
        Ok(())
    }

    fn finish_events(&mut self) -> Result<(), AppError> {
        self.calls.push("finish".to_owned());
        Ok(())
    }

    fn converge(&mut self) -> Result<(), AppError> {
        self.calls.push("converge".to_owned());
        Ok(())
    }

    fn projection_repair_required(&mut self, _convergence_error: AppError) -> AppError {
        self.calls.push("projection-repair".to_owned());
        AppError::blocked("projection repair")
    }

    fn remove_projection_lag(&mut self) -> Result<(), AppError> {
        self.calls.push("remove-lag".to_owned());
        Ok(())
    }

    fn validate_cleanup_authority(&mut self) -> Result<(), AppError> {
        self.calls.push("validate-cleanup".to_owned());
        Ok(())
    }

    fn remove_journal(&mut self) -> Result<(), AppError> {
        self.calls.push("remove-journal".to_owned());
        Ok(())
    }
}

#[test]
fn accepts_only_the_next_bound_event() {
    let first = event("first");
    let second = event("second");
    let plan = [planned(first.clone(), 1), planned(second.clone(), 2)];
    let mut coordinator = TransactionCoordinator::new(&plan);

    assert!(coordinator.validate_next(1, &second).is_err());
    assert!(coordinator.validate_next(0, &second).is_err());
    assert_eq!(coordinator.validate_next(0, &first).unwrap(), &plan[0]);
    coordinator.record_appended(0).unwrap();
    assert!(coordinator.validate_next(0, &first).is_err());
    assert_eq!(coordinator.validate_next(1, &second).unwrap(), &plan[1]);
}

#[test]
fn finishes_only_after_every_planned_event_is_recorded() {
    let first = event("first");
    let second = event("second");
    let plan = [planned(first.clone(), 1), planned(second.clone(), 2)];
    let mut coordinator = TransactionCoordinator::new(&plan);

    assert!(coordinator.finish().is_err());
    coordinator.validate_next(0, &first).unwrap();
    coordinator.record_appended(0).unwrap();
    assert!(coordinator.finish().is_err());
    coordinator.validate_next(1, &second).unwrap();
    coordinator.record_appended(1).unwrap();
    coordinator.finish().unwrap();
}

#[test]
fn approval_commit_order_is_application_owned() {
    let mut port = FakeApprovalPort::default();

    execute_approval_transaction(&mut port, TransactionExecution::Commit).unwrap();

    assert_eq!(
        port.calls,
        [
            "fault:T1",
            "append:0",
            "fault:T2",
            "snapshot:First",
            "append:1",
            "fault:T3-before-pointer",
            "pointer:First",
            "fault:T3",
            "append:2",
            "append:3",
            "append:4",
            "fault:T4",
            "source",
            "fault:T5",
            "append:5",
            "append:6",
            "append:7",
            "fault:T6",
            "transcript",
            "append:8",
            "fault:T7",
            "snapshot:Second",
            "append:9",
            "fault:T8-before-pointer",
            "pointer:Second",
            "fault:T8",
            "current",
            "fault:T9",
            "finish",
            "converge",
            "fault:T10",
            "remove-lag",
            "validate-cleanup",
            "remove-journal",
        ]
    );
}

#[test]
fn approval_recovery_reuses_order_without_faults_or_journal_cleanup() {
    let mut port = FakeApprovalPort::default();

    execute_approval_transaction(&mut port, TransactionExecution::Recovery).unwrap();

    assert!(port.calls.iter().all(|call| !call.starts_with("fault:")));
    assert!(!port.calls.iter().any(|call| call == "remove-journal"));
    assert_eq!(port.calls.first().map(String::as_str), Some("append:0"));
    assert_eq!(
        port.calls.last().map(String::as_str),
        Some("validate-cleanup")
    );
}

#[test]
fn verification_commit_and_recovery_share_one_order() {
    let mut commit = FakeVerificationPort::default();
    execute_verification_transaction(&mut commit, TransactionExecution::Commit).unwrap();
    assert_eq!(
        commit.calls,
        [
            "fault:V1",
            "append:0",
            "fault:V2",
            "snapshot",
            "append:1",
            "fault:V3-before-pointer",
            "pointer",
            "fault:V3",
            "append:2",
            "fault:V4",
            "current",
            "fault:V5",
            "finish",
            "converge",
            "fault:V6",
            "remove-journal",
        ]
    );

    let mut recovery = FakeVerificationPort::default();
    execute_verification_transaction(&mut recovery, TransactionExecution::Recovery).unwrap();
    assert_eq!(
        recovery.calls,
        [
            "append:0", "snapshot", "append:1", "pointer", "append:2", "current", "finish",
            "converge",
        ]
    );
}

#[test]
fn terminal_action_commit_and_recovery_share_one_order() {
    let mut commit = FakeTerminalActionPort::default();
    execute_terminal_action_transaction(&mut commit, TransactionExecution::Commit).unwrap();
    assert_eq!(
        commit.calls,
        [
            "fault:A1-after-journal",
            "append:0",
            "fault:A2-after-intent",
            "source",
            "fault:A3-after-source",
            "snapshot",
            "append:1",
            "fault:A4-after-snapshot",
            "pointer",
            "fault:A5-after-pointer",
            "append:2",
            "finish",
            "fault:A6-after-ledger",
            "current",
            "fault:A7-after-current",
            "converge",
            "fault:A8-after-projection",
            "remove-journal",
        ]
    );

    let mut recovery = FakeTerminalActionPort::default();
    execute_terminal_action_transaction(&mut recovery, TransactionExecution::Recovery).unwrap();
    assert_eq!(
        recovery.calls,
        [
            "append:0", "source", "snapshot", "append:1", "pointer", "append:2", "finish",
            "current", "converge",
        ]
    );
}

#[test]
fn checkpoint_transition_order_is_application_owned() {
    let mut port = FakeStateTransitionPort::default();

    execute_state_transition(&mut port, true).unwrap();

    assert_eq!(
        port.calls,
        [
            "fault:Journal",
            "fault:CheckpointTransaction",
            "snapshot",
            "fault:CheckpointSnapshot",
            "fault:Artifacts",
            "append",
            "fault:Ledger",
            "fault:CheckpointLedger",
            "pointer",
            "fault:CheckpointPointer",
            "finish",
            "current",
            "fault:Current",
            "converge",
            "fault:Projection",
            "remove-journal",
        ]
    );
}

#[test]
fn reconcile_preserves_backup_before_canonical_append() {
    let mut port = FakeReconcilePort::default();

    execute_reconcile_transaction(&mut port).unwrap();

    assert_eq!(
        port.calls,
        [
            "fault:Journal",
            "backup",
            "fault:Artifacts",
            "append",
            "fault:Ledger",
            "finish",
            "current",
            "fault:Current",
            "converge",
            "fault:Projection",
            "remove-journal",
        ]
    );
}
