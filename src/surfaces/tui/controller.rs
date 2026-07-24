use crate::foundation::error::AppError;
use crate::runtime_core::terminal::{FrameWriteBoundary, TerminalFault, TerminalIo};

use super::outcome::{exact_tui_outcome, TuiOutcome, TuiOutcomeCode, TuiOutcomeContext};
use super::runtime_bridge::{
    OneShotSecret, SelectionLease, TuiAttachment, TuiConversationTurn, TuiGateKind, TuiIntent,
    TuiModelOption, TuiReadPage, TuiReadRequest, TuiStatusSnapshot,
};
use super::view_model::{ConversationRole, InteractiveState, InteractiveView};

mod attachments;
mod model_selection;
mod terminal_flow;

use attachments::{capture_attachment_notice, looks_like_attachment_path};
use model_selection::{apply_model_choice, choose_model, model_options_notice};
use terminal_flow::{
    confirm, confirm_workflow_action, outcome_notice, outcome_was_dispatched,
    post_dispatch_write_error, pre_dispatch_write_error, write_pending_conversation_frame,
    write_pre_dispatch_frame,
};
pub(crate) use terminal_flow::{consume_outcome, terminal_fault_error};

pub(crate) trait TuiRuntimePort {
    fn startup_update_notice(&mut self) -> Option<String>;
    fn conversation_history(&mut self) -> Result<Vec<TuiConversationTurn>, AppError>;
    fn clear_conversation_history(&mut self) -> Result<(), AppError>;
    fn apply_update(&mut self) -> Result<String, AppError>;
    fn read_tui_page(&mut self, request: TuiReadRequest) -> Result<TuiReadPage, AppError>;
    fn read_tui_status(&mut self) -> Result<TuiStatusSnapshot, AppError>;
    fn model_options(&mut self) -> Vec<TuiModelOption>;
    fn setup_model(&mut self, id: &str) -> Result<String, AppError>;
    fn doctor_report(&mut self) -> String;
    fn compact_context(&mut self) -> Result<String, AppError>;
    fn capture_attachment(&mut self, path: &str) -> Result<TuiAttachment, AppError>;
    fn submit_request(
        &mut self,
        request: &str,
        attachments: &[TuiAttachment],
    ) -> Result<String, AppError>;
    fn new_tui_intent_id(&mut self) -> String;
    fn tui_selection_lease(&mut self, selected_object_id: &str)
        -> Result<SelectionLease, AppError>;
    fn tui_gate_descriptor(&mut self, workflow_id: &str)
        -> Result<(String, TuiGateKind), AppError>;
    fn dispatch_tui_intent(&mut self, intent: TuiIntent) -> Result<TuiOutcome, AppError>;
}

