use crate::foundation::error::AppError;
use crate::runtime_core::terminal::TerminalIo;

use super::super::outcome::{exact_tui_outcome, TuiOutcomeCode, TuiOutcomeContext};
use super::super::runtime_bridge::{OneShotSecret, TuiReadPage};
use super::super::view_model::{ConversationRole, InteractiveState, InteractiveView};
use super::attachments::{
    attachment_path_candidate, capture_attachment_notice, looks_like_attachment_path,
};
use super::request_submission::{
    submit_request_with_progress, submit_web_tool_command, test_secret_probe_enabled,
};
use super::session_selection::resume_selected_session;
use super::terminal_flow::{outcome_notice, terminal_fault_error, write_pre_dispatch_frame};
use super::TuiRuntimePort;

mod workflow;
mod workspace;

pub(super) enum LoopControl {
    Continue,
    Exit,
    PostDispatch(String),
}

pub(super) fn dispatch_line(
    terminal: &mut impl TerminalIo,
    runtime: &mut impl TuiRuntimePort,
    state: &mut InteractiveState,
    page: &TuiReadPage,
    width: u16,
    height: u16,
    line: &str,
) -> Result<LoopControl, AppError> {
    let words = line.split_whitespace().collect::<Vec<_>>();
    if !matches!(words.as_slice(), ["/more"] | ["/back"]) {
        state.reset_notice_page();
    }
    if let Some(control) =
        workspace::dispatch_workspace(terminal, runtime, state, width, height, &words)?
    {
        return Ok(control);
    }
    if let Some(control) = workflow::dispatch_workflow(terminal, runtime, state, &words)? {
        return Ok(control);
    }
    let control = match words.as_slice() {
        [] | ["refresh"] => {
            state.notice = "정본 상태를 새로고침했습니다.".to_string();
            LoopControl::Continue
        }
        ["quit"] | ["exit"] | ["/quit"] => LoopControl::Exit,
        ["help"] | ["/help"] => {
            state.notice = super::super::command_palette::help_notice();
            LoopControl::Continue
        }
        ["/more"] => {
            let conversation_pages =
                super::super::render::conversation_page_count(state, width, height);
            state.next_notice_page(height, conversation_pages);
            LoopControl::Continue
        }
        ["/back"] => {
            state.previous_notice_page();
            LoopControl::Continue
        }
        ["/compact"] => {
            state.notice = match runtime.compact_context() {
                Ok(report) => report,
                Err(error) => error.message,
            };
            LoopControl::Continue
        }
        ["/search"] => {
            state.notice = "사용법: /search <인터넷에서 찾을 질문>".to_string();
            LoopControl::Continue
        }
        ["/search", ..] => {
            state.view = InteractiveView::Conversation;
            state.push_turn(ConversationRole::User, line.trim());
            let progress =
                "웹 조사 · 검색 중\n검색 ● → 결과 평가 ○ → 문서 읽기 ○ → 증거 구성 ○ → 답변 ○";
            match submit_request_with_progress(
                terminal,
                runtime,
                state,
                width,
                height,
                line.trim(),
                &[],
                progress,
            )? {
                Ok(report) => state.push_turn(ConversationRole::Assistant, report),
                Err(error) => state.push_turn(
                    ConversationRole::Error,
                    format!("웹 검색을 완료하지 못했습니다.\n{}", error.message),
                ),
            }
            LoopControl::Continue
        }
        ["/open"] => {
            state.notice = "사용법: /open <HTTPS URL>".to_string();
            LoopControl::Continue
        }
        ["/open", url @ ..] => {
            let request = format!("/open {}", url.join(" "));
            submit_web_tool_command(
                terminal,
                runtime,
                state,
                width,
                height,
                &request,
                "웹 조사 · 페이지 여는 중\n문서 읽기 ● → 증거 구성 ○ → 답변 ○",
                "웹 페이지를 열지 못했습니다.",
            )?;
            LoopControl::Continue
        }
        ["/find"] => {
            state.notice = "사용법: /find <열린 페이지에서 찾을 텍스트>".to_string();
            LoopControl::Continue
        }
        ["/find", query @ ..] => {
            let request = format!("/find {}", query.join(" "));
            submit_web_tool_command(
                terminal,
                runtime,
                state,
                width,
                height,
                &request,
                "웹 조사 · 페이지 찾는 중\n문서 읽기 ✓ → 본문 찾기 ● → 답변 ○",
                "페이지 내부 찾기를 완료하지 못했습니다.",
            )?;
            LoopControl::Continue
        }
        ["test-secret"] if test_secret_probe_enabled() => {
            let intent_id = runtime.new_tui_intent_id();
            write_pre_dispatch_frame(
                terminal,
                &intent_id,
                "비밀 probe를 무반향으로 입력하세요.\n",
            )?;
            let Some(secret) = terminal.read_secret().map_err(terminal_fault_error)? else {
                state.notice = "비밀 입력 EOF: probe를 완료하지 않았습니다.".to_string();
                return Ok(LoopControl::Continue);
            };
            drop(OneShotSecret::new(secret)?);
            let outcome = exact_tui_outcome(
                TuiOutcomeCode::SecretRefreshOnly,
                TuiOutcomeContext {
                    intent_id: Some(&intent_id),
                    ..TuiOutcomeContext::default()
                },
            )?;
            state.notice = outcome_notice(outcome);
            LoopControl::PostDispatch(intent_id)
        }
        ["next"] if page.has_next => {
            state.page = state.page.saturating_add(1);
            state.notice = format!("{} 페이지", state.page + 1);
            LoopControl::Continue
        }
        ["prev"] if page.has_previous => {
            state.page = state.page.saturating_sub(1);
            state.notice = format!("{} 페이지", state.page + 1);
            LoopControl::Continue
        }
        ["next"] | ["prev"] => {
            state.notice = "이동할 페이지가 없습니다.".to_string();
            LoopControl::Continue
        }
        ["view", "overview"] => {
            state.set_view(InteractiveView::Overview);
            LoopControl::Continue
        }
        ["view", "chat"] => {
            state.set_view(InteractiveView::Conversation);
            LoopControl::Continue
        }
        ["view", "monitor"] => {
            state.set_view(InteractiveView::Monitor);
            LoopControl::Continue
        }
        ["view", "sessions"] => {
            state.set_view(InteractiveView::Sessions);
            LoopControl::Continue
        }
        ["view", "approvals"] => {
            state.set_view(InteractiveView::Approvals);
            LoopControl::Continue
        }
        ["view", "evidence"] => {
            state.set_view(InteractiveView::Evidence);
            LoopControl::Continue
        }
        ["view", "transcript", session_id] => {
            state.set_view(InteractiveView::Transcript((*session_id).to_string()));
            LoopControl::Continue
        }
        ["view", "tool-output", artifact_id] => {
            state.set_view(InteractiveView::ToolOutput((*artifact_id).to_string()));
            LoopControl::Continue
        }
        ["view", "diff", proposal_id] => {
            state.set_view(InteractiveView::Diff((*proposal_id).to_string()));
            LoopControl::Continue
        }
        ["select", "session", session_id] => {
            resume_selected_session(runtime, state, session_id);
            LoopControl::Continue
        }
        [command, ..] if command.starts_with('/') && looks_like_attachment_path(line.trim()) => {
            let path = attachment_path_candidate(line.trim())
                .expect("attachment guard and normalization share one classifier");
            state.notice = capture_attachment_notice(runtime, state, &path);
            LoopControl::Continue
        }
        [command, ..] if command.starts_with('/') => {
            state.notice = format!("알 수 없는 TUI 명령입니다: {command}\n/help로 확인하세요.");
            LoopControl::Continue
        }
        _ => {
            if looks_like_attachment_path(line.trim()) {
                let path = attachment_path_candidate(line.trim())
                    .expect("attachment guard and normalization share one classifier");
                state.notice = capture_attachment_notice(runtime, state, &path);
                return Ok(LoopControl::Continue);
            }
            state.view = InteractiveView::Conversation;
            state.push_turn(ConversationRole::User, line.trim());
            let progress = runtime
                .request_progress_hint(line.trim())
                .unwrap_or_else(|| "에이전트가 요청을 처리하고 있습니다…".to_string());
            let attachments = state.attachments.clone();
            match submit_request_with_progress(
                terminal,
                runtime,
                state,
                width,
                height,
                line.trim(),
                &attachments,
                &progress,
            )? {
                Ok(report) => {
                    state.clear_attachments();
                    state.push_turn(ConversationRole::Assistant, report);
                }
                Err(error) => state.push_turn(
                    ConversationRole::Error,
                    format!(
                        "요청을 완료하지 못했습니다.\n{}\n첨부는 재시도를 위해 유지했습니다.",
                        error.message
                    ),
                ),
            }
            LoopControl::Continue
        }
    };
    Ok(control)
}
