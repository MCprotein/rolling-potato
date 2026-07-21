use crate::foundation::error::AppError;
use crate::runtime_core::terminal::{FrameWriteBoundary, TerminalFault, TerminalIo};

use super::outcome::{exact_tui_outcome, TuiEffect, TuiOutcome, TuiOutcomeCode, TuiOutcomeContext};
use super::runtime_bridge::{
    OneShotSecret, SelectionLease, TuiGateKind, TuiIntent, TuiModelOption, TuiReadPage,
    TuiReadRequest, TuiStatusSnapshot,
};
use super::view_model::{InteractiveState, InteractiveView};

pub(crate) trait TuiRuntimePort {
    fn read_tui_page(&mut self, request: TuiReadRequest) -> Result<TuiReadPage, AppError>;
    fn read_tui_status(&mut self) -> Result<TuiStatusSnapshot, AppError>;
    fn model_options(&mut self) -> Vec<TuiModelOption>;
    fn setup_model(&mut self, id: &str) -> Result<String, AppError>;
    fn doctor_report(&mut self) -> String;
    fn compact_context(&mut self) -> Result<String, AppError>;
    fn submit_request(&mut self, request: &str) -> Result<String, AppError>;
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
    let mut post_dispatch_intent: Option<String> = None;

    loop {
        let (width, height) = terminal.dimensions().map_err(terminal_fault_error)?;
        let request = state.read_request(width, height);
        let page = runtime.read_tui_page(request)?;
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

        let Some(line) = terminal.read_line().map_err(terminal_fault_error)? else {
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
                state.notice = "요청을 바로 입력하세요.\n- /model [id]: 모델 확인/변경\n- /compact: 현재 대화 컨텍스트 압축\n- /status: 상태 새로고침\n- /sessions: 세션 목록\n- /doctor: 환경 진단\n- /more, /back: 긴 응답 페이지 이동\n- /clear: 알림 지우기\n- /help: 도움말\n- /quit: 종료\n고급 호환 명령: rpotato debug --help".to_string();
            }
            ["/more"] => state.next_notice_page(height),
            ["/back"] => state.previous_notice_page(),
            ["/compact"] => {
                state.notice = match runtime.compact_context() {
                    Ok(report) => report,
                    Err(error) => error.message,
                };
            }
            ["/status"] => {
                state.notice = "모델·컨텍스트·backend·세션 상태를 새로고침했습니다.".to_string();
            }
            ["/sessions"] => state.set_view(InteractiveView::Sessions),
            ["/doctor"] => {
                state.notice = runtime.doctor_report();
            }
            ["/clear"] => {
                state.notice.clear();
            }
            ["/model"] => {
                state.notice = model_options_notice(&runtime.model_options());
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
                if !confirm(
                    terminal,
                    &format!(
                        "{} ({}) 다운로드 및 적용을 확인하려면 yes를 입력하세요.\n",
                        selected.display_name,
                        bytes_label(selected.download_bytes)
                    ),
                )? {
                    state.notice = "모델 변경을 취소했습니다.".to_string();
                    continue;
                }
                terminal
                    .write_frame(
                        "backend 준비 → 모델 다운로드/SHA-256 검증 → 기본 모델 적용 중...\n",
                    )
                    .map_err(|_| terminal_fault_error(TerminalFault::FrameWrite))?;
                state.notice = model_setup_notice(runtime.setup_model(id));
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
                if !confirm(terminal, "세션 선택을 확인하려면 yes를 입력하세요.\n")?
                {
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
                if !confirm(terminal, "패치 적용 승인을 확인하려면 yes를 입력하세요.\n")?
                {
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
                if !confirm(terminal, "검증 실행 승인을 확인하려면 yes를 입력하세요.\n")?
                {
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
                if !confirm(terminal, "상태 변경을 확인하려면 yes를 입력하세요.\n")?
                {
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
            [command, ..] if command.starts_with('/') => {
                state.notice = format!("알 수 없는 TUI 명령입니다: {command}\n/help로 확인하세요.");
            }
            _ => {
                state.notice = match runtime.submit_request(line.trim()) {
                    Ok(report) => report,
                    Err(error) => error.message,
                };
            }
        }
    }
}

fn model_options_notice(options: &[TuiModelOption]) -> String {
    let mut lines = vec!["사용 가능한 모델".to_string()];
    for option in options {
        let recommendation = if option.recommended { " | 권장" } else { "" };
        lines.push(format!(
            "- {} | {} | {} | context {} | RAM {} | {}{}\n  근거: {}",
            option.id,
            option.quantization,
            bytes_label(option.download_bytes),
            option
                .context_length
                .map(compact_tokens)
                .unwrap_or_else(|| "미확정".to_string()),
            option.ram,
            option.license,
            recommendation,
            option.note,
        ));
    }
    lines.push("변경: /model <id>".to_string());
    lines.join("\n")
}

fn model_setup_notice(result: Result<String, AppError>) -> String {
    match result {
        Ok(report) => report,
        Err(error) => format!(
            "모델 변경 실패\n- 이유: {}\n- TUI는 계속 실행됩니다. /model로 다시 선택하세요.",
            error.message
        ),
    }
}

fn bytes_label(bytes: u64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    format!("{:.1} GiB", bytes as f64 / GIB)
}

fn compact_tokens(tokens: u32) -> String {
    if tokens >= 1000 {
        format!("{}k", tokens / 1000)
    } else {
        tokens.to_string()
    }
}

fn test_secret_probe_enabled() -> bool {
    cfg!(debug_assertions)
        && std::env::var_os("RPOTATO_TEST_TUI_SECRET_PROBE").as_deref()
            == Some(std::ffi::OsStr::new("1"))
}

fn confirm(terminal: &mut impl TerminalIo, prompt: &str) -> Result<bool, AppError> {
    terminal
        .write_frame(prompt)
        .map_err(|_| terminal_fault_error(TerminalFault::FrameWrite))?;
    Ok(matches!(
        terminal
            .read_line()
            .map_err(terminal_fault_error)?
            .as_deref(),
        Some("yes")
    ))
}

fn write_pre_dispatch_frame(
    terminal: &mut impl TerminalIo,
    intent_id: &str,
    prompt: &str,
) -> Result<(), AppError> {
    terminal
        .write_frame_at(prompt, FrameWriteBoundary::PreDispatch)
        .map_err(|_| pre_dispatch_write_error(intent_id))
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

fn pre_dispatch_write_error(intent_id: &str) -> AppError {
    exact_outcome_error(
        TuiOutcomeCode::TerminalFrameWritePreDispatch,
        TuiOutcomeContext {
            intent_id: Some(intent_id),
            ..TuiOutcomeContext::default()
        },
    )
}

fn post_dispatch_write_error(intent_id: &str) -> AppError {
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

fn outcome_notice(outcome: TuiOutcome) -> String {
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

fn outcome_was_dispatched(effect: TuiEffect) -> bool {
    !matches!(effect, TuiEffect::NotDispatched)
}

#[cfg(test)]
mod usability_tests {
    use super::model_setup_notice;
    use crate::foundation::error::AppError;

    #[test]
    fn recoverable_model_setup_error_becomes_a_notice_instead_of_exiting() {
        let notice = model_setup_notice(Err(AppError::runtime("download failed")));

        assert!(notice.contains("모델 변경 실패"));
        assert!(notice.contains("download failed"));
        assert!(notice.contains("TUI는 계속 실행됩니다"));
    }
}