pub(crate) fn run_controller(
    terminal: &mut impl TerminalIo,
    runtime: &mut impl TuiRuntimePort,
) -> Result<(), AppError> {
    terminal
        .validate_configuration()
        .map_err(terminal_fault_error)?;
    let mut state = InteractiveState::new();
    state.turns = runtime.conversation_history()?;
    let mut startup_update_pending = true;
    let mut post_dispatch_intent: Option<String> = None;

    loop {
        let (width, height) = terminal.dimensions().map_err(terminal_fault_error)?;
        let page = if matches!(state.view, InteractiveView::Conversation) {
            TuiReadPage::conversation_placeholder()
        } else {
            let request = state.read_request(width, height);
            runtime.read_tui_page(request)?
        };
        let status = runtime
            .read_tui_status()
            .unwrap_or_else(|_| TuiStatusSnapshot::unavailable());
        let frame = super::render::render_interactive_frame_with_options(
            &state,
            &page,
            &status,
            width,
            height,
            terminal.supports_ansi_layout(),
            terminal.supports_color(),
        );
        let boundary = if post_dispatch_intent.is_some() {
            FrameWriteBoundary::PostDispatch
        } else {
            FrameWriteBoundary::Ordinary
        };
        if terminal.write_frame_at(&frame, boundary).is_err() {
            return Err(match post_dispatch_intent.take() {
                Some(intent_id) => post_dispatch_write_error(&intent_id),
                None => pre_dispatch_write_error(&runtime.new_tui_intent_id()),
            });
        }
        post_dispatch_intent = None;

        if startup_update_pending {
            startup_update_pending = false;
            if let Some(notice) = runtime.startup_update_notice() {
                state.notice = notice;
                continue;
            }
        }

        let Some(line) = terminal
            .read_line_with_suggestions(super::command_palette::commands())
            .map_err(terminal_fault_error)?
        else {
            return Ok(());
        };
        let words = line.split_whitespace().collect::<Vec<_>>();
        if !matches!(words.as_slice(), ["/more"] | ["/back"]) {
            state.reset_notice_page();
        }
        match words.as_slice() {
            [] | ["refresh"] => {
                state.notice = "정본 상태를 새로고침했습니다.".to_string();
            }
            ["quit"] | ["exit"] | ["/quit"] => return Ok(()),
            ["help"] | ["/help"] => {
                state.notice = super::command_palette::help_notice();
            }
            ["/more"] => {
                let conversation_pages =
                    super::render::conversation_page_count(&state, width, height);
                state.next_notice_page(height, conversation_pages);
            }
            ["/back"] => state.previous_notice_page(),
            ["/compact"] => {
                state.notice = match runtime.compact_context() {
                    Ok(report) => report,
                    Err(error) => error.message,
                };
            }
            ["/search"] => {
                state.notice = "사용법: /search <인터넷에서 찾을 질문>".to_string();
            }
            ["/search", ..] => {
                state.view = InteractiveView::Conversation;
                state.push_turn(ConversationRole::User, line.trim());
                state.notice = "검색 중 · 최신 웹 자료를 확인하고 있습니다…".to_string();
                write_pending_conversation_frame(terminal, runtime, &state, width, height)?;
                let response = match runtime.submit_request(line.trim(), &[]) {
                    Ok(report) => report,
                    Err(error) => format!("웹 검색을 완료하지 못했습니다.\n{}", error.message),
                };
                state.push_turn(ConversationRole::Assistant, response);
            }
            ["/open"] => {
                state.notice = "사용법: /open <HTTPS URL>".to_string();
            }
            ["/open", url @ ..] => {
                let request = format!("/open {}", url.join(" "));
                submit_web_tool_command(
                    terminal,
                    runtime,
                    &mut state,
                    width,
                    height,
                    &request,
                    "페이지 여는 중 · 안전한 읽기 전용 연결을 확인하고 있습니다…",
                    "웹 페이지를 열지 못했습니다.",
                )?;
            }
            ["/find"] => {
                state.notice = "사용법: /find <열린 페이지에서 찾을 텍스트>".to_string();
            }
            ["/find", query @ ..] => {
                let request = format!("/find {}", query.join(" "));
                submit_web_tool_command(
                    terminal,
                    runtime,
                    &mut state,
                    width,
                    height,
                    &request,
                    "페이지 찾는 중 · 열린 문서의 텍스트를 확인하고 있습니다…",
                    "페이지 내부 찾기를 완료하지 못했습니다.",
                )?;
            }
            ["/attach"] => {
                state.notice = "사용법: /attach <로컬 파일 경로>".to_string();
            }
            ["/attach", path @ ..] => {
                let path = path.join(" ");
                state.notice = capture_attachment_notice(runtime, &mut state, &path);
            }
            ["/update"] => {
                if !confirm(
                    terminal,
                    "업데이트 확인",
                    "업데이트 시작",
                    "최신 stable release 확인 → archive 다운로드 → SHA-256 검증 → binary 교체",
                )? {
                    state.notice = "업데이트를 취소했습니다.".to_string();
                    continue;
                }
                terminal
                    .write_frame("release 확인 → archive 다운로드 → SHA-256 검증 → 설치 중...\n")
                    .map_err(|_| terminal_fault_error(TerminalFault::FrameWrite))?;
                state.notice = match runtime.apply_update() {
                    Ok(report) => report,
                    Err(error) => error.message,
                };
            }
            ["/status"] => {
                state.notice = "모델·컨텍스트·backend·세션 상태를 새로고침했습니다.".to_string();
            }
            ["/chat"] => state.set_view(InteractiveView::Conversation),
            ["/sessions"] => state.set_view(InteractiveView::Sessions),
            ["/doctor"] => {
                state.notice = runtime.doctor_report();
            }
            ["/clear"] => match runtime.clear_conversation_history() {
                Ok(()) => state.clear_conversation(),
                Err(error) => state.notice = error.message,
            },
            ["/model"] => {
                let options = runtime.model_options();
                if options.is_empty() {
                    state.notice = "사용 가능한 모델이 없습니다.".to_string();
                    continue;
                }
                let Some(id) = choose_model(terminal, &options)? else {
                    state.notice = "모델 선택을 취소했습니다.".to_string();
                    continue;
                };
                let selected = options
                    .iter()
                    .find(|option| option.id == id)
                    .expect("terminal choice must originate from model options");
                state.notice = apply_model_choice(terminal, runtime, selected)?;
            }
            ["/model", id] => {
                let options = runtime.model_options();
                let Some(selected) = options.iter().find(|option| option.id == *id) else {
                    state.notice = format!(
                        "알 수 없는 model id입니다: {id}\n{}",
                        model_options_notice(&options)
                    );
                    continue;
                };
                state.notice = apply_model_choice(terminal, runtime, selected)?;
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
                    continue;
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
                post_dispatch_intent = Some(intent_id);
            }
            ["next"] if page.has_next => {
                state.page = state.page.saturating_add(1);
                state.notice = format!("{} 페이지", state.page + 1);
            }
            ["prev"] if page.has_previous => {
                state.page = state.page.saturating_sub(1);
                state.notice = format!("{} 페이지", state.page + 1);
            }
            ["next"] | ["prev"] => {
                state.notice = "이동할 페이지가 없습니다.".to_string();
            }
            ["view", "overview"] => state.set_view(InteractiveView::Overview),
            ["view", "chat"] => state.set_view(InteractiveView::Conversation),
            ["view", "monitor"] => state.set_view(InteractiveView::Monitor),
            ["view", "sessions"] => state.set_view(InteractiveView::Sessions),
            ["view", "approvals"] => state.set_view(InteractiveView::Approvals),
            ["view", "evidence"] => state.set_view(InteractiveView::Evidence),
            ["view", "transcript", session_id] => {
                state.set_view(InteractiveView::Transcript((*session_id).to_string()))
            }
            ["view", "tool-output", artifact_id] => {
                state.set_view(InteractiveView::ToolOutput((*artifact_id).to_string()))
            }
            ["view", "diff", proposal_id] => {
                state.set_view(InteractiveView::Diff((*proposal_id).to_string()))
            }
            ["select", "session", session_id] => {
                if !confirm(
                    terminal,
                    "세션 선택 확인",
                    "이 세션으로 전환",
                    format!("session {session_id}을 정본 상태로 다시 확인한 뒤 선택"),
                )? {
                    state.notice = "세션 선택 요청을 보내지 않았습니다.".to_string();
                    continue;
                }
                let intent_id = runtime.new_tui_intent_id();
                let lease = runtime.tui_selection_lease(session_id)?;
                write_pre_dispatch_frame(
                    terminal,
                    &intent_id,
                    "정본 세션 상태를 재검증했습니다.\n",
                )?;
                let outcome = runtime.dispatch_tui_intent(TuiIntent::SelectSession {
                    intent_id: intent_id.clone(),
                    session_id: (*session_id).to_string(),
                    lease,
                })?;
                let was_dispatched = outcome_was_dispatched(outcome.effect);
                state.notice = outcome_notice(outcome);
                post_dispatch_intent = was_dispatched.then_some(intent_id);
            }
            ["select", selected_id] => {
                state.selected_id = Some((*selected_id).to_string());
                state.notice = format!("선택: {selected_id}");
            }
            ["approve", proposal_id] => {
                let Some(workflow_id) = state.selected_id.clone() else {
                    state.notice = "먼저 select <workflow-id>를 실행하세요.".to_string();
                    continue;
                };
                if !cfg!(unix) {
                    state.notice = outcome_notice(exact_tui_outcome(
                        TuiOutcomeCode::SourceInstallUnsupportedPlatform,
                        TuiOutcomeContext {
                            platform: Some(std::env::consts::OS),
                            ..TuiOutcomeContext::default()
                        },
                    )?);
                    continue;
                }
                if !confirm(
                    terminal,
                    "패치 적용 확인",
                    "패치 적용 승인",
                    format!("proposal {proposal_id}을 검증한 뒤 선택한 workflow에 적용"),
                )? {
                    state.notice = "승인을 보내지 않았습니다.".to_string();
                    continue;
                }
                let intent_id = runtime.new_tui_intent_id();
                let lease = runtime.tui_selection_lease(&workflow_id)?;
                write_pre_dispatch_frame(terminal, &intent_id, "토큰을 무반향으로 입력하세요.\n")?;
                let Some(secret) = terminal.read_secret().map_err(terminal_fault_error)? else {
                    state.notice = "비밀 입력 EOF: 승인을 보내지 않았습니다.".to_string();
                    continue;
                };
                let outcome = runtime.dispatch_tui_intent(TuiIntent::ApprovePatch {
                    intent_id: intent_id.clone(),
                    proposal_id: (*proposal_id).to_string(),
                    lease,
                    secret: OneShotSecret::new(secret)?,
                })?;
                let consumed = consume_outcome(terminal, &intent_id, outcome)?;
                state.notice = consumed.notice;
                post_dispatch_intent = consumed.was_dispatched.then_some(intent_id);
            }
            ["approve", "verification", proposal_id] => {
                let Some(workflow_id) = state.selected_id.clone() else {
                    state.notice = "먼저 select <workflow-id>를 실행하세요.".to_string();
                    continue;
                };
                if !confirm(
                    terminal,
                    "검증 실행 확인",
                    "검증 실행 승인",
                    format!("proposal {proposal_id}의 검증 단계를 실행"),
                )? {
                    state.notice = "검증 승인을 보내지 않았습니다.".to_string();
                    continue;
                }
                let intent_id = runtime.new_tui_intent_id();
                let lease = runtime.tui_selection_lease(&workflow_id)?;
                write_pre_dispatch_frame(terminal, &intent_id, "토큰을 무반향으로 입력하세요.\n")?;
                let Some(secret) = terminal.read_secret().map_err(terminal_fault_error)? else {
                    state.notice = "비밀 입력 EOF: 검증 승인을 보내지 않았습니다.".to_string();
                    continue;
                };
                let outcome = runtime.dispatch_tui_intent(TuiIntent::ApproveVerification {
                    intent_id: intent_id.clone(),
                    proposal_id: (*proposal_id).to_string(),
                    lease,
                    secret: OneShotSecret::new(secret)?,
                })?;
                let was_dispatched = outcome_was_dispatched(outcome.effect);
                state.notice = outcome_notice(outcome);
                post_dispatch_intent = was_dispatched.then_some(intent_id);
            }
            [action @ ("deny" | "resume" | "cancel")] => {
                let Some(workflow_id) = state.selected_id.clone() else {
                    state.notice = "먼저 select <workflow-id>를 실행하세요.".to_string();
                    continue;
                };
                if !confirm_workflow_action(terminal, action, &workflow_id)? {
                    state.notice = "요청을 보내지 않았습니다.".to_string();
                    continue;
                }
                let intent_id = runtime.new_tui_intent_id();
                let gate = (*action == "deny")
                    .then(|| runtime.tui_gate_descriptor(&workflow_id))
                    .transpose()?;
                let lease = runtime.tui_selection_lease(&workflow_id)?;
                write_pre_dispatch_frame(terminal, &intent_id, "정본 상태를 재검증했습니다.\n")?;
                let intent = match *action {
                    "deny" => TuiIntent::DenyPendingGate {
                        intent_id: intent_id.clone(),
                        workflow_id,
                        gate_id: gate.as_ref().expect("deny gate prepared").0.clone(),
                        gate_kind: gate.expect("deny gate prepared").1,
                        lease,
                    },
                    "resume" => TuiIntent::ResumeWorkflow {
                        intent_id: intent_id.clone(),
                        workflow_id,
                        lease,
                    },
                    "cancel" => TuiIntent::CancelWorkflow {
                        intent_id: intent_id.clone(),
                        workflow_id,
                        lease,
                    },
                    _ => unreachable!(),
                };
                let outcome = runtime.dispatch_tui_intent(intent)?;
                let was_dispatched = outcome_was_dispatched(outcome.effect);
                state.notice = outcome_notice(outcome);
                post_dispatch_intent = was_dispatched.then_some(intent_id);
            }
            [command, ..]
                if command.starts_with('/') && looks_like_attachment_path(line.trim()) =>
            {
                state.notice = capture_attachment_notice(runtime, &mut state, line.trim());
            }
            [command, ..] if command.starts_with('/') => {
                state.notice = format!("알 수 없는 TUI 명령입니다: {command}\n/help로 확인하세요.");
            }
            _ => {
                if looks_like_attachment_path(line.trim()) {
                    state.notice = capture_attachment_notice(runtime, &mut state, line.trim());
                    continue;
                }
                state.view = InteractiveView::Conversation;
                state.push_turn(ConversationRole::User, line.trim());
                state.notice = "작업 중 · 에이전트가 요청을 처리하고 있습니다…".to_string();
                write_pending_conversation_frame(terminal, runtime, &state, width, height)?;
                let response = match runtime.submit_request(line.trim(), &state.attachments) {
                    Ok(report) => {
                        state.clear_attachments();
                        report
                    }
                    Err(error) => format!(
                        "요청을 완료하지 못했습니다.\n{}\n첨부는 재시도를 위해 유지했습니다.",
                        error.message
                    ),
                };
                state.push_turn(ConversationRole::Assistant, response);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn submit_web_tool_command(
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
    state.notice = pending.to_string();
    write_pending_conversation_frame(terminal, runtime, state, width, height)?;
    let response = match runtime.submit_request(request, &[]) {
        Ok(report) => report,
        Err(error) => format!("{error_heading}\n{}", error.message),
    };
    state.push_turn(ConversationRole::Assistant, response);
    Ok(())
}

fn test_secret_probe_enabled() -> bool {
    cfg!(debug_assertions)
        && std::env::var_os("RPOTATO_TEST_TUI_SECRET_PROBE").as_deref()
            == Some(std::ffi::OsStr::new("1"))
}
