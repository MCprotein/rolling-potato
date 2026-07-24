use crate::foundation::error::AppError;
use crate::runtime_core::terminal::{
    FrameWriteBoundary, TerminalChoice, TerminalFault, TerminalIo,
};

use super::super::outcome::{
    exact_tui_outcome, TuiEffect, TuiOutcome, TuiOutcomeCode, TuiOutcomeContext,
};
use super::super::runtime_bridge::{TuiReadPage, TuiStatusSnapshot};
use super::super::view_model::InteractiveState;
use super::TuiRuntimePort;

pub(super) fn confirm(
    terminal: &mut impl TerminalIo,
    title: &str,
    action_label: &str,
    action_description: impl Into<String>,
) -> Result<bool, AppError> {
    let choices = [
        TerminalChoice {
            value: "cancel".to_string(),
            label: "취소".to_string(),
            description: "아무 변경도 하지 않고 대화로 돌아갑니다.".to_string(),
            current: false,
            recommended: false,
        },
        TerminalChoice {
            value: "confirm".to_string(),
            label: action_label.to_string(),
            description: action_description.into(),
            current: false,
            recommended: true,
        },
    ];
    Ok(terminal
        .choose(title, &choices)
        .map_err(terminal_fault_error)?
        .as_deref()
        == Some("confirm"))
}

pub(super) fn write_pre_dispatch_frame(
    terminal: &mut impl TerminalIo,
    intent_id: &str,
    prompt: &str,
) -> Result<(), AppError> {
    terminal
        .write_frame_at(prompt, FrameWriteBoundary::PreDispatch)
        .map_err(|_| pre_dispatch_write_error(intent_id))
}

pub(super) fn write_pending_conversation_frame(
    terminal: &mut impl TerminalIo,
    runtime: &mut impl TuiRuntimePort,
    state: &InteractiveState,
    width: u16,
    height: u16,
) -> Result<(), AppError> {
    let status = runtime
        .read_tui_status()
        .unwrap_or_else(|_| TuiStatusSnapshot::unavailable());
    let frame = super::super::render::render_interactive_frame_with_options(
        state,
        &TuiReadPage::conversation_placeholder(),
        &status,
        width,
        height,
        terminal.supports_ansi_layout(),
        terminal.supports_color(),
    );
    terminal
        .write_frame_at(&frame, FrameWriteBoundary::Ordinary)
        .map_err(|_| pre_dispatch_write_error(&runtime.new_tui_intent_id()))
}

pub(crate) fn terminal_fault_error(fault: TerminalFault) -> AppError {
    if fault == TerminalFault::EchoRestore {
        return AppError::blocked(
            "터미널 echo 복원 실패\n- code: terminal.echo-restore.failed\n- 동작: 비밀 입력을 재시도하지 않고 TUI를 종료합니다.\n- 다음: 터미널에서 `stty echo`로 입력 echo를 복구한 뒤 새 세션을 시작하세요.",
        );
    }
    let code = match fault {
        #[cfg(debug_assertions)]
        TerminalFault::InvalidFaultConfiguration => {
            return AppError::blocked(
                "터미널 장애 주입 구성 오류\n- kind: InvalidFaultConfiguration\n- effect: NotDispatched\n- retry: FixConfiguration\n- 동작: 터미널 상태를 변경하거나 런타임 요청을 보내지 않았습니다.\n- 다음: RPOTATO_TEST_TERMINAL_FAULT 값을 닫힌 지원 목록으로 고치세요.",
            )
        }
        TerminalFault::SizeRead => TuiOutcomeCode::TerminalCapabilitySizeRead,
        TerminalFault::ModeRead | TerminalFault::LineRead => {
            TuiOutcomeCode::TerminalCapabilityModeRead
        }
        TerminalFault::NoEchoSet => TuiOutcomeCode::TerminalNoEchoSetFailed,
        TerminalFault::SecretRead => TuiOutcomeCode::TerminalSecretReadFailed,
        TerminalFault::EchoRestore => unreachable!("handled above"),
        TerminalFault::FrameWrite => TuiOutcomeCode::TerminalFrameWritePreDispatch,
    };
    let context = if code == TuiOutcomeCode::TerminalFrameWritePreDispatch {
        TuiOutcomeContext {
            intent_id: Some("intent-terminal-frame-write"),
            ..TuiOutcomeContext::default()
        }
    } else {
        TuiOutcomeContext::default()
    };
    exact_outcome_error(code, context)
}

pub(super) fn pre_dispatch_write_error(intent_id: &str) -> AppError {
    exact_outcome_error(
        TuiOutcomeCode::TerminalFrameWritePreDispatch,
        TuiOutcomeContext {
            intent_id: Some(intent_id),
            ..TuiOutcomeContext::default()
        },
    )
}

pub(super) fn post_dispatch_write_error(intent_id: &str) -> AppError {
    exact_outcome_error(
        TuiOutcomeCode::TerminalFrameWritePostDispatch,
        TuiOutcomeContext {
            intent_id: Some(intent_id),
            ..TuiOutcomeContext::default()
        },
    )
}

fn exact_outcome_error(code: TuiOutcomeCode, context: TuiOutcomeContext<'_>) -> AppError {
    match exact_tui_outcome(code, context) {
        Ok(outcome) => AppError::blocked(outcome_notice(outcome)),
        Err(error) => error,
    }
}

pub(super) fn outcome_notice(outcome: TuiOutcome) -> String {
    let TuiOutcome {
        status,
        code,
        effect,
        safe_message,
        freshness,
        next_action,
        one_shot_secret,
    } = outcome;
    debug_assert!(one_shot_secret.is_none());
    let _typed_contract = (status, code, effect, freshness, next_action);
    safe_message
}

pub(crate) fn consume_outcome(
    terminal: &mut impl TerminalIo,
    intent_id: &str,
    outcome: TuiOutcome,
) -> Result<ConsumedOutcome, AppError> {
    let TuiOutcome {
        status,
        code,
        effect,
        safe_message,
        freshness,
        next_action,
        one_shot_secret,
    } = outcome;
    let was_dispatched = outcome_was_dispatched(effect);
    let _typed_contract = (status, code, freshness, next_action);
    if let Some(secret) = one_shot_secret {
        terminal
            .write_frame_at(
                "verification credential (one-time): ",
                FrameWriteBoundary::PostDispatch,
            )
            .map_err(|_| post_dispatch_write_error(intent_id))?;
        secret
            .expose(|plaintext| {
                terminal.write_frame_at(plaintext, FrameWriteBoundary::PostDispatch)
            })
            .map_err(|_| post_dispatch_write_error(intent_id))?;
        terminal
            .write_frame_at("\n", FrameWriteBoundary::PostDispatch)
            .map_err(|_| post_dispatch_write_error(intent_id))?;
    }
    Ok(ConsumedOutcome {
        notice: safe_message,
        was_dispatched,
    })
}

pub(crate) struct ConsumedOutcome {
    pub(crate) notice: String,
    pub(crate) was_dispatched: bool,
}

pub(super) fn outcome_was_dispatched(effect: TuiEffect) -> bool {
    !matches!(effect, TuiEffect::NotDispatched)
}
