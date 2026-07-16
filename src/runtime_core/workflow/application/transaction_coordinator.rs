//! Ordered transaction progress independent of concrete persistence adapters.

use crate::foundation::error::AppError;
use crate::runtime_core::workflow::storage_compat::ledger::LedgerEvent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlannedEvent {
    pub event: LedgerEvent,
    pub ordinal: u64,
    pub previous_event_hash: String,
    pub event_hash: String,
}

pub(crate) struct TransactionCoordinator<'plan> {
    planned: &'plan [PlannedEvent],
    next_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransactionExecution {
    Commit,
    Recovery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ApprovalFault {
    T1,
    T2,
    T3BeforePointer,
    T3,
    T4,
    T5,
    T6,
    T7,
    T8BeforePointer,
    T8,
    T9,
    T10,
}

impl ApprovalFault {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::T1 => "T1",
            Self::T2 => "T2",
            Self::T3BeforePointer => "T3-before-pointer",
            Self::T3 => "T3",
            Self::T4 => "T4",
            Self::T5 => "T5",
            Self::T6 => "T6",
            Self::T7 => "T7",
            Self::T8BeforePointer => "T8-before-pointer",
            Self::T8 => "T8",
            Self::T9 => "T9",
            Self::T10 => "T10",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ApprovalRevision {
    First,
    Second,
}

pub(crate) trait ApprovalTransactionPort {
    fn fault(&mut self, point: ApprovalFault) -> Result<(), AppError>;
    fn append_event(&mut self, index: usize) -> Result<(), AppError>;
    fn install_snapshot(&mut self, revision: ApprovalRevision) -> Result<(), AppError>;
    fn install_pointer(&mut self, revision: ApprovalRevision) -> Result<(), AppError>;
    fn install_source(&mut self) -> Result<(), AppError>;
    fn install_transcript(&mut self) -> Result<(), AppError>;
    fn install_current(&mut self) -> Result<(), AppError>;
    fn finish_events(&mut self) -> Result<(), AppError>;
    fn converge(&mut self) -> Result<(), AppError>;
    fn projection_repair_required(&mut self, convergence_error: AppError) -> AppError;
    fn remove_projection_lag(&mut self) -> Result<(), AppError>;
    fn validate_cleanup_authority(&mut self) -> Result<(), AppError>;
    fn remove_journal(&mut self) -> Result<(), AppError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VerificationFault {
    V1,
    V2,
    V3BeforePointer,
    V3,
    V4,
    V5,
    V6,
}

impl VerificationFault {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::V1 => "V1",
            Self::V2 => "V2",
            Self::V3BeforePointer => "V3-before-pointer",
            Self::V3 => "V3",
            Self::V4 => "V4",
            Self::V5 => "V5",
            Self::V6 => "V6",
        }
    }
}

pub(crate) trait VerificationTransactionPort {
    fn fault(&mut self, point: VerificationFault) -> Result<(), AppError>;
    fn append_event(&mut self, index: usize) -> Result<(), AppError>;
    fn install_snapshot(&mut self) -> Result<(), AppError>;
    fn install_pointer(&mut self) -> Result<(), AppError>;
    fn install_current(&mut self) -> Result<(), AppError>;
    fn finish_events(&mut self) -> Result<(), AppError>;
    fn converge(&mut self) -> Result<(), AppError>;
    fn remove_journal(&mut self) -> Result<(), AppError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalActionFault {
    Journal,
    Intent,
    Source,
    Snapshot,
    Pointer,
    Ledger,
    Current,
    Projection,
}

impl TerminalActionFault {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Journal => "A1-after-journal",
            Self::Intent => "A2-after-intent",
            Self::Source => "A3-after-source",
            Self::Snapshot => "A4-after-snapshot",
            Self::Pointer => "A5-after-pointer",
            Self::Ledger => "A6-after-ledger",
            Self::Current => "A7-after-current",
            Self::Projection => "A8-after-projection",
        }
    }
}

