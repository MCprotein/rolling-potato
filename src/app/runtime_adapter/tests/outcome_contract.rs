#[test]
fn runtime_tui_outcome_oracle_all_families_exact_utf8() {
    let intent = "intent-outcome-0001";
    let workflow = "workflow-outcome-0001";
    let context = |phase| TuiOutcomeContext {
        intent_id: Some(intent),
        workflow_id: Some(workflow),
        phase: Some(phase),
        platform: Some("windows"),
    };
    let fixtures = [
        (
            TuiOutcomeCode::DenyPatchAccepted,
            context("pending-approval"),
            TuiOutcomeStatus::Succeeded,
            TuiEffect::Committed,
            TuiFreshness::Fresh,
            TuiNextAction::InspectDeniedReceipt,
            "패치 적용 거부 완료\n- code: deny.patch.accepted\n- intent: intent-outcome-0001\n- workflow: workflow-outcome-0001\n- 동작: 소스 변경 없이 취소 상태를 기록했습니다.\n- 다음: 거부 영수증을 확인하세요.",
        ),
        (
            TuiOutcomeCode::DenyVerificationRolledBack,
            context("pending-verification-approval"),
            TuiOutcomeStatus::Succeeded,
            TuiEffect::RolledBack,
            TuiFreshness::Fresh,
            TuiNextAction::InspectRollbackReceipt,
            "검증 거부 및 롤백 완료\n- code: deny.verification.rolled-back\n- intent: intent-outcome-0001\n- workflow: workflow-outcome-0001\n- 동작: 원본 해시를 검증하고 취소 상태를 기록했습니다.\n- 다음: 롤백 영수증을 확인하세요.",
        ),
        (
            TuiOutcomeCode::DenyBlockedNotPending,
            context("verification-started"),
            TuiOutcomeStatus::Blocked,
            TuiEffect::NotDispatched,
            TuiFreshness::Fresh,
            TuiNextAction::UseCancelOrRefresh,
            "승인 대기 상태가 아니어서 거부 차단\n- code: deny.blocked.not-pending\n- intent: intent-outcome-0001\n- workflow: workflow-outcome-0001\n- phase: verification-started\n- 동작: 승인 상태와 효과를 변경하지 않았습니다.\n- 다음: 취소를 사용하거나 정본 상태를 새로고침하세요.",
        ),
        (
            TuiOutcomeCode::DenyBlockedTerminalState,
            context("complete"),
            TuiOutcomeStatus::Blocked,
            TuiEffect::NotDispatched,
            TuiFreshness::Fresh,
            TuiNextAction::InspectTerminalReceipt,
            "종료 상태여서 거부 차단\n- code: deny.blocked.terminal-state\n- intent: intent-outcome-0001\n- workflow: workflow-outcome-0001\n- phase: complete\n- 동작: 종료 상태와 영수증을 변경하지 않았습니다.\n- 다음: 기존 종료 영수증을 확인하세요.",
        ),
        (
            TuiOutcomeCode::RollbackConflict,
            context("pending-verification-approval"),
            TuiOutcomeStatus::Blocked,
            TuiEffect::NotDispatched,
            TuiFreshness::Stale,
            TuiNextAction::ResolveRollbackConflict,
            "롤백 충돌로 차단됨\n- code: rollback.conflict\n- intent: intent-outcome-0001\n- workflow: workflow-outcome-0001\n- 동작: 현재 포인터와 소스는 변경하지 않았습니다.\n- 다음: 소스 충돌을 해결한 뒤 다시 읽으세요.",
        ),
        (
            TuiOutcomeCode::CancelAccepted,
            context("pending-approval"),
            TuiOutcomeStatus::Succeeded,
            TuiEffect::Committed,
            TuiFreshness::Fresh,
            TuiNextAction::RefreshCanonicalState,
            "워크플로 취소 완료\n- code: cancel.accepted\n- intent: intent-outcome-0001\n- workflow: workflow-outcome-0001\n- 동작: 취소 상태를 기록했습니다.\n- 다음: 정본 상태를 새로고침하세요.",
        ),
        (
            TuiOutcomeCode::CancelPhaseBlocked,
            context("verification-started"),
            TuiOutcomeStatus::Blocked,
            TuiEffect::NotDispatched,
            TuiFreshness::Fresh,
            TuiNextAction::ChooseCancellablePhase,
            "현재 단계에서는 취소할 수 없음\n- code: cancel.phase-blocked\n- workflow: workflow-outcome-0001\n- phase: verification-started\n- 동작: 상태를 변경하지 않았습니다.\n- 다음: 취소 가능한 단계를 확인하세요.",
        ),
        (
            TuiOutcomeCode::CancelTerminalBlocked,
            context("complete"),
            TuiOutcomeStatus::Blocked,
            TuiEffect::NotDispatched,
            TuiFreshness::Fresh,
            TuiNextAction::CloseOrInspectTerminal,
            "종료된 워크플로는 취소할 수 없음\n- code: cancel.terminal-blocked\n- workflow: workflow-outcome-0001\n- phase: complete\n- 동작: 종료 상태를 유지했습니다.\n- 다음: 종료 영수증을 확인하세요.",
        ),
        (
            TuiOutcomeCode::CancelNoActiveWorkflow,
            context("complete"),
            TuiOutcomeStatus::Blocked,
            TuiEffect::NotDispatched,
            TuiFreshness::Unavailable,
            TuiNextAction::SelectActiveWorkflow,
            "취소할 활성 워크플로가 없음\n- code: cancel.no-active-workflow\n- 동작: 상태를 변경하지 않았습니다.\n- 다음: 활성 워크플로를 선택하세요.",
        ),
        (
            TuiOutcomeCode::ResumeAccepted,
            context("pending-approval"),
            TuiOutcomeStatus::Succeeded,
            TuiEffect::Committed,
            TuiFreshness::Fresh,
            TuiNextAction::RefreshCanonicalState,
            "워크플로 재개 완료\n- code: resume.accepted\n- intent: intent-outcome-0001\n- workflow: workflow-outcome-0001\n- 동작: 검증된 정본 상태에서 재개했습니다.\n- 다음: 정본 상태를 새로고침하세요.",
        ),
        (
            TuiOutcomeCode::ResumeStaleSelection,
            context("pending-approval"),
            TuiOutcomeStatus::Blocked,
            TuiEffect::NotDispatched,
            TuiFreshness::Stale,
            TuiNextAction::RetryResumeAfterRefresh,
            "오래된 선택으로 재개 차단\n- code: resume.stale-selection\n- workflow: workflow-outcome-0001\n- 동작: 상태를 변경하거나 효과를 재실행하지 않았습니다.\n- 다음: 새로고침 후 다시 선택하세요.",
        ),
        (
            TuiOutcomeCode::ResumeCorruptState,
            context("pending-approval"),
            TuiOutcomeStatus::Blocked,
            TuiEffect::NotDispatched,
            TuiFreshness::Unavailable,
            TuiNextAction::RepairCorruptState,
            "손상된 상태로 재개 차단\n- code: resume.corrupt-state\n- workflow: workflow-outcome-0001\n- 동작: 상태와 효과를 변경하지 않았습니다.\n- 다음: 정본 상태와 해시를 복구하세요.",
        ),
        (
            TuiOutcomeCode::ResumeInconclusiveEffect,
            context("verification-started"),
            TuiOutcomeStatus::Blocked,
            TuiEffect::RecoveryPending,
            TuiFreshness::Stale,
            TuiNextAction::ResolveInconclusiveEffect,
            "불확실한 효과로 자동 재개 차단\n- code: resume.inconclusive-effect\n- workflow: workflow-outcome-0001\n- phase: verification-started\n- 동작: 모델 또는 검증 명령을 재실행하지 않았습니다.\n- 다음: 효과를 확인하고 명시적으로 정리하세요.",
        ),
        (
            TuiOutcomeCode::SecretRefreshOnly,
            context("pending-approval"),
            TuiOutcomeStatus::Succeeded,
            TuiEffect::Committed,
            TuiFreshness::Fresh,
            TuiNextAction::RefreshOnly,
            "커밋 완료, 비밀값 재표시 불가\n- code: secret.refresh-only\n- intent: intent-outcome-0001\n- 동작: 커밋 영수증만 반환합니다.\n- 다음: 비밀값 없이 상태를 새로고침하세요.",
        ),
        (
            TuiOutcomeCode::TerminalCapabilitySizeRead,
            context("pending-approval"),
            TuiOutcomeStatus::Blocked,
            TuiEffect::NotDispatched,
            TuiFreshness::Unavailable,
            TuiNextAction::ReadOnly,
            "터미널 크기 확인 실패\n- code: terminal.capability.size-read\n- 동작: 런타임 요청을 보내지 않았습니다.\n- 다음: 읽기 전용 모드를 사용하세요.",
        ),
        (
            TuiOutcomeCode::TerminalCapabilityModeRead,
            context("pending-approval"),
            TuiOutcomeStatus::Blocked,
            TuiEffect::NotDispatched,
            TuiFreshness::Unavailable,
            TuiNextAction::ReadOnly,
            "터미널 모드 확인 실패\n- code: terminal.capability.mode-read\n- 동작: 모드와 상태를 변경하지 않았습니다.\n- 다음: 터미널 모드를 확인한 뒤 다시 시도하세요.",
        ),
        (
            TuiOutcomeCode::TerminalNoEchoSetFailed,
            context("pending-approval"),
            TuiOutcomeStatus::Blocked,
            TuiEffect::NotDispatched,
            TuiFreshness::Unavailable,
            TuiNextAction::ReadOnly,
            "비밀 입력 보호 설정 실패\n- code: terminal.no-echo-set.failed\n- 동작: 비밀값을 읽거나 요청을 보내지 않았습니다.\n- 다음: 무반향 입력을 복구하세요.",
        ),
        (
            TuiOutcomeCode::TerminalSecretReadFailed,
            context("pending-approval"),
            TuiOutcomeStatus::Blocked,
            TuiEffect::NotDispatched,
            TuiFreshness::Unavailable,
            TuiNextAction::RetryInput,
            "비밀 입력 읽기 실패\n- code: terminal.secret-read.failed\n- 동작: 비밀값을 수락하거나 저장하지 않았습니다.\n- 다음: 새 입력으로 다시 시도하세요.",
        ),
        (
            TuiOutcomeCode::TerminalFrameWritePreDispatch,
            context("pending-approval"),
            TuiOutcomeStatus::Blocked,
            TuiEffect::NotDispatched,
            TuiFreshness::Stale,
            TuiNextAction::RetryIntent,
            "요청 전 화면 출력 실패\n- code: terminal.frame-write.pre-dispatch\n- intent: intent-outcome-0001\n- 동작: 런타임 요청을 보내지 않았습니다.\n- 다음: 정본 상태를 다시 읽고 요청하세요.",
        ),
        (
            TuiOutcomeCode::TerminalFrameWritePostDispatch,
            context("pending-approval"),
            TuiOutcomeStatus::Blocked,
            TuiEffect::Committed,
            TuiFreshness::Stale,
            TuiNextAction::RefreshOnly,
            "커밋 후 화면 출력 실패\n- code: terminal.frame-write.post-dispatch\n- intent: intent-outcome-0001\n- 동작: 요청을 다시 보내지 않습니다.\n- 다음: 영수증을 새로고침하세요.",
        ),
        (
            TuiOutcomeCode::SourceInstallRecoveryRequired,
            context("pending-approval"),
            TuiOutcomeStatus::Blocked,
            TuiEffect::RecoveryPending,
            TuiFreshness::Stale,
            TuiNextAction::RepairSourceInstall,
            "소스 설치 복구 필요\n- code: source-install.recovery-required\n- intent: intent-outcome-0001\n- 동작: 저널과 복구 증거를 보존했습니다.\n- 다음: 동일 저널로 복구를 실행하세요.",
        ),
        (
            TuiOutcomeCode::SourceInstallRecoveryConflict,
            context("pending-approval"),
            TuiOutcomeStatus::Blocked,
            TuiEffect::RecoveryPending,
            TuiFreshness::Unavailable,
            TuiNextAction::ResolveSourceConflict,
            "소스 설치 복구 충돌\n- code: source-install.recovery-conflict\n- intent: intent-outcome-0001\n- 동작: 대상과 저널을 덮어쓰지 않았습니다.\n- 다음: 경로와 해시 충돌을 해결하세요.",
        ),
        (
            TuiOutcomeCode::SourceInstallRecoveryComplete,
            context("pending-approval"),
            TuiOutcomeStatus::Succeeded,
            TuiEffect::Refreshed,
            TuiFreshness::Fresh,
            TuiNextAction::RefreshSourceState,
            "소스 설치 복구 완료\n- code: source-install.recovery-complete\n- intent: intent-outcome-0001\n- 동작: 준비된 바이트로 정확히 수렴했습니다.\n- 다음: 소스 상태를 새로고침하세요.",
        ),
        (
            TuiOutcomeCode::ProjectionRepairRequired,
            context("pending-approval"),
            TuiOutcomeStatus::Blocked,
            TuiEffect::RecoveryPending,
            TuiFreshness::ProjectionLag,
            TuiNextAction::RepairProjection,
            "파생 출력 복구 필요\n- code: projection.repair-required\n- intent: intent-outcome-0001\n- 동작: 저널과 지연 표식을 보존했습니다.\n- 다음: project ledger, operation log, SQLite 순서로 복구하세요.",
        ),
        (
            TuiOutcomeCode::ProjectionLagInstallFailed,
            context("pending-approval"),
            TuiOutcomeStatus::Blocked,
            TuiEffect::RecoveryPending,
            TuiFreshness::ProjectionLag,
            TuiNextAction::RepairProjection,
            "지연 표식 설치 실패\n- code: projection.lag-install-failed\n- intent: intent-outcome-0001\n- 동작: 저널을 보존하고 정리를 중단했습니다.\n- 다음: 지연 표식을 다시 설치한 뒤 파생 출력을 복구하세요.",
        ),
        (
            TuiOutcomeCode::ProjectionRepairComplete,
            context("pending-approval"),
            TuiOutcomeStatus::Succeeded,
            TuiEffect::Refreshed,
            TuiFreshness::Fresh,
            TuiNextAction::RefreshProjection,
            "파생 출력 복구 완료\n- code: projection.repair-complete\n- intent: intent-outcome-0001\n- 동작: 지연 표식과 저널 정리를 완료했습니다.\n- 다음: 투영 상태를 새로고침하세요.",
        ),
        (
            TuiOutcomeCode::SourceInstallUnsupportedPlatform,
            context("pending-approval"),
            TuiOutcomeStatus::Blocked,
            TuiEffect::NotDispatched,
            TuiFreshness::Fresh,
            TuiNextAction::UseUnixOrChooseNonSourceAction,
            "source install 차단\n- code: source-install.unsupported-platform\n- platform: windows\n- 지원 범위: v0.34.0 source installation은 Unix만 지원합니다.\n- 동작: journal/temp/guard/rollback/target 변경 없음",
        ),
    ];

    assert_eq!(fixtures.len(), 27);
    for (code, context, status, effect, freshness, next_action, message) in fixtures {
        let outcome = exact_tui_outcome(code, context).unwrap();
        assert_eq!(outcome.status, status, "{} status", code.as_str());
        assert_eq!(outcome.code, code, "{} code", code.as_str());
        assert_eq!(outcome.effect, effect, "{} effect", code.as_str());
        assert_eq!(outcome.safe_message.as_bytes(), message.as_bytes());
        assert_eq!(outcome.freshness, freshness, "{} freshness", code.as_str());
        assert_eq!(outcome.next_action, next_action, "{} action", code.as_str());
        assert!(
            outcome.one_shot_secret.is_none(),
            "{} secret",
            code.as_str()
        );
    }
}

