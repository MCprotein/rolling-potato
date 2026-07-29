//! Ports and value objects shared by workflow transaction use cases.

use crate::foundation::error::AppError;

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
