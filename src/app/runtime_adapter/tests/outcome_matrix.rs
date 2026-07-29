#[test]
fn denial_truth_table_outcome_mapping_is_total() {
    let codes = [
        TuiOutcomeCode::DenyPatchAccepted,
        TuiOutcomeCode::DenyVerificationRolledBack,
        TuiOutcomeCode::DenyBlockedNotPending,
        TuiOutcomeCode::DenyBlockedTerminalState,
        TuiOutcomeCode::RollbackConflict,
        TuiOutcomeCode::CancelAccepted,
        TuiOutcomeCode::CancelPhaseBlocked,
        TuiOutcomeCode::CancelTerminalBlocked,
        TuiOutcomeCode::CancelNoActiveWorkflow,
        TuiOutcomeCode::ResumeAccepted,
        TuiOutcomeCode::ResumeStaleSelection,
        TuiOutcomeCode::ResumeCorruptState,
        TuiOutcomeCode::ResumeInconclusiveEffect,
        TuiOutcomeCode::SecretRefreshOnly,
        TuiOutcomeCode::TerminalCapabilitySizeRead,
        TuiOutcomeCode::TerminalCapabilityModeRead,
        TuiOutcomeCode::TerminalNoEchoSetFailed,
        TuiOutcomeCode::TerminalSecretReadFailed,
        TuiOutcomeCode::TerminalFrameWritePreDispatch,
        TuiOutcomeCode::TerminalFrameWritePostDispatch,
        TuiOutcomeCode::SourceInstallRecoveryRequired,
        TuiOutcomeCode::SourceInstallRecoveryConflict,
        TuiOutcomeCode::SourceInstallRecoveryComplete,
        TuiOutcomeCode::ProjectionRepairRequired,
        TuiOutcomeCode::ProjectionLagInstallFailed,
        TuiOutcomeCode::ProjectionRepairComplete,
        TuiOutcomeCode::SourceInstallUnsupportedPlatform,
    ];
    let unique = codes
        .iter()
        .map(|code| code.as_str())
        .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(codes.len(), 27);
    assert_eq!(unique.len(), 27);
    for (phase, code) in [
        ("pending-approval", TuiOutcomeCode::DenyPatchAccepted),
        (
            "pending-verification-approval",
            TuiOutcomeCode::DenyVerificationRolledBack,
        ),
        ("approved", TuiOutcomeCode::DenyBlockedNotPending),
        (
            "verification-approved",
            TuiOutcomeCode::DenyBlockedNotPending,
        ),
        (
            "verification-started",
            TuiOutcomeCode::DenyBlockedNotPending,
        ),
        ("verified", TuiOutcomeCode::DenyBlockedNotPending),
        ("complete", TuiOutcomeCode::DenyBlockedTerminalState),
        ("failed", TuiOutcomeCode::DenyBlockedTerminalState),
        ("cancelled", TuiOutcomeCode::DenyBlockedTerminalState),
    ] {
        assert_eq!(
            patch::denial_phase_outcome_code(phase),
            Some(code),
            "production denial dispatch mismatch for phase: {phase}"
        );
    }
    assert_eq!(patch::denial_phase_outcome_code("unknown"), None);
}
