//! Synchronous request submission with live terminal progress presentation.

use crate::foundation::error::AppError;
use crate::runtime_core::terminal::{FrameWriteBoundary, TerminalIo};

use super::super::runtime_bridge::{TuiAttachment, TuiRequestProgress, TuiRequestProgressReporter};
use super::super::view_model::{ConversationRole, InteractiveState, InteractiveView};
use super::terminal_flow::write_pending_conversation_frame_with_status;
use super::{read_status_or_notice, TuiRuntimePort};

#[allow(clippy::too_many_arguments)]
pub(super) fn submit_web_tool_command(
    terminal: &mut impl TerminalIo,
    runtime: &mut impl TuiRuntimePort,
    state: &mut InteractiveState,
    width: u16,
    height: u16,
    request: &str,
    pending: &str,
    error_heading: &str,
) -> Result<(), AppError> {
    state.view = InteractiveView::Conversation;
    state.push_turn(ConversationRole::User, request);
    match submit_request_with_progress(
        terminal,
        runtime,
        state,
        width,
        height,
        request,
        &[],
        pending,
    )? {
        Ok(report) => state.push_turn(ConversationRole::Assistant, report),
        Err(error) => state.push_turn(
            ConversationRole::Error,
            format!("{error_heading}\n{}", error.message),
        ),
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn submit_request_with_progress(
    terminal: &mut impl TerminalIo,
    runtime: &mut impl TuiRuntimePort,
    state: &mut InteractiveState,
    width: u16,
    height: u16,
    request: &str,
    attachments: &[TuiAttachment],
    progress: &str,
) -> Result<Result<String, AppError>, AppError> {
    const REFRESH: std::time::Duration = std::time::Duration::from_millis(90);
    const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

    state.context_tokens_estimate = runtime.request_context_tokens_hint(request, attachments);
    let status = read_status_or_notice(runtime, state);
    let intent_id = runtime.new_tui_intent_id();
    let started = std::time::Instant::now();
    state.notice = activity_notice(SPINNER[0], started.elapsed(), progress);
    write_pending_conversation_frame_with_status(
        terminal,
        state,
        &status,
        width,
        height,
        &intent_id,
        FrameWriteBoundary::PreDispatch,
    )?;

    let mut frame_error = None;
    let (progress_reporter, progress_receiver) = TuiRequestProgressReporter::channel(32);
    let result = std::thread::scope(|scope| {
        let handle = scope.spawn(|| {
            runtime.submit_request_with_progress(request, attachments, &progress_reporter)
        });
        let mut tick = 1;
        let mut runtime_progress = Vec::new();
        loop {
            std::thread::sleep(REFRESH);
            let progress_changed = drain_progress(&progress_receiver, &mut runtime_progress);
            if frame_error.is_none() && (!handle.is_finished() || progress_changed) {
                let current = progress_notice(progress, &runtime_progress);
                state.notice =
                    activity_notice(SPINNER[tick % SPINNER.len()], started.elapsed(), &current);
                tick += 1;
                if let Err(error) = write_pending_conversation_frame_with_status(
                    terminal,
                    state,
                    &status,
                    width,
                    height,
                    &intent_id,
                    FrameWriteBoundary::PostDispatch,
                ) {
                    frame_error = Some(error);
                }
            }
            if handle.is_finished() {
                break;
            }
        }
        handle.join()
    });
    state.context_tokens_estimate = None;
    if let Some(error) = frame_error {
        return Err(error);
    }
    result.map_err(|_| AppError::runtime("TUI 요청 실행 thread가 예기치 않게 종료되었습니다."))
}

fn drain_progress(
    receiver: &std::sync::mpsc::Receiver<TuiRequestProgress>,
    observed: &mut Vec<TuiRequestProgress>,
) -> bool {
    let mut changed = false;
    while let Ok(progress) = receiver.try_recv() {
        if !observed.contains(&progress) {
            observed.push(progress);
            changed = true;
        }
    }
    changed
}

fn progress_notice(initial: &str, observed: &[TuiRequestProgress]) -> String {
    if observed.is_empty() {
        return initial.to_string();
    }
    format!(
        "런타임 단계 · {}",
        observed
            .iter()
            .map(|progress| progress.label())
            .collect::<Vec<_>>()
            .join(" → ")
    )
}

pub(super) fn test_secret_probe_enabled() -> bool {
    cfg!(debug_assertions)
        && std::env::var_os("RPOTATO_TEST_TUI_SECRET_PROBE").as_deref()
            == Some(std::ffi::OsStr::new("1"))
}

fn activity_notice(spinner: char, elapsed: std::time::Duration, progress: &str) -> String {
    format!(
        "{spinner} 처리 중 · 경과 {:.1}초\n{progress}",
        elapsed.as_secs_f32()
    )
}