pub(crate) trait TerminalActionTransactionPort {
    fn fault(&mut self, point: TerminalActionFault) -> Result<(), AppError>;
    fn append_event(&mut self, index: usize) -> Result<(), AppError>;
    fn install_source(&mut self) -> Result<(), AppError>;
    fn install_snapshot(&mut self) -> Result<(), AppError>;
    fn install_pointer(&mut self) -> Result<(), AppError>;
    fn finish_events(&mut self) -> Result<(), AppError>;
    fn install_current(&mut self) -> Result<(), AppError>;
    fn converge(&mut self) -> Result<(), AppError>;
    fn remove_journal(&mut self) -> Result<(), AppError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StateTransitionFault {
    Journal,
    CheckpointTransaction,
    CheckpointSnapshot,
    Artifacts,
    Ledger,
    CheckpointLedger,
    CheckpointPointer,
    Current,
    Projection,
}

pub(crate) trait StateTransitionTransactionPort {
    fn fault(&mut self, point: StateTransitionFault) -> Result<(), AppError>;
    fn install_snapshot(&mut self) -> Result<(), AppError>;
    fn append_event(&mut self) -> Result<(), AppError>;
    fn install_pointer(&mut self) -> Result<(), AppError>;
    fn finish_events(&mut self) -> Result<(), AppError>;
    fn install_current(&mut self) -> Result<(), AppError>;
    fn converge(&mut self) -> Result<(), AppError>;
    fn remove_journal(&mut self) -> Result<(), AppError>;
}

pub(crate) trait ReconcileTransactionPort {
    fn fault(&mut self, point: StateTransitionFault) -> Result<(), AppError>;
    fn install_backup(&mut self) -> Result<(), AppError>;
    fn append_event(&mut self) -> Result<(), AppError>;
    fn finish_events(&mut self) -> Result<(), AppError>;
    fn install_current(&mut self) -> Result<(), AppError>;
    fn converge(&mut self) -> Result<(), AppError>;
    fn remove_journal(&mut self) -> Result<(), AppError>;
}

pub(crate) fn execute_approval_transaction(
    port: &mut impl ApprovalTransactionPort,
    execution: TransactionExecution,
) -> Result<(), AppError> {
    let commit = execution == TransactionExecution::Commit;
    if commit {
        port.fault(ApprovalFault::T1)?;
    }
    port.append_event(0)?;
    if commit {
        port.fault(ApprovalFault::T2)?;
    }
    port.install_snapshot(ApprovalRevision::First)?;
    port.append_event(1)?;
    if commit {
        port.fault(ApprovalFault::T3BeforePointer)?;
    }
    port.install_pointer(ApprovalRevision::First)?;
    if commit {
        port.fault(ApprovalFault::T3)?;
    }
    for index in 2..5 {
        port.append_event(index)?;
    }
    if commit {
        port.fault(ApprovalFault::T4)?;
    }
    port.install_source()?;
    if commit {
        port.fault(ApprovalFault::T5)?;
    }
    for index in 5..8 {
        port.append_event(index)?;
    }
    if commit {
        port.fault(ApprovalFault::T6)?;
    }
    port.install_transcript()?;
    port.append_event(8)?;
    if commit {
        port.fault(ApprovalFault::T7)?;
    }
    port.install_snapshot(ApprovalRevision::Second)?;
    port.append_event(9)?;
    if commit {
        port.fault(ApprovalFault::T8BeforePointer)?;
    }
    port.install_pointer(ApprovalRevision::Second)?;
    if commit {
        port.fault(ApprovalFault::T8)?;
    }
    port.install_current()?;
    if commit {
        port.fault(ApprovalFault::T9)?;
    }
    port.finish_events()?;
    if let Err(error) = port.converge() {
        return Err(port.projection_repair_required(error));
    }
    if commit {
        port.fault(ApprovalFault::T10)?;
    }
    port.remove_projection_lag()?;
    port.validate_cleanup_authority()?;
    if commit {
        port.remove_journal()?;
    }
    Ok(())
}

pub(crate) fn execute_verification_transaction(
    port: &mut impl VerificationTransactionPort,
    execution: TransactionExecution,
) -> Result<(), AppError> {
    let commit = execution == TransactionExecution::Commit;
    if commit {
        port.fault(VerificationFault::V1)?;
    }
    port.append_event(0)?;
    if commit {
        port.fault(VerificationFault::V2)?;
    }
    port.install_snapshot()?;
    port.append_event(1)?;
    if commit {
        port.fault(VerificationFault::V3BeforePointer)?;
    }
    port.install_pointer()?;
    if commit {
        port.fault(VerificationFault::V3)?;
    }
    port.append_event(2)?;
    if commit {
        port.fault(VerificationFault::V4)?;
    }
    port.install_current()?;
    if commit {
        port.fault(VerificationFault::V5)?;
    }
    port.finish_events()?;
    port.converge()?;
    if commit {
        port.fault(VerificationFault::V6)?;
        port.remove_journal()?;
    }
    Ok(())
}

pub(crate) fn execute_terminal_action_transaction(
    port: &mut impl TerminalActionTransactionPort,
    execution: TransactionExecution,
) -> Result<(), AppError> {
    let commit = execution == TransactionExecution::Commit;
    if commit {
        port.fault(TerminalActionFault::Journal)?;
    }
    port.append_event(0)?;
    if commit {
        port.fault(TerminalActionFault::Intent)?;
    }
    port.install_source()?;
    if commit {
        port.fault(TerminalActionFault::Source)?;
    }
    port.install_snapshot()?;
    port.append_event(1)?;
    if commit {
        port.fault(TerminalActionFault::Snapshot)?;
    }
    port.install_pointer()?;
    if commit {
        port.fault(TerminalActionFault::Pointer)?;
    }
    port.append_event(2)?;
    port.finish_events()?;
    if commit {
        port.fault(TerminalActionFault::Ledger)?;
    }
    port.install_current()?;
    if commit {
        port.fault(TerminalActionFault::Current)?;
    }
    port.converge()?;
    if commit {
        port.fault(TerminalActionFault::Projection)?;
        port.remove_journal()?;
    }
    Ok(())
}

pub(crate) fn execute_state_transition(
    port: &mut impl StateTransitionTransactionPort,
    checkpoint: bool,
) -> Result<(), AppError> {
    port.fault(StateTransitionFault::Journal)?;
    if checkpoint {
        port.fault(StateTransitionFault::CheckpointTransaction)?;
    }
    port.install_snapshot()?;
    if checkpoint {
        port.fault(StateTransitionFault::CheckpointSnapshot)?;
    }
    port.fault(StateTransitionFault::Artifacts)?;
    port.append_event()?;
    port.fault(StateTransitionFault::Ledger)?;
    if checkpoint {
        port.fault(StateTransitionFault::CheckpointLedger)?;
    }
    port.install_pointer()?;
    if checkpoint {
        port.fault(StateTransitionFault::CheckpointPointer)?;
    }
    port.finish_events()?;
    port.install_current()?;
    port.fault(StateTransitionFault::Current)?;
    port.converge()?;
    port.fault(StateTransitionFault::Projection)?;
    port.remove_journal()
}

pub(crate) fn execute_reconcile_transaction(
    port: &mut impl ReconcileTransactionPort,
) -> Result<(), AppError> {
    port.fault(StateTransitionFault::Journal)?;
    port.install_backup()?;
    port.fault(StateTransitionFault::Artifacts)?;
    port.append_event()?;
    port.fault(StateTransitionFault::Ledger)?;
    port.finish_events()?;
    port.install_current()?;
    port.fault(StateTransitionFault::Current)?;
    port.converge()?;
    port.fault(StateTransitionFault::Projection)?;
    port.remove_journal()
}

impl<'plan> TransactionCoordinator<'plan> {
    pub(crate) fn new(planned: &'plan [PlannedEvent]) -> Self {
        Self {
            planned,
            next_index: 0,
        }
    }

