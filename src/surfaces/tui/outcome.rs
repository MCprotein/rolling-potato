use crate::foundation::error::AppError;
use crate::surfaces::tui::runtime_bridge::{OneShotSecret, TuiFreshness};

mod oracle;

pub(crate) use oracle::exact_tui_outcome;

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

pub(crate) fn unsupported_source_platform_outcome(platform: &str) -> Result<TuiOutcome, AppError> {
    exact_tui_outcome(
        TuiOutcomeCode::SourceInstallUnsupportedPlatform,
        TuiOutcomeContext {
            platform: Some(platform),
            ..TuiOutcomeContext::default()
        },
    )
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
