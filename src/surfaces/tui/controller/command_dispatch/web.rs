use crate::foundation::error::AppError;
use crate::runtime_core::terminal::TerminalIo;

use super::super::request_submission::{submit_request_with_progress, submit_web_tool_command};
use super::super::TuiRuntimePort;
use super::LoopControl;
use crate::surfaces::tui::view_model::{ConversationRole, InteractiveState, InteractiveView};

pub(super) fn dispatch_web(
    terminal: &mut impl TerminalIo,
    runtime: &mut impl TuiRuntimePort,
    state: &mut InteractiveState,
    width: u16,
    height: u16,
    line: &str,
    words: &[&str],
) -> Result<Option<LoopControl>, AppError> {
    let control = match words {
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
        _ => return Ok(None),
    };
    Ok(Some(control))
}