    pub(crate) fn validate_next(
        &self,
        index: usize,
        event: &LedgerEvent,
    ) -> Result<&'plan PlannedEvent, AppError> {
        if index != self.next_index {
            return Err(AppError::blocked(format!(
                "transaction event sink 순서 불일치\n- expected index: {}\n- requested index: {index}",
                self.next_index
            )));
        }
        let planned = self
            .planned
            .get(index)
            .ok_or_else(|| AppError::blocked("transaction event sink index 범위 초과"))?;
        if &planned.event != event {
            return Err(AppError::blocked(
                "transaction event sink semantic event binding 불일치",
            ));
        }
        Ok(planned)
    }

    pub(crate) fn record_appended(&mut self, index: usize) -> Result<(), AppError> {
        if index != self.next_index {
            return Err(AppError::blocked(format!(
                "transaction event sink 순서 불일치\n- expected index: {}\n- requested index: {index}",
                self.next_index
            )));
        }
        self.next_index = self
            .next_index
            .checked_add(1)
            .ok_or_else(|| AppError::blocked("transaction event sink index overflow"))?;
        Ok(())
    }

    pub(crate) fn finish(&self) -> Result<(), AppError> {
        if self.next_index != self.planned.len() {
            return Err(AppError::blocked(format!(
                "transaction event sink 미완료\n- appended: {}\n- planned: {}",
                self.next_index,
                self.planned.len()
            )));
        }
        Ok(())
    }

    pub(crate) fn planned(&self) -> &'plan [PlannedEvent] {
        self.planned
    }
}

#[cfg(test)]
mod tests {
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
}
