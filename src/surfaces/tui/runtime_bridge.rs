pub(crate) const TUI_MAX_ITEMS: usize = 120;
pub(crate) const TUI_MAX_CHARS: usize = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TuiReadBudget {
    pub(crate) max_items: usize,
    pub(crate) max_chars: usize,
}

impl TuiReadBudget {
    pub(crate) fn bounded(max_items: usize, max_chars: usize) -> Self {
        Self {
            max_items: max_items.clamp(1, TUI_MAX_ITEMS),
            max_chars: max_chars.clamp(1, TUI_MAX_CHARS),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TuiReadRequest {
    Overview {
        budget: TuiReadBudget,
    },
    Monitor {
        budget: TuiReadBudget,
    },
    Sessions {
        page: u64,
        budget: TuiReadBudget,
    },
    Transcript {
        session_id: String,
        page: u64,
        budget: TuiReadBudget,
    },
    ToolOutput {
        artifact_id: String,
        page: u64,
        budget: TuiReadBudget,
    },
    Approvals {
        page: u64,
        budget: TuiReadBudget,
    },
    Diff {
        proposal_id: String,
        page: u64,
        budget: TuiReadBudget,
    },
    Evidence {
        page: u64,
        budget: TuiReadBudget,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TuiReadPage {
    pub(crate) title: String,
    pub(crate) lines: Vec<String>,
    pub(crate) page: u64,
    pub(crate) has_previous: bool,
    pub(crate) has_next: bool,
    pub(crate) freshness: TuiFreshness,
    pub(crate) continuation: TuiReadContinuation,
    pub(crate) authority: TuiReadAuthority,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TuiReadContinuation {
    Complete,
    NextPage,
    Truncated,
    Unavailable,
    Redacted,
}

impl TuiReadContinuation {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::NextPage => "next-page",
            Self::Truncated => "truncated",
            Self::Unavailable => "unavailable",
            Self::Redacted => "redacted",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct TuiReadAuthority {
    pub(crate) current_revision: Option<u64>,
    pub(crate) current_hash: Option<String>,
    pub(crate) workflow_revision: Option<u64>,
    pub(crate) workflow_hash: Option<String>,
    pub(crate) ledger_sequence: Option<u64>,
    pub(crate) ledger_hash: Option<String>,
    pub(crate) projected_sequence: Option<u64>,
    pub(crate) content_hash: Option<String>,
    pub(crate) transcript_hash: Option<String>,
    pub(crate) validated_at_ms: Option<u128>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectionLease {
    pub(crate) project_id: String,
    pub(crate) session_id: String,
    pub(crate) selected_object_id: String,
    pub(crate) current_revision: u64,
    pub(crate) current_hash: String,
    pub(crate) active_session_id: String,
    pub(crate) active_workflow: Option<ObservedWorkflow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObservedWorkflow {
    pub(crate) workflow_id: String,
    pub(crate) revision: u64,
    pub(crate) hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TuiGateKind {
    PatchApply,
    VerificationCommand,
}

pub(crate) enum TuiIntent {
    #[allow(dead_code)]
    Refresh { request: TuiReadRequest },
    #[allow(dead_code)]
    Inspect { request: TuiReadRequest },
    ApprovePatch {
        intent_id: String,
        proposal_id: String,
        lease: SelectionLease,
        secret: OneShotSecret,
    },
    ApproveVerification {
        intent_id: String,
        proposal_id: String,
        lease: SelectionLease,
        secret: OneShotSecret,
    },
    DenyPendingGate {
        intent_id: String,
        workflow_id: String,
        gate_id: String,
        gate_kind: TuiGateKind,
        lease: SelectionLease,
    },
    ResumeWorkflow {
        intent_id: String,
        workflow_id: String,
        lease: SelectionLease,
    },
    CancelWorkflow {
        intent_id: String,
        workflow_id: String,
        lease: SelectionLease,
    },
    #[allow(dead_code)]
    SelectSession {
        intent_id: String,
        session_id: String,
        lease: SelectionLease,
    },
    #[allow(dead_code)]
    ResumeSession {
        intent_id: String,
        session_id: String,
        lease: SelectionLease,
    },
}

pub(crate) struct OneShotSecret(Vec<u8>);

impl OneShotSecret {
    pub(crate) fn new(value: String) -> Result<Self, AppError> {
        if value.is_empty() {
            return Err(AppError::blocked(
                "비밀 입력 차단\n- 이유: 빈 비밀값은 사용할 수 없습니다.",
            ));
        }
        Ok(Self(value.into_bytes()))
    }

    pub(crate) fn expose<R>(self, use_plaintext: impl FnOnce(&str) -> R) -> R {
        let plaintext = std::str::from_utf8(&self.0)
            .expect("OneShotSecret is constructed only from valid UTF-8 String values");
        use_plaintext(plaintext)
    }
}

impl Drop for OneShotSecret {
    fn drop(&mut self) {
        for byte in &mut self.0 {
            // SAFETY: `byte` is a valid, uniquely borrowed byte in the owned buffer.
            unsafe { std::ptr::write_volatile(byte, 0) };
        }
        std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TuiOutcomeStatus {
    Succeeded,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TuiOutcomeCode {
    VerificationCredentialIssued,
    DenyPatchAccepted,
    DenyVerificationRolledBack,
    DenyBlockedNotPending,
    DenyBlockedTerminalState,
    RollbackConflict,
    CancelAccepted,
    CancelPhaseBlocked,
    CancelTerminalBlocked,
    CancelNoActiveWorkflow,
    ResumeAccepted,
    ResumeStaleSelection,
    ResumeCorruptState,
    ResumeInconclusiveEffect,
    SecretRefreshOnly,
    TerminalCapabilitySizeRead,
    TerminalCapabilityModeRead,
    TerminalNoEchoSetFailed,
    TerminalSecretReadFailed,
    TerminalFrameWritePreDispatch,
    TerminalFrameWritePostDispatch,
    SourceInstallRecoveryRequired,
    SourceInstallRecoveryConflict,
    SourceInstallRecoveryComplete,
    ProjectionRepairRequired,
    ProjectionLagInstallFailed,
    ProjectionRepairComplete,
    SourceInstallUnsupportedPlatform,
}

impl TuiOutcomeCode {
    pub(crate) const ALL: [Self; 27] = [
        Self::DenyPatchAccepted,
        Self::DenyVerificationRolledBack,
        Self::DenyBlockedNotPending,
        Self::DenyBlockedTerminalState,
        Self::RollbackConflict,
        Self::CancelAccepted,
        Self::CancelPhaseBlocked,
        Self::CancelTerminalBlocked,
        Self::CancelNoActiveWorkflow,
        Self::ResumeAccepted,
        Self::ResumeStaleSelection,
        Self::ResumeCorruptState,
        Self::ResumeInconclusiveEffect,
        Self::SecretRefreshOnly,
        Self::TerminalCapabilitySizeRead,
        Self::TerminalCapabilityModeRead,
        Self::TerminalNoEchoSetFailed,
        Self::TerminalSecretReadFailed,
        Self::TerminalFrameWritePreDispatch,
        Self::TerminalFrameWritePostDispatch,
        Self::SourceInstallRecoveryRequired,
        Self::SourceInstallRecoveryConflict,
        Self::SourceInstallRecoveryComplete,
        Self::ProjectionRepairRequired,
        Self::ProjectionLagInstallFailed,
        Self::ProjectionRepairComplete,
        Self::SourceInstallUnsupportedPlatform,
    ];

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::VerificationCredentialIssued => "verification.credential-issued",
            Self::DenyPatchAccepted => "deny.patch.accepted",
            Self::DenyVerificationRolledBack => "deny.verification.rolled-back",
            Self::DenyBlockedNotPending => "deny.blocked.not-pending",
            Self::DenyBlockedTerminalState => "deny.blocked.terminal-state",
            Self::RollbackConflict => "rollback.conflict",
            Self::CancelAccepted => "cancel.accepted",
            Self::CancelPhaseBlocked => "cancel.phase-blocked",
            Self::CancelTerminalBlocked => "cancel.terminal-blocked",
            Self::CancelNoActiveWorkflow => "cancel.no-active-workflow",
            Self::ResumeAccepted => "resume.accepted",
            Self::ResumeStaleSelection => "resume.stale-selection",
            Self::ResumeCorruptState => "resume.corrupt-state",
            Self::ResumeInconclusiveEffect => "resume.inconclusive-effect",
            Self::SecretRefreshOnly => "secret.refresh-only",
            Self::TerminalCapabilitySizeRead => "terminal.capability.size-read",
            Self::TerminalCapabilityModeRead => "terminal.capability.mode-read",
            Self::TerminalNoEchoSetFailed => "terminal.no-echo-set.failed",
            Self::TerminalSecretReadFailed => "terminal.secret-read.failed",
            Self::TerminalFrameWritePreDispatch => "terminal.frame-write.pre-dispatch",
            Self::TerminalFrameWritePostDispatch => "terminal.frame-write.post-dispatch",
            Self::SourceInstallRecoveryRequired => "source-install.recovery-required",
            Self::SourceInstallRecoveryConflict => "source-install.recovery-conflict",
            Self::SourceInstallRecoveryComplete => "source-install.recovery-complete",
            Self::ProjectionRepairRequired => "projection.repair-required",
            Self::ProjectionLagInstallFailed => "projection.lag-install-failed",
            Self::ProjectionRepairComplete => "projection.repair-complete",
            Self::SourceInstallUnsupportedPlatform => "source-install.unsupported-platform",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TuiEffect {
    NotDispatched,
    Committed,
    RolledBack,
    RecoveryPending,
    Refreshed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TuiNextAction {
    InspectDeniedReceipt,
    InspectRollbackReceipt,
    UseCancelOrRefresh,
    InspectTerminalReceipt,
    ResolveRollbackConflict,
    RefreshCanonicalState,
    ChooseCancellablePhase,
    CloseOrInspectTerminal,
    SelectActiveWorkflow,
    RetryResumeAfterRefresh,
    RepairCorruptState,
    ResolveInconclusiveEffect,
    RefreshOnly,
    ReadOnly,
    RetryInput,
    RetryIntent,
    RepairSourceInstall,
    ResolveSourceConflict,
    RefreshSourceState,
    RepairProjection,
    RefreshProjection,
    UseUnixOrChooseNonSourceAction,
}

pub(crate) struct TuiOutcome {
    pub(crate) status: TuiOutcomeStatus,
    pub(crate) code: TuiOutcomeCode,
    pub(crate) effect: TuiEffect,
    pub(crate) safe_message: String,
    pub(crate) freshness: TuiFreshness,
    pub(crate) next_action: TuiNextAction,
    pub(crate) one_shot_secret: Option<OneShotSecret>,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct TuiOutcomeContext<'a> {
    pub(crate) intent_id: Option<&'a str>,
    pub(crate) workflow_id: Option<&'a str>,
    pub(crate) phase: Option<&'a str>,
    pub(crate) platform: Option<&'a str>,
}

impl TuiOutcome {
    pub(crate) fn without_secret(
        status: TuiOutcomeStatus,
        code: TuiOutcomeCode,
        effect: TuiEffect,
        safe_message: String,
        freshness: TuiFreshness,
        next_action: TuiNextAction,
    ) -> Self {
        Self {
            status,
            code,
            effect,
            safe_message,
            freshness,
            next_action,
            one_shot_secret: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TuiFreshness {
    Fresh,
    Stale,
    Unavailable,
    ProjectionLag,
}

impl TuiFreshness {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Fresh => "fresh",
            Self::Stale => "stale",
            Self::Unavailable => "unavailable",
            Self::ProjectionLag => "projection-lag",
        }
    }
}
use crate::foundation::error::AppError;

pub(crate) fn exact_tui_outcome(
    code: TuiOutcomeCode,
    context: TuiOutcomeContext<'_>,
) -> Result<TuiOutcome, AppError> {
    let (status, effect, freshness, next_action, safe_message) = match code {
        TuiOutcomeCode::VerificationCredentialIssued => {
            return Err(AppError::blocked(
                "verification credential 발급 결과는 exact recovery/refresh oracle에서 만들 수 없습니다.",
            ));
        }
        TuiOutcomeCode::DenyPatchAccepted => {
            let intent_id = required_outcome_id(context.intent_id, "intent")?;
            let workflow_id = required_outcome_id(context.workflow_id, "workflow")?;
            (
                TuiOutcomeStatus::Succeeded,
                TuiEffect::Committed,
                TuiFreshness::Fresh,
                TuiNextAction::InspectDeniedReceipt,
                format!(
                    "패치 적용 거부 완료\n- code: deny.patch.accepted\n- intent: {intent_id}\n- workflow: {workflow_id}\n- 동작: 소스 변경 없이 취소 상태를 기록했습니다.\n- 다음: 거부 영수증을 확인하세요."
                ),
            )
        }
        TuiOutcomeCode::DenyVerificationRolledBack => {
            let intent_id = required_outcome_id(context.intent_id, "intent")?;
            let workflow_id = required_outcome_id(context.workflow_id, "workflow")?;
            (
                TuiOutcomeStatus::Succeeded,
                TuiEffect::RolledBack,
                TuiFreshness::Fresh,
                TuiNextAction::InspectRollbackReceipt,
                format!(
                    "검증 거부 및 롤백 완료\n- code: deny.verification.rolled-back\n- intent: {intent_id}\n- workflow: {workflow_id}\n- 동작: 원본 해시를 검증하고 취소 상태를 기록했습니다.\n- 다음: 롤백 영수증을 확인하세요."
                ),
            )
        }
        TuiOutcomeCode::DenyBlockedNotPending => {
            let intent_id = required_outcome_id(context.intent_id, "intent")?;
            let workflow_id = required_outcome_id(context.workflow_id, "workflow")?;
            let phase = required_outcome_phase(context.phase)?;
            (
                TuiOutcomeStatus::Blocked,
                TuiEffect::NotDispatched,
                TuiFreshness::Fresh,
                TuiNextAction::UseCancelOrRefresh,
                format!(
                    "승인 대기 상태가 아니어서 거부 차단\n- code: deny.blocked.not-pending\n- intent: {intent_id}\n- workflow: {workflow_id}\n- phase: {phase}\n- 동작: 승인 상태와 효과를 변경하지 않았습니다.\n- 다음: 취소를 사용하거나 정본 상태를 새로고침하세요."
                ),
            )
        }
        TuiOutcomeCode::DenyBlockedTerminalState => {
            let intent_id = required_outcome_id(context.intent_id, "intent")?;
            let workflow_id = required_outcome_id(context.workflow_id, "workflow")?;
            let phase = required_outcome_phase(context.phase)?;
            (
                TuiOutcomeStatus::Blocked,
                TuiEffect::NotDispatched,
                TuiFreshness::Fresh,
                TuiNextAction::InspectTerminalReceipt,
                format!(
                    "종료 상태여서 거부 차단\n- code: deny.blocked.terminal-state\n- intent: {intent_id}\n- workflow: {workflow_id}\n- phase: {phase}\n- 동작: 종료 상태와 영수증을 변경하지 않았습니다.\n- 다음: 기존 종료 영수증을 확인하세요."
                ),
            )
        }
        TuiOutcomeCode::RollbackConflict => {
            let intent_id = required_outcome_id(context.intent_id, "intent")?;
            let workflow_id = required_outcome_id(context.workflow_id, "workflow")?;
            (
                TuiOutcomeStatus::Blocked,
                TuiEffect::NotDispatched,
                TuiFreshness::Stale,
                TuiNextAction::ResolveRollbackConflict,
                format!(
                    "롤백 충돌로 차단됨\n- code: rollback.conflict\n- intent: {intent_id}\n- workflow: {workflow_id}\n- 동작: 현재 포인터와 소스는 변경하지 않았습니다.\n- 다음: 소스 충돌을 해결한 뒤 다시 읽으세요."
                ),
            )
        }
        TuiOutcomeCode::CancelAccepted => {
            let intent_id = required_outcome_id(context.intent_id, "intent")?;
            let workflow_id = required_outcome_id(context.workflow_id, "workflow")?;
            (
                TuiOutcomeStatus::Succeeded,
                TuiEffect::Committed,
                TuiFreshness::Fresh,
                TuiNextAction::RefreshCanonicalState,
                format!(
                    "워크플로 취소 완료\n- code: cancel.accepted\n- intent: {intent_id}\n- workflow: {workflow_id}\n- 동작: 취소 상태를 기록했습니다.\n- 다음: 정본 상태를 새로고침하세요."
                ),
            )
        }
        TuiOutcomeCode::CancelPhaseBlocked => {
            let workflow_id = required_outcome_id(context.workflow_id, "workflow")?;
            let phase = required_outcome_phase(context.phase)?;
            (
                TuiOutcomeStatus::Blocked,
                TuiEffect::NotDispatched,
                TuiFreshness::Fresh,
                TuiNextAction::ChooseCancellablePhase,
                format!(
                    "현재 단계에서는 취소할 수 없음\n- code: cancel.phase-blocked\n- workflow: {workflow_id}\n- phase: {phase}\n- 동작: 상태를 변경하지 않았습니다.\n- 다음: 취소 가능한 단계를 확인하세요."
                ),
            )
        }
        TuiOutcomeCode::CancelTerminalBlocked => {
            let workflow_id = required_outcome_id(context.workflow_id, "workflow")?;
            let phase = required_outcome_phase(context.phase)?;
            (
                TuiOutcomeStatus::Blocked,
                TuiEffect::NotDispatched,
                TuiFreshness::Fresh,
                TuiNextAction::CloseOrInspectTerminal,
                format!(
                    "종료된 워크플로는 취소할 수 없음\n- code: cancel.terminal-blocked\n- workflow: {workflow_id}\n- phase: {phase}\n- 동작: 종료 상태를 유지했습니다.\n- 다음: 종료 영수증을 확인하세요."
                ),
            )
        }
        TuiOutcomeCode::CancelNoActiveWorkflow => (
            TuiOutcomeStatus::Blocked,
            TuiEffect::NotDispatched,
            TuiFreshness::Unavailable,
            TuiNextAction::SelectActiveWorkflow,
            "취소할 활성 워크플로가 없음\n- code: cancel.no-active-workflow\n- 동작: 상태를 변경하지 않았습니다.\n- 다음: 활성 워크플로를 선택하세요."
                .to_string(),
        ),
        TuiOutcomeCode::ResumeAccepted => {
            let intent_id = required_outcome_id(context.intent_id, "intent")?;
            let workflow_id = required_outcome_id(context.workflow_id, "workflow")?;
            (
                TuiOutcomeStatus::Succeeded,
                TuiEffect::Committed,
                TuiFreshness::Fresh,
                TuiNextAction::RefreshCanonicalState,
                format!(
                    "워크플로 재개 완료\n- code: resume.accepted\n- intent: {intent_id}\n- workflow: {workflow_id}\n- 동작: 검증된 정본 상태에서 재개했습니다.\n- 다음: 정본 상태를 새로고침하세요."
                ),
            )
        }
        TuiOutcomeCode::ResumeStaleSelection => {
            let workflow_id = required_outcome_id(context.workflow_id, "workflow")?;
            (
                TuiOutcomeStatus::Blocked,
                TuiEffect::NotDispatched,
                TuiFreshness::Stale,
                TuiNextAction::RetryResumeAfterRefresh,
                format!(
                    "오래된 선택으로 재개 차단\n- code: resume.stale-selection\n- workflow: {workflow_id}\n- 동작: 상태를 변경하거나 효과를 재실행하지 않았습니다.\n- 다음: 새로고침 후 다시 선택하세요."
                ),
            )
        }
        TuiOutcomeCode::ResumeCorruptState => {
            let workflow_id = required_outcome_id(context.workflow_id, "workflow")?;
            (
                TuiOutcomeStatus::Blocked,
                TuiEffect::NotDispatched,
                TuiFreshness::Unavailable,
                TuiNextAction::RepairCorruptState,
                format!(
                    "손상된 상태로 재개 차단\n- code: resume.corrupt-state\n- workflow: {workflow_id}\n- 동작: 상태와 효과를 변경하지 않았습니다.\n- 다음: 정본 상태와 해시를 복구하세요."
                ),
            )
        }
        TuiOutcomeCode::ResumeInconclusiveEffect => {
            let workflow_id = required_outcome_id(context.workflow_id, "workflow")?;
            let phase = required_outcome_phase(context.phase)?;
            (
                TuiOutcomeStatus::Blocked,
                TuiEffect::RecoveryPending,
                TuiFreshness::Stale,
                TuiNextAction::ResolveInconclusiveEffect,
                format!(
                    "불확실한 효과로 자동 재개 차단\n- code: resume.inconclusive-effect\n- workflow: {workflow_id}\n- phase: {phase}\n- 동작: 모델 또는 검증 명령을 재실행하지 않았습니다.\n- 다음: 효과를 확인하고 명시적으로 정리하세요."
                ),
            )
        }
        TuiOutcomeCode::SecretRefreshOnly => {
            let intent_id = required_outcome_id(context.intent_id, "intent")?;
            (
                TuiOutcomeStatus::Succeeded,
                TuiEffect::Committed,
                TuiFreshness::Fresh,
                TuiNextAction::RefreshOnly,
                format!(
                    "커밋 완료, 비밀값 재표시 불가\n- code: secret.refresh-only\n- intent: {intent_id}\n- 동작: 커밋 영수증만 반환합니다.\n- 다음: 비밀값 없이 상태를 새로고침하세요."
                ),
            )
        }
        TuiOutcomeCode::TerminalCapabilitySizeRead => (
            TuiOutcomeStatus::Blocked,
            TuiEffect::NotDispatched,
            TuiFreshness::Unavailable,
            TuiNextAction::ReadOnly,
            "터미널 크기 확인 실패\n- code: terminal.capability.size-read\n- 동작: 런타임 요청을 보내지 않았습니다.\n- 다음: 읽기 전용 모드를 사용하세요."
                .to_string(),
        ),
        TuiOutcomeCode::TerminalCapabilityModeRead => (
            TuiOutcomeStatus::Blocked,
            TuiEffect::NotDispatched,
            TuiFreshness::Unavailable,
            TuiNextAction::ReadOnly,
            "터미널 모드 확인 실패\n- code: terminal.capability.mode-read\n- 동작: 모드와 상태를 변경하지 않았습니다.\n- 다음: 터미널 모드를 확인한 뒤 다시 시도하세요."
                .to_string(),
        ),
        TuiOutcomeCode::TerminalNoEchoSetFailed => (
            TuiOutcomeStatus::Blocked,
            TuiEffect::NotDispatched,
            TuiFreshness::Unavailable,
            TuiNextAction::ReadOnly,
            "비밀 입력 보호 설정 실패\n- code: terminal.no-echo-set.failed\n- 동작: 비밀값을 읽거나 요청을 보내지 않았습니다.\n- 다음: 무반향 입력을 복구하세요."
                .to_string(),
        ),
        TuiOutcomeCode::TerminalSecretReadFailed => (
            TuiOutcomeStatus::Blocked,
            TuiEffect::NotDispatched,
            TuiFreshness::Unavailable,
            TuiNextAction::RetryInput,
            "비밀 입력 읽기 실패\n- code: terminal.secret-read.failed\n- 동작: 비밀값을 수락하거나 저장하지 않았습니다.\n- 다음: 새 입력으로 다시 시도하세요."
                .to_string(),
        ),
        TuiOutcomeCode::TerminalFrameWritePreDispatch => {
            let intent_id = required_outcome_id(context.intent_id, "intent")?;
            (
                TuiOutcomeStatus::Blocked,
                TuiEffect::NotDispatched,
                TuiFreshness::Stale,
                TuiNextAction::RetryIntent,
                format!(
                    "요청 전 화면 출력 실패\n- code: terminal.frame-write.pre-dispatch\n- intent: {intent_id}\n- 동작: 런타임 요청을 보내지 않았습니다.\n- 다음: 정본 상태를 다시 읽고 요청하세요."
                ),
            )
        }
        TuiOutcomeCode::TerminalFrameWritePostDispatch => {
            let intent_id = required_outcome_id(context.intent_id, "intent")?;
            (
                TuiOutcomeStatus::Blocked,
                TuiEffect::Committed,
                TuiFreshness::Stale,
                TuiNextAction::RefreshOnly,
                format!(
                    "커밋 후 화면 출력 실패\n- code: terminal.frame-write.post-dispatch\n- intent: {intent_id}\n- 동작: 요청을 다시 보내지 않습니다.\n- 다음: 영수증을 새로고침하세요."
                ),
            )
        }
        TuiOutcomeCode::SourceInstallRecoveryRequired => {
            let intent_id = required_outcome_id(context.intent_id, "intent")?;
            (
                TuiOutcomeStatus::Blocked,
                TuiEffect::RecoveryPending,
                TuiFreshness::Stale,
                TuiNextAction::RepairSourceInstall,
                format!(
                    "소스 설치 복구 필요\n- code: source-install.recovery-required\n- intent: {intent_id}\n- 동작: 저널과 복구 증거를 보존했습니다.\n- 다음: 동일 저널로 복구를 실행하세요."
                ),
            )
        }
        TuiOutcomeCode::SourceInstallRecoveryConflict => {
            let intent_id = required_outcome_id(context.intent_id, "intent")?;
            (
                TuiOutcomeStatus::Blocked,
                TuiEffect::RecoveryPending,
                TuiFreshness::Unavailable,
                TuiNextAction::ResolveSourceConflict,
                format!(
                    "소스 설치 복구 충돌\n- code: source-install.recovery-conflict\n- intent: {intent_id}\n- 동작: 대상과 저널을 덮어쓰지 않았습니다.\n- 다음: 경로와 해시 충돌을 해결하세요."
                ),
            )
        }
        TuiOutcomeCode::SourceInstallRecoveryComplete => {
            let intent_id = required_outcome_id(context.intent_id, "intent")?;
            (
                TuiOutcomeStatus::Succeeded,
                TuiEffect::Refreshed,
                TuiFreshness::Fresh,
                TuiNextAction::RefreshSourceState,
                format!(
                    "소스 설치 복구 완료\n- code: source-install.recovery-complete\n- intent: {intent_id}\n- 동작: 준비된 바이트로 정확히 수렴했습니다.\n- 다음: 소스 상태를 새로고침하세요."
                ),
            )
        }
        TuiOutcomeCode::ProjectionRepairRequired => {
            let intent_id = required_outcome_id(context.intent_id, "intent")?;
            (
                TuiOutcomeStatus::Blocked,
                TuiEffect::RecoveryPending,
                TuiFreshness::ProjectionLag,
                TuiNextAction::RepairProjection,
                format!(
                    "파생 출력 복구 필요\n- code: projection.repair-required\n- intent: {intent_id}\n- 동작: 저널과 지연 표식을 보존했습니다.\n- 다음: project ledger, operation log, SQLite 순서로 복구하세요."
                ),
            )
        }
        TuiOutcomeCode::ProjectionLagInstallFailed => {
            let intent_id = required_outcome_id(context.intent_id, "intent")?;
            (
                TuiOutcomeStatus::Blocked,
                TuiEffect::RecoveryPending,
                TuiFreshness::ProjectionLag,
                TuiNextAction::RepairProjection,
                format!(
                    "지연 표식 설치 실패\n- code: projection.lag-install-failed\n- intent: {intent_id}\n- 동작: 저널을 보존하고 정리를 중단했습니다.\n- 다음: 지연 표식을 다시 설치한 뒤 파생 출력을 복구하세요."
                ),
            )
        }
        TuiOutcomeCode::ProjectionRepairComplete => {
            let intent_id = required_outcome_id(context.intent_id, "intent")?;
            (
                TuiOutcomeStatus::Succeeded,
                TuiEffect::Refreshed,
                TuiFreshness::Fresh,
                TuiNextAction::RefreshProjection,
                format!(
                    "파생 출력 복구 완료\n- code: projection.repair-complete\n- intent: {intent_id}\n- 동작: 지연 표식과 저널 정리를 완료했습니다.\n- 다음: 투영 상태를 새로고침하세요."
                ),
            )
        }
        TuiOutcomeCode::SourceInstallUnsupportedPlatform => {
            let platform = required_outcome_platform(context.platform)?;
            (
                TuiOutcomeStatus::Blocked,
                TuiEffect::NotDispatched,
                TuiFreshness::Fresh,
                TuiNextAction::UseUnixOrChooseNonSourceAction,
                format!(
                    "source install 차단\n- code: source-install.unsupported-platform\n- platform: {platform}\n- 지원 범위: v0.34.0 source installation은 Unix만 지원합니다.\n- 동작: journal/temp/guard/rollback/target 변경 없음"
                ),
            )
        }
    };

    Ok(TuiOutcome::without_secret(
        status,
        code,
        effect,
        safe_message,
        freshness,
        next_action,
    ))
}

pub(crate) fn verification_credential_issued(
    intent_id: &str,
    credential: OneShotSecret,
) -> Result<TuiOutcome, AppError> {
    validate_tui_id(intent_id, "intent")?;
    let mut outcome = TuiOutcome::without_secret(
        TuiOutcomeStatus::Succeeded,
        TuiOutcomeCode::VerificationCredentialIssued,
        TuiEffect::Committed,
        format!(
            "검증 credential 발급 완료\n- code: verification.credential-issued\n- intent: {intent_id}\n- 동작: credential을 이번 응답에서만 한 번 표시합니다.\n- 다음: 새로고침 후 verification 승인을 선택하세요."
        ),
        TuiFreshness::Fresh,
        TuiNextAction::RefreshOnly,
    );
    outcome.one_shot_secret = Some(credential);
    Ok(outcome)
}

fn required_outcome_id<'a>(value: Option<&'a str>, kind: &str) -> Result<&'a str, AppError> {
    let value = value.ok_or_else(|| corrupt_outcome_placeholder(kind))?;
    if validate_tui_id(value, kind).is_ok() {
        Ok(value)
    } else {
        Err(corrupt_outcome_placeholder(kind))
    }
}

fn required_outcome_phase(value: Option<&str>) -> Result<&str, AppError> {
    let value = value.ok_or_else(|| corrupt_outcome_placeholder("phase"))?;
    if matches!(
        value,
        "pending-approval"
            | "approved"
            | "pending-verification-approval"
            | "verification-approved"
            | "verification-started"
            | "verified"
            | "complete"
            | "failed"
            | "cancelled"
    ) {
        Ok(value)
    } else {
        Err(corrupt_outcome_placeholder("phase"))
    }
}

fn required_outcome_platform(value: Option<&str>) -> Result<&str, AppError> {
    let value = value.ok_or_else(|| corrupt_outcome_placeholder("platform"))?;
    let valid = !value.is_empty()
        && value.len() <= 32
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        });
    if valid {
        Ok(value)
    } else {
        Err(corrupt_outcome_placeholder("platform"))
    }
}

fn corrupt_outcome_placeholder(kind: &str) -> AppError {
    AppError::blocked(format!(
        "TUI 결과 상태 검증 차단\n- code: outcome.corrupt-state\n- kind: {kind}\n- 동작: 신뢰할 수 없는 값을 출력하지 않았습니다."
    ))
}

pub(crate) fn validate_tui_id(value: &str, kind: &str) -> Result<(), AppError> {
    let valid = !value.is_empty()
        && value.len() <= 96
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.')
        });
    if valid {
        Ok(())
    } else {
        Err(AppError::blocked(format!(
            "TUI 식별자 검증 차단\n- kind: {kind}\n- 동작: 신뢰할 수 없는 식별자를 출력하지 않았습니다."
        )))
    }
}