#[test]
fn source_install_unsupported_platform_result_is_exact() {
    let outcome =
        crate::surfaces::tui::outcome::unsupported_source_platform_outcome("windows").unwrap();

    assert_eq!(outcome.status, TuiOutcomeStatus::Blocked);
    assert_eq!(
        outcome.code,
        TuiOutcomeCode::SourceInstallUnsupportedPlatform
    );
    assert_eq!(outcome.effect, TuiEffect::NotDispatched);
    assert_eq!(
        outcome.safe_message.as_bytes(),
        b"source install \xec\xb0\xa8\xeb\x8b\xa8\n- code: source-install.unsupported-platform\n- platform: windows\n- \xec\xa7\x80\xec\x9b\x90 \xeb\xb2\x94\xec\x9c\x84: v0.34.0 source installation\xec\x9d\x80 Unix\xeb\xa7\x8c \xec\xa7\x80\xec\x9b\x90\xed\x95\xa9\xeb\x8b\x88\xeb\x8b\xa4.\n- \xeb\x8f\x99\xec\x9e\x91: journal/temp/guard/rollback/target \xeb\xb3\x80\xea\xb2\xbd \xec\x97\x86\xec\x9d\x8c"
    );
    assert_eq!(outcome.freshness, TuiFreshness::Fresh);
    assert_eq!(
        outcome.next_action,
        TuiNextAction::UseUnixOrChooseNonSourceAction
    );
    assert!(outcome.one_shot_secret.is_none());
}

