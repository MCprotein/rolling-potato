use crate::foundation::error::AppError;
use crate::runtime_core::terminal::{TerminalFault, TerminalIo};

use super::super::model_selection::{apply_model_choice, choose_model, model_options_notice};
use super::super::session_selection::{resume_session, start_new_session};
use super::super::source_selection::select_source;
use super::super::terminal_flow::{confirm, terminal_fault_error};
use super::super::TuiRuntimePort;
use super::LoopControl;
use crate::surfaces::tui::view_model::{InteractiveState, InteractiveView};

pub(super) fn dispatch_workspace(
    terminal: &mut impl TerminalIo,
    runtime: &mut impl TuiRuntimePort,
    state: &mut InteractiveState,
    width: u16,
    height: u16,
    words: &[&str],
) -> Result<Option<LoopControl>, AppError> {
    let control = match words {
        ["/sources"] => {
            select_source(terminal, runtime, state, width, height)?;
            LoopControl::Continue
        }
        ["/attach"] => {
            state.notice = "사용법: /attach <로컬 파일 경로>".to_string();
            LoopControl::Continue
        }
        ["/attach", path @ ..] => {
            state.notice = super::super::attachments::capture_attachment_notice(
                runtime,
                state,
                &path.join(" "),
            );
            LoopControl::Continue
        }
        ["/update"] => dispatch_update(terminal, runtime, state)?,
        ["/status"] => {
            state.notice = "모델·컨텍스트·backend·세션 상태를 새로고침했습니다.".to_string();
            LoopControl::Continue
        }
        ["/chat"] => {
            state.set_view(InteractiveView::Conversation);
            LoopControl::Continue
        }
        ["/sessions"] => {
            state.set_view(InteractiveView::Sessions);
            LoopControl::Continue
        }
        ["/new"] => {
            start_new_session(runtime, state);
            LoopControl::Continue
        }
        ["/resume"] => {
            resume_session(terminal, runtime, state)?;
            LoopControl::Continue
        }
        ["/doctor"] => {
            state.notice = runtime.doctor_report();
            LoopControl::Continue
        }
        ["/clear"] => {
            match runtime.clear_conversation_history() {
                Ok(()) => state.clear_conversation(),
                Err(error) => state.notice = error.message,
            }
            LoopControl::Continue
        }
        ["/model"] => dispatch_model_picker(terminal, runtime, state)?,
        ["/model", id] => dispatch_model_id(terminal, runtime, state, id)?,
        _ => return Ok(None),
    };
    Ok(Some(control))
}

fn dispatch_update(
    terminal: &mut impl TerminalIo,
    runtime: &mut impl TuiRuntimePort,
    state: &mut InteractiveState,
) -> Result<LoopControl, AppError> {
    if !confirm(
        terminal,
        "업데이트 확인",
        "업데이트 시작",
        "최신 stable release 확인 → archive 다운로드 → SHA-256 검증 → binary 교체",
    )? {
        state.notice = "업데이트를 취소했습니다.".to_string();
        return Ok(LoopControl::Continue);
    }
    terminal
        .write_frame("release 확인 → archive 다운로드 → SHA-256 검증 → 설치 중...\n")
        .map_err(|_| terminal_fault_error(TerminalFault::FrameWrite))?;
    state.notice = match runtime.apply_update() {
        Ok(report) => report,
        Err(error) => error.message,
    };
    Ok(LoopControl::Continue)
}

fn dispatch_model_picker(
    terminal: &mut impl TerminalIo,
    runtime: &mut impl TuiRuntimePort,
    state: &mut InteractiveState,
) -> Result<LoopControl, AppError> {
    let options = runtime.model_options();
    if options.is_empty() {
        state.notice = "사용 가능한 모델이 없습니다.".to_string();
        return Ok(LoopControl::Continue);
    }
    let Some(id) = choose_model(terminal, &options)? else {
        state.notice = "모델 선택을 취소했습니다.".to_string();
        return Ok(LoopControl::Continue);
    };
    let selected = options
        .iter()
        .find(|option| option.id == id)
        .expect("terminal choice must originate from model options");
    state.notice = apply_model_choice(terminal, runtime, selected)?;
    Ok(LoopControl::Continue)
}

fn dispatch_model_id(
    terminal: &mut impl TerminalIo,
    runtime: &mut impl TuiRuntimePort,
    state: &mut InteractiveState,
    id: &str,
) -> Result<LoopControl, AppError> {
    let options = runtime.model_options();
    let Some(selected) = options.iter().find(|option| option.id == id) else {
        state.notice = format!(
            "알 수 없는 model id입니다: {id}\n{}",
            model_options_notice(&options)
        );
        return Ok(LoopControl::Continue);
    };
    state.notice = apply_model_choice(terminal, runtime, selected)?;
    Ok(LoopControl::Continue)
}