#[test]
fn tui_outcome_public_dto_and_exact_fixtures_share_field_order() {
    let source = include_str!("../../../surfaces/tui/outcome.rs");
    let start = source.find("pub(crate) struct TuiOutcome {").unwrap();
    let end = source[start..].find("\n}").unwrap() + start;
    let definition = &source[start..end];
    let fields = [
        "pub(crate) status:",
        "pub(crate) code:",
        "pub(crate) effect:",
        "pub(crate) safe_message:",
        "pub(crate) freshness:",
        "pub(crate) next_action:",
        "pub(crate) one_shot_secret:",
    ];
    let positions = fields
        .iter()
        .map(|field| definition.find(field).unwrap())
        .collect::<Vec<_>>();

    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(definition.matches("    pub(crate) ").count(), fields.len());
}

#[test]
fn one_shot_secret_plaintext_accessor_consumes_value() {
    assert!(
        include_str!("../../../surfaces/tui/runtime_bridge/intent.rs")
            .contains("fn expose<R>(self, use_plaintext: impl FnOnce(&str) -> R) -> R")
    );
    let secret = OneShotSecret::new("secret-value".to_string()).unwrap();
    assert_eq!(secret.expose(str::to_string), "secret-value");
    assert!(OneShotSecret::new(String::new()).is_err());
}

#[test]
fn immediate_credential_outcome_is_separate_from_the_27_exact_rows() {
    let credential = "ab".repeat(32);
    let outcome = verification_credential_issued(
        "intent-credential-issued",
        OneShotSecret::new(credential.clone()).unwrap(),
    )
    .unwrap();

    assert_eq!(TuiOutcomeCode::ALL.len(), 27);
    assert!(!TuiOutcomeCode::ALL.contains(&TuiOutcomeCode::VerificationCredentialIssued));
    assert_eq!(outcome.code, TuiOutcomeCode::VerificationCredentialIssued);
    assert!(!outcome.safe_message.contains(&credential));
    assert_eq!(
        outcome.one_shot_secret.unwrap().expose(str::to_string),
        credential
    );
    assert!(exact_tui_outcome(
        TuiOutcomeCode::VerificationCredentialIssued,
        TuiOutcomeContext::default()
    )
    .is_err());
}
