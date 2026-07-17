use crate::adapters::terminal::capability;
#[cfg(test)]
use crate::adapters::terminal::native::ScriptedTerminal;
use crate::adapters::terminal::native::{
    FrameWriteBoundary, NativeTerminal, TerminalFault, TerminalIo,
};
use crate::foundation::error::AppError;
use crate::runtime::{
    self, OneShotSecret, TuiEffect, TuiIntent, TuiOutcome, TuiOutcomeCode, TuiOutcomeContext,
    TuiReadBudget, TuiReadPage, TuiReadRequest,
};
use crate::surfaces::tui::view_model::{InteractiveState, InteractiveView};

const DEFAULT_WIDTH: usize = 92;
const MIN_WIDTH: usize = 64;
const MAX_WIDTH: usize = 120;

pub fn run_auto() -> Result<(), AppError> {
    if capability::attached() {
        let mut terminal = NativeTerminal::new();
        run_controller(&mut terminal)
    } else {
        println!("{}", overview_report()?);
        Ok(())
    }
}

pub fn run_interactive() -> Result<(), AppError> {
    let mut terminal = NativeTerminal::explicit_line_mode();
    run_controller(&mut terminal)
}

fn run_controller(terminal: &mut impl TerminalIo) -> Result<(), AppError> {
    terminal
        .validate_configuration()
        .map_err(terminal_fault_error)?;
    let mut state = InteractiveState::new();
    let mut post_dispatch_intent: Option<String> = None;

    loop {
        let (width, height) = terminal.dimensions().map_err(terminal_fault_error)?;
        let request = state.read_request(width, height);
        let page = runtime::read_tui_page(request)?;
        let frame = render_interactive_frame(&state, &page, width, height);
        let boundary = if post_dispatch_intent.is_some() {
            FrameWriteBoundary::PostDispatch
        } else {
            FrameWriteBoundary::Ordinary
        };
        if terminal.write_frame_at(&frame, boundary).is_err() {
            return Err(match post_dispatch_intent.take() {
                Some(intent_id) => post_dispatch_write_error(&intent_id),
                None => pre_dispatch_write_error(&runtime::new_tui_intent_id()),
            });
        }
        post_dispatch_intent = None;

        let Some(line) = terminal.read_line().map_err(terminal_fault_error)? else {
            return Ok(());
        };
        let words = line.split_whitespace().collect::<Vec<_>>();
        match words.as_slice() {
            [] | ["refresh"] => {
                state.notice = "정본 상태를 새로고침했습니다.".to_string();
            }
            ["quit"] | ["exit"] => return Ok(()),
            ["help"] => {
                state.notice = "help | view overview|monitor|sessions|approvals|evidence|transcript <session>|tool-output <artifact>|diff <proposal> | next | prev | select <canonical-id> | select session <session-id> | approve <proposal> | approve verification <proposal> | deny | resume | cancel | quit".to_string();
            }
            ["test-secret"] if test_secret_probe_enabled() => {
                let intent_id = runtime::new_tui_intent_id();
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
                let outcome = runtime::exact_tui_outcome(
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
                let intent_id = runtime::new_tui_intent_id();
                let lease = runtime::tui_selection_lease(session_id)?;
                write_pre_dispatch_frame(
                    terminal,
                    &intent_id,
                    "정본 세션 상태를 재검증했습니다.\n",
                )?;
                let outcome = runtime::dispatch_tui_intent(TuiIntent::SelectSession {
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
                    state.notice = outcome_notice(runtime::unsupported_source_platform_outcome(
                        std::env::consts::OS,
                    )?);
                    continue;
                }
                if !confirm(terminal, "패치 적용 승인을 확인하려면 yes를 입력하세요.\n")?
                {
                    state.notice = "승인을 보내지 않았습니다.".to_string();
                    continue;
                }
                let intent_id = runtime::new_tui_intent_id();
                let lease = runtime::tui_selection_lease(&workflow_id)?;
                write_pre_dispatch_frame(terminal, &intent_id, "토큰을 무반향으로 입력하세요.\n")?;
                let Some(secret) = terminal.read_secret().map_err(terminal_fault_error)? else {
                    state.notice = "비밀 입력 EOF: 승인을 보내지 않았습니다.".to_string();
                    continue;
                };
                let outcome = runtime::dispatch_tui_intent(TuiIntent::ApprovePatch {
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
                let intent_id = runtime::new_tui_intent_id();
                let lease = runtime::tui_selection_lease(&workflow_id)?;
                write_pre_dispatch_frame(terminal, &intent_id, "토큰을 무반향으로 입력하세요.\n")?;
                let Some(secret) = terminal.read_secret().map_err(terminal_fault_error)? else {
                    state.notice = "비밀 입력 EOF: 검증 승인을 보내지 않았습니다.".to_string();
                    continue;
                };
                let outcome = runtime::dispatch_tui_intent(TuiIntent::ApproveVerification {
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
                let intent_id = runtime::new_tui_intent_id();
                let gate = (*action == "deny")
                    .then(|| runtime::tui_gate_descriptor(&workflow_id))
                    .transpose()?;
                let lease = runtime::tui_selection_lease(&workflow_id)?;
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
                let outcome = runtime::dispatch_tui_intent(intent)?;
                let was_dispatched = outcome_was_dispatched(outcome.effect);
                state.notice = outcome_notice(outcome);
                post_dispatch_intent = was_dispatched.then_some(intent_id);
            }
            _ => {
                state.notice = "알 수 없는 명령입니다. help로 지원 명령을 확인하세요.".to_string();
            }
        }
    }
}

fn test_secret_probe_enabled() -> bool {
    cfg!(debug_assertions)
        && std::env::var_os("RPOTATO_TEST_TUI_SECRET_PROBE").as_deref()
            == Some(std::ffi::OsStr::new("1"))
}

fn render_interactive_frame(
    state: &InteractiveState,
    page: &TuiReadPage,
    width: u16,
    height: u16,
) -> String {
    let width = usize::from(width).clamp(20, MAX_WIDTH);
    let body_rows = usize::from(height).saturating_sub(5).max(1);
    let mut output = String::new();
    output.push_str(&format!(
        "rpotato interactive | {} | page {} | freshness {} | continuation {}\n",
        sanitize_terminal_text(&page.title),
        page.page + 1,
        page.freshness.as_str(),
        page.continuation.as_str(),
    ));
    output.push_str(&"-".repeat(width));
    output.push('\n');
    for line in page.lines.iter().take(body_rows) {
        output.push_str(&truncate_chars(&sanitize_terminal_text(line), width));
        output.push('\n');
    }
    render_notice_lines(&mut output, &state.notice, width);
    output.push_str("rpotato> ");
    output
}

fn render_notice_lines(output: &mut String, notice: &str, width: usize) {
    for (index, line) in notice.split('\n').enumerate() {
        let prefix = if index == 0 { "notice: " } else { "        " };
        output.push_str(prefix);
        output.push_str(&truncate_chars(
            &sanitize_terminal_text(line),
            width.saturating_sub(prefix.len()),
        ));
        output.push('\n');
    }
}

fn sanitize_terminal_text(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{001b}' {
            match chars.peek().copied() {
                Some('[') => {
                    chars.next();
                    for next in chars.by_ref() {
                        if ('@'..='~').contains(&next) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    chars.next();
                    let mut escaped = false;
                    for next in chars.by_ref() {
                        if next == '\u{0007}' || (escaped && next == '\\') {
                            break;
                        }
                        escaped = next == '\u{001b}';
                    }
                }
                Some(_) => {
                    chars.next();
                }
                None => {}
            }
            out.push_str("<esc>");
        } else if ch.is_control() {
            match ch {
                '\n' => out.push_str("<lf>"),
                '\r' => out.push_str("<cr>"),
                '\t' => out.push_str("  "),
                _ => out.push_str("<ctl>"),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn truncate_chars(value: &str, width: usize) -> String {
    let count = value.chars().count();
    if count <= width {
        return value.to_string();
    }
    if width <= 1 {
        return "…".chars().take(width).collect();
    }
    let mut out = value.chars().take(width - 1).collect::<String>();
    out.push('…');
    out
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
    match runtime::exact_tui_outcome(code, context) {
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

fn consume_outcome(
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

struct ConsumedOutcome {
    notice: String,
    was_dispatched: bool,
}

fn outcome_was_dispatched(effect: TuiEffect) -> bool {
    !matches!(effect, TuiEffect::NotDispatched)
}

mod legacy_reports {
    use super::{
        runtime, AppError, TuiReadBudget, TuiReadPage, TuiReadRequest, DEFAULT_WIDTH, MAX_WIDTH,
        MIN_WIDTH,
    };
    use crate::adapters::filesystem::layout as paths;
    use crate::{evidence, ledger, model, observability};

    pub fn overview_report() -> Result<String, AppError> {
        let width = terminal_width();
        let store = observability::status()?;
        let models = observability::model_summaries()?;
        let sessions = observability::session_history(5)?;
        let identity = ledger::validated_current_identity()?;

        let mut lines = Vec::new();
        push_header(&mut lines, width, "rpotato TUI beta - overview");
        push_kv(&mut lines, width, "project", &identity.project_root);
        push_kv(&mut lines, width, "session", &identity.session_id);
        push_kv(&mut lines, width, "mode", "read-only dashboard");
        push_rule(&mut lines, width);
        push_section(&mut lines, width, "runtime");
        push_kv(
            &mut lines,
            width,
            "observability",
            &store.path.display().to_string(),
        );
        push_kv(
            &mut lines,
            width,
            "ledger events",
            &store.ledger_events.to_string(),
        );
        push_kv(&mut lines, width, "sessions", &store.sessions.to_string());
        push_kv(&mut lines, width, "workflows", &store.workflows.to_string());
        push_kv(
            &mut lines,
            width,
            "transcript records",
            &store.transcript_records.to_string(),
        );
        push_kv(
            &mut lines,
            width,
            "transcript boundary",
            "visible/normalized turns persisted; hidden response and raw source excluded",
        );
        if let Some(path) = store.recovered_from {
            push_kv(
                &mut lines,
                width,
                "recovered db",
                &path.display().to_string(),
            );
        }
        push_rule(&mut lines, width);
        push_section(&mut lines, width, "model/token summary");
        if models.is_empty() {
            push_kv(
                &mut lines,
                width,
                "model runs",
                &format!("none; candidates {}", model::candidate_summary()),
            );
        } else {
            for summary in models.iter().take(4) {
                push_wrapped(
                    &mut lines,
                    width,
                    &format!(
                        "{} | runs {} | tokens {} | avg latency {} | avg tps {}",
                        summary.model_id,
                        summary.runs,
                        summary.total_tokens,
                        latency_label(summary.avg_latency_ms),
                        tps_label(summary.avg_tokens_per_second)
                    ),
                );
            }
        }
        push_rule(&mut lines, width);
        push_section(&mut lines, width, "recent sessions");
        if sessions.is_empty() {
            push_kv(&mut lines, width, "history", "none");
        } else {
            for session in sessions.iter().take(3) {
                push_wrapped(
                    &mut lines,
                    width,
                    &format!(
                        "{} | events {} | last {}",
                        short_id(&session.session_id),
                        session.event_count,
                        session
                            .last_summary
                            .as_deref()
                            .unwrap_or("no summary recorded")
                    ),
                );
            }
        }
        push_rule(&mut lines, width);
        push_kv(
        &mut lines,
        width,
        "views",
        "rpotato tui | rpotato tui monitor | rpotato tui sessions | rpotato tui transcript <session-id> | rpotato tui approvals | rpotato tui evidence",
    );
        push_footer(&mut lines, width);
        Ok(lines.join("\n"))
    }

    pub fn monitor_report() -> Result<String, AppError> {
        let width = terminal_width();
        let store = observability::status()?;
        let models = observability::model_summaries()?;
        let resource = observability::latest_resource_sample()?;

        let mut lines = Vec::new();
        push_header(&mut lines, width, "rpotato TUI beta - monitor");
        push_kv(
            &mut lines,
            width,
            "observability",
            &store.path.display().to_string(),
        );
        push_kv(
            &mut lines,
            width,
            "schema",
            &format!("v{}", store.migration_version),
        );
        push_kv(
            &mut lines,
            width,
            "model runs",
            &store.model_runs.to_string(),
        );
        push_kv(
            &mut lines,
            width,
            "token records",
            &store.token_records.to_string(),
        );
        push_kv(
            &mut lines,
            width,
            "transcript records",
            &store.transcript_records.to_string(),
        );
        push_kv(
            &mut lines,
            width,
            "resource samples",
            &store.resource_samples.to_string(),
        );
        push_rule(&mut lines, width);
        push_section(&mut lines, width, "resource pressure");
        if let Some(sample) = resource {
            push_wrapped(
                &mut lines,
                width,
                &format!(
                    "pressure: {} | backend: {} | pid: {} | sample count: {} | recorded ms: {}",
                    sample.pressure_status,
                    sample.backend_id,
                    sample.pid,
                    sample.sample_count,
                    sample.recorded_at_ms
                ),
            );
            push_wrapped(
                &mut lines,
                width,
                &format!(
                    "cpu: {} | avg rss: {}",
                    percent_label(sample.process_cpu_percent),
                    bytes_label(sample.average_rss_bytes)
                ),
            );
            push_wrapped(
                &mut lines,
                width,
                &format!(
                    "peak rss: {} | disk: {}",
                    bytes_label(sample.peak_rss_bytes),
                    bytes_label(sample.disk_bytes)
                ),
            );
            push_wrapped(
                &mut lines,
                width,
                &format!("latest sample: {}", short_id(&sample.resource_sample_id)),
            );
        } else {
            push_wrapped(
            &mut lines,
            width,
            "No resource samples yet. Run backend start, backend status, or backend chat after a sidecar is running.",
        );
        }
        push_rule(&mut lines, width);
        push_section(&mut lines, width, "models");
        if models.is_empty() {
            push_wrapped(
                &mut lines,
                width,
                &format!(
                    "No recorded model runs yet. Candidate state: {}",
                    model::candidate_summary()
                ),
            );
        } else {
            push_wrapped(
                &mut lines,
                width,
                "model | runs | prompt | completion | total | avg ms | tps",
            );
            for summary in &models {
                push_wrapped(
                    &mut lines,
                    width,
                    &format!(
                        "{} | {} | {} | {} | {} | {} | {}",
                        summary.model_id,
                        summary.runs,
                        summary.prompt_tokens,
                        summary.completion_tokens,
                        summary.total_tokens,
                        latency_label(summary.avg_latency_ms),
                        tps_label(summary.avg_tokens_per_second)
                    ),
                );
            }
        }
        push_rule(&mut lines, width);
        push_kv(
            &mut lines,
            width,
            "actions",
            "read-only; export/prune remain monitor CLI commands",
        );
        push_footer(&mut lines, width);
        Ok(lines.join("\n"))
    }

    pub fn sessions_report() -> Result<String, AppError> {
        let width = terminal_width();
        let identity = ledger::validated_current_identity()?;
        let sessions = observability::session_history(10)?;

        let mut lines = Vec::new();
        push_header(&mut lines, width, "rpotato TUI beta - sessions");
        push_kv(&mut lines, width, "project", &identity.project_root);
        push_kv(&mut lines, width, "current session", &identity.session_id);
        push_rule(&mut lines, width);
        if sessions.is_empty() {
            push_wrapped(
                &mut lines,
                width,
                "No session history yet. Start with `rpotato init` or `rpotato session new`.",
            );
        } else {
            push_wrapped(&mut lines, width, "session id | events | last summary");
            for session in &sessions {
                push_wrapped(
                    &mut lines,
                    width,
                    &format!(
                        "{} | {} | {}",
                        session.session_id,
                        session.event_count,
                        session
                            .last_summary
                            .as_deref()
                            .unwrap_or("no summary recorded")
                    ),
                );
            }
        }
        push_rule(&mut lines, width);
        push_kv(
            &mut lines,
            width,
            "resume",
            "rpotato session resume <session-id>",
        );
        push_kv(
            &mut lines,
            width,
            "inspect",
            "rpotato tui transcript <session-id>",
        );
        push_kv(
            &mut lines,
            width,
            "state",
            &paths::current_state_file().display().to_string(),
        );
        push_footer(&mut lines, width);
        Ok(lines.join("\n"))
    }

    pub fn transcript_report(session_id: &str) -> Result<String, AppError> {
        let width = terminal_width();
        let session = observability::session_entry(session_id)?.ok_or_else(|| {
        AppError::blocked(format!(
            "tui transcript 차단\n- session id: {}\n- 이유: 현재 project의 session history에서 찾지 못했습니다.\n- 확인: rpotato tui sessions",
            session_id
        ))
    })?;
        let events = observability::session_events(session_id, 40)?;
        let transcript = crate::transcript::records_for_session(session_id)?;

        let mut lines = Vec::new();
        push_header(&mut lines, width, "rpotato TUI beta - transcript");
        push_kv(&mut lines, width, "project", &session.project_root);
        push_kv(&mut lines, width, "session", &session.session_id);
        push_kv(
            &mut lines,
            width,
            "started",
            &session.started_at_ms.to_string(),
        );
        push_kv(
            &mut lines,
            width,
            "last event",
            &session
                .last_event_at_ms
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
        );
        push_kv(
            &mut lines,
            width,
            "events",
            &session.event_count.to_string(),
        );
        push_rule(&mut lines, width);
        push_section(&mut lines, width, "durable conversation");
        if transcript.is_empty() {
            push_wrapped(&mut lines, width, "No durable conversation turns recorded.");
        } else {
            for record in &transcript {
                push_wrapped(
                    &mut lines,
                    width,
                    &format!(
                        "{} | {} | {}",
                        record.kind,
                        short_id(&record.workflow_id),
                        record.content
                    ),
                );
            }
        }
        push_rule(&mut lines, width);
        push_section(&mut lines, width, "timeline");
        if events.is_empty() {
            push_wrapped(
                &mut lines,
                width,
                "No ledger events are projected for this session yet.",
            );
        } else {
            push_wrapped(&mut lines, width, "ts_ms | event type | event id | summary");
            for event in &events {
                push_wrapped(
                    &mut lines,
                    width,
                    &format!(
                        "{} | {} | {} | {}",
                        event.ts_ms,
                        event.event_type,
                        short_id(&event.event_id),
                        event.summary
                    ),
                );
            }
            if session.event_count > i64::try_from(events.len()).unwrap_or(i64::MAX) {
                push_wrapped(
                    &mut lines,
                    width,
                    &format!(
                        "showing first {} projected events; total event count is {}",
                        events.len(),
                        session.event_count
                    ),
                );
            }
        }
        push_rule(&mut lines, width);
        push_kv(
            &mut lines,
            width,
            "resume",
            &format!("rpotato session resume {}", session.session_id),
        );
        push_kv(
            &mut lines,
            width,
            "raw details",
            "not shown in the TUI beta by default",
        );
        push_footer(&mut lines, width);
        Ok(lines.join("\n"))
    }

    pub fn approvals_report() -> Result<String, AppError> {
        let page = runtime::read_tui_page(TuiReadRequest::Approvals {
            page: 0,
            budget: TuiReadBudget::bounded(40, 64 * 1024),
        })?;
        Ok(canonical_page_report(page))
    }

    pub fn diff_report(proposal_id: &str) -> Result<String, AppError> {
        let page = runtime::read_tui_page(TuiReadRequest::Diff {
            proposal_id: proposal_id.to_string(),
            page: 0,
            budget: TuiReadBudget::bounded(120, 64 * 1024),
        })?;
        Ok(canonical_page_report(page))
    }

    fn canonical_page_report(page: TuiReadPage) -> String {
        let width = terminal_width();
        let literal_content = page.title == "diff";
        let mut lines = Vec::new();
        push_header(
            &mut lines,
            width,
            &format!("rpotato TUI beta - {}", page.title),
        );
        push_kv(&mut lines, width, "page", &(page.page + 1).to_string());
        push_kv(&mut lines, width, "freshness", page.freshness.as_str());
        push_kv(
            &mut lines,
            width,
            "continuation",
            page.continuation.as_str(),
        );
        push_rule(&mut lines, width);
        push_section(&mut lines, width, "canonical authority");
        push_kv(
            &mut lines,
            width,
            "current",
            &authority_pair(
                page.authority.current_revision,
                page.authority.current_hash.as_deref(),
            ),
        );
        push_kv(
            &mut lines,
            width,
            "workflow",
            &authority_pair(
                page.authority.workflow_revision,
                page.authority.workflow_hash.as_deref(),
            ),
        );
        push_kv(
            &mut lines,
            width,
            "ledger",
            &authority_pair(
                page.authority.ledger_sequence,
                page.authority.ledger_hash.as_deref(),
            ),
        );
        push_kv(
            &mut lines,
            width,
            "content hash",
            page.authority
                .content_hash
                .as_deref()
                .unwrap_or("unavailable"),
        );
        push_kv(
            &mut lines,
            width,
            "validated at ms",
            &page
                .authority
                .validated_at_ms
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unavailable".to_string()),
        );
        push_rule(&mut lines, width);
        push_section(&mut lines, width, "content");
        if page.lines.is_empty() {
            push_wrapped(&mut lines, width, "No canonical records are available.");
        } else {
            for (index, line) in page.lines.iter().enumerate() {
                if literal_content && index > 0 {
                    push_literal_block(&mut lines, width, line);
                } else {
                    push_wrapped(&mut lines, width, line);
                }
            }
        }
        push_footer(&mut lines, width);
        lines.join("\n")
    }

    fn authority_pair(revision: Option<u64>, hash: Option<&str>) -> String {
        match (revision, hash) {
            (Some(revision), Some(hash)) => format!("revision={revision} hash={hash}"),
            _ => "unavailable".to_string(),
        }
    }

    pub fn evidence_report() -> Result<String, AppError> {
        let width = terminal_width();
        let identity = ledger::validated_current_identity()?;
        let store = observability::status()?;
        let evidence = evidence::store_status()?;

        let mut lines = Vec::new();
        push_header(&mut lines, width, "rpotato TUI beta - evidence");
        push_kv(&mut lines, width, "project", &identity.project_root);
        push_kv(&mut lines, width, "session", &identity.session_id);
        push_kv(&mut lines, width, "mode", "read-only evidence status");
        push_rule(&mut lines, width);
        push_section(&mut lines, width, "stores");
        push_kv(
            &mut lines,
            width,
            "runtime evidence",
            &evidence.runtime_evidence_file.display().to_string(),
        );
        push_kv(
            &mut lines,
            width,
            "runtime records",
            &evidence.runtime_evidence_records.to_string(),
        );
        push_kv(
            &mut lines,
            width,
            "project evidence",
            &evidence.project_evidence_dir.display().to_string(),
        );
        push_kv(
            &mut lines,
            width,
            "project artifacts",
            &evidence.project_artifacts.to_string(),
        );
        push_kv(
            &mut lines,
            width,
            "observability",
            &store.path.display().to_string(),
        );
        push_rule(&mut lines, width);
        push_section(&mut lines, width, "stop gate boundary");
        push_kv(
            &mut lines,
            width,
            "recorded evidence",
            &store.evidence_records.to_string(),
        );
        push_kv(
            &mut lines,
            width,
            "stop gate results",
            &store.stop_gate_results.to_string(),
        );
        push_kv(&mut lines, width, "stale policy", evidence.stale_policy);
        push_kv(
            &mut lines,
            width,
            "terminal gate",
            "not implemented; this view does not pass or fail workflows",
        );
        push_rule(&mut lines, width);
        push_kv(
            &mut lines,
            width,
            "validate",
            "rpotato evidence validate <artifact-pointer>",
        );
        push_kv(
            &mut lines,
            width,
            "raw prompt/source",
            "disabled by default",
        );
        push_footer(&mut lines, width);
        Ok(lines.join("\n"))
    }

    fn terminal_width() -> usize {
        std::env::var("COLUMNS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(DEFAULT_WIDTH)
            .clamp(MIN_WIDTH, MAX_WIDTH)
    }

    fn push_header(lines: &mut Vec<String>, width: usize, title: &str) {
        push_border(lines, width, '=');
        push_center(lines, width, title);
        push_border(lines, width, '=');
    }

    fn push_footer(lines: &mut Vec<String>, width: usize) {
        push_border(lines, width, '=');
        push_wrapped(
        lines,
        width,
        "beta boundary: this TUI surface reads runtime state only and does not approve, apply, resume, cancel, or mutate workflows.",
    );
    }

    fn push_section(lines: &mut Vec<String>, width: usize, label: &str) {
        push_wrapped(lines, width, &format!("[{label}]"));
    }

    fn push_rule(lines: &mut Vec<String>, width: usize) {
        push_border(lines, width, '-');
    }

    fn push_border(lines: &mut Vec<String>, width: usize, ch: char) {
        lines.push(ch.to_string().repeat(width));
    }

    fn push_center(lines: &mut Vec<String>, width: usize, value: &str) {
        let value = truncate(value, width);
        let padding = width.saturating_sub(value.len()) / 2;
        lines.push(format!("{}{}", " ".repeat(padding), value));
    }

    fn push_kv(lines: &mut Vec<String>, width: usize, key: &str, value: &str) {
        push_wrapped(lines, width, &format!("{key}: {value}"));
    }

    fn push_wrapped(lines: &mut Vec<String>, width: usize, value: &str) {
        let mut current = String::new();
        for word in value.split_whitespace() {
            let next_len = if current.is_empty() {
                word.len()
            } else {
                current.len() + 1 + word.len()
            };
            if next_len > width && !current.is_empty() {
                lines.push(truncate(&current, width));
                current.clear();
            }
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
        if current.is_empty() {
            lines.push(String::new());
        } else {
            lines.push(truncate(&current, width));
        }
    }

    fn push_literal_block(lines: &mut Vec<String>, width: usize, value: &str) {
        for line in value.lines() {
            lines.push(truncate(line, width));
        }
        if value.is_empty() {
            lines.push(String::new());
        }
    }

    fn truncate(value: &str, width: usize) -> String {
        if value.chars().count() <= width {
            return value.to_string();
        }
        if width <= 3 {
            return ".".repeat(width);
        }
        let prefix = value.chars().take(width - 3).collect::<String>();
        format!("{prefix}...")
    }

    fn latency_label(value: Option<f64>) -> String {
        value
            .map(|latency| format!("{latency:.1}ms"))
            .unwrap_or_else(|| "not recorded".to_string())
    }

    fn tps_label(value: Option<f64>) -> String {
        value
            .map(|value| format!("{value:.1} tok/s"))
            .unwrap_or_else(|| "not recorded".to_string())
    }

    fn percent_label(value: Option<f64>) -> String {
        value
            .map(|value| format!("{value:.1}%"))
            .unwrap_or_else(|| "unknown".to_string())
    }

    fn bytes_label(value: Option<u64>) -> String {
        let Some(value) = value else {
            return "unknown".to_string();
        };
        const KIB: f64 = 1024.0;
        const MIB: f64 = KIB * 1024.0;
        const GIB: f64 = MIB * 1024.0;
        let value = value as f64;
        if value >= GIB {
            format!("{:.1} GiB", value / GIB)
        } else if value >= MIB {
            format!("{:.1} MiB", value / MIB)
        } else if value >= KIB {
            format!("{:.1} KiB", value / KIB)
        } else {
            format!("{value:.0} B")
        }
    }

    fn short_id(value: &str) -> String {
        if value.len() <= 18 {
            return value.to_string();
        }
        format!("{}...", &value[..18])
    }
}

pub use legacy_reports::{
    approvals_report, diff_report, evidence_report, monitor_report, overview_report,
    sessions_report, transcript_report,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::filesystem::layout as paths;
    use crate::{ledger, observability, patch};

    #[test]
    fn interactive_view_change_resets_page_and_updates_notice() {
        let mut state = InteractiveState {
            view: InteractiveView::Sessions,
            page: 4,
            selected_id: Some("workflow-selected".to_string()),
            notice: "old notice".to_string(),
        };

        state.set_view(InteractiveView::Transcript("session-next".to_string()));

        assert_eq!(
            state.view,
            InteractiveView::Transcript("session-next".to_string())
        );
        assert_eq!(state.page, 0);
        assert_eq!(state.selected_id.as_deref(), Some("workflow-selected"));
        assert_eq!(state.notice, "화면을 변경했습니다.");
    }

    #[test]
    fn interactive_view_builds_bounded_read_request_from_viewport() {
        let state = InteractiveState {
            view: InteractiveView::ToolOutput("artifact-one".to_string()),
            page: 3,
            selected_id: None,
            notice: String::new(),
        };

        let request = state.read_request(10, 8);

        assert_eq!(
            request,
            TuiReadRequest::ToolOutput {
                artifact_id: "artifact-one".to_string(),
                page: 3,
                budget: TuiReadBudget::bounded(2, 20),
            }
        );
    }

    #[test]
    fn one_shot_outcome_writes_secret_once_without_storing_it_in_notice() {
        let intent_id = "intent-one-shot-test";
        let secret = "ab".repeat(32);
        let outcome = runtime::verification_credential_issued(
            intent_id,
            OneShotSecret::new(secret.clone()).unwrap(),
        )
        .unwrap();
        let mut terminal = ScriptedTerminal::new([]);

        let notice = consume_outcome(&mut terminal, intent_id, outcome).unwrap();

        assert_eq!(terminal.frames.len(), 3);
        let rendered = terminal.frames.concat();
        assert_eq!(
            rendered.matches(&secret).count(),
            1,
            "credential must be written exactly once"
        );
        assert!(notice.was_dispatched);
        assert!(!notice.notice.contains(&secret));
        assert!(notice.notice.contains("verification.credential-issued"));
    }

    #[test]
    fn ordinary_line_read_failure_has_a_distinct_non_secret_taxonomy() {
        let error = terminal_fault_error(TerminalFault::LineRead);

        assert!(error.message.contains("terminal.capability.mode-read"));
        assert!(!error.message.contains("terminal.secret-read.failed"));
    }

    #[test]
    fn live_controller_compile_time_boundary_uses_only_runtime_and_terminal_authority() {
        let live = include_str!("tui.rs")
            .split_once("mod legacy_reports {")
            .unwrap()
            .0;
        for forbidden in [
            "use crate::approval",
            "use crate::{evidence",
            "ledger::",
            "observability::",
            "patch::",
            "state::",
        ] {
            assert!(
                !live.contains(forbidden),
                "live boundary escaped via {forbidden}"
            );
        }
        assert!(live.contains("runtime::read_tui_page(request)"));
        assert!(live.contains("runtime::dispatch_tui_intent"));
    }

    #[test]
    fn one_shot_approval_and_diff_views_use_the_canonical_runtime_facade() {
        let source = include_str!("tui.rs");
        let legacy = source
            .split_once("mod legacy_reports {")
            .unwrap()
            .1
            .split_once("\npub use legacy_reports")
            .unwrap()
            .0;

        assert!(legacy.contains("runtime::read_tui_page(TuiReadRequest::Approvals"));
        assert!(legacy.contains("runtime::read_tui_page(TuiReadRequest::Diff"));
        assert!(!legacy.contains("proposal_summaries("));
        assert!(!legacy.contains("request_summaries("));
        assert!(!legacy.contains("proposal_detail("));
    }

    #[test]
    fn echo_restore_failure_exits_without_retrying_secret_input() {
        let error = terminal_fault_error(TerminalFault::EchoRestore);

        assert!(error.message.contains("terminal.echo-restore.failed"));
        assert!(error.message.contains("재시도하지 않고 TUI를 종료"));
        assert!(error.message.contains("stty echo"));
        assert!(!error.message.contains("terminal.secret-read.failed"));
    }

    #[test]
    fn interactive_controller_exits_cleanly_and_never_emits_terminal_injection() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = test_root("rpotato-interactive-controller-test");
        std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::fs::create_dir_all(root.join("project")).unwrap();
        crate::state::initialize().unwrap();
        let mut terminal = ScriptedTerminal::new(["help", "quit"]);

        run_controller(&mut terminal).unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = std::fs::remove_dir_all(root);
        assert!(terminal.frames.len() >= 2);
        assert!(terminal
            .frames
            .iter()
            .all(|frame| !frame.contains('\u{001b}')));
        assert!(terminal
            .frames
            .iter()
            .any(|frame| frame.contains("rpotato>")));
    }

    #[test]
    fn interactive_sanitizer_escapes_ansi_osc_and_control_bytes() {
        let hostile = "safe\u{001b}[31mred\u{001b}[0m\u{001b}]0;title\u{0007}\nnext\u{0000}";
        let sanitized = sanitize_terminal_text(hostile);

        assert_eq!(sanitized, "safe<esc>red<esc><esc><lf>next<ctl>");
        assert!(!sanitized.contains('\u{001b}'));
        assert!(!sanitized.contains('\u{0000}'));
    }

    #[test]
    fn exact_outcome_notice_preserves_trusted_multiline_structure() {
        let state = InteractiveState {
            view: InteractiveView::Overview,
            page: 0,
            selected_id: None,
            notice: "결과 제목\n- code: exact.test\n- 동작: 상태를 변경하지 않았습니다."
                .to_string(),
        };
        let page = TuiReadPage {
            title: "overview".to_string(),
            lines: Vec::new(),
            page: 0,
            has_previous: false,
            has_next: false,
            freshness: runtime::TuiFreshness::Fresh,
            continuation: runtime::TuiReadContinuation::Complete,
            authority: runtime::TuiReadAuthority::default(),
        };

        let frame = render_interactive_frame(&state, &page, 120, 40);

        assert!(frame.contains(
            "notice: 결과 제목\n        - code: exact.test\n        - 동작: 상태를 변경하지 않았습니다.\n"
        ));
        assert!(!frame.contains("<lf>"));
    }

    #[test]
    fn overview_renders_read_only_dashboard() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!("rpotato-tui-test-{}", std::process::id()));
        std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("COLUMNS", "72");

        let report = overview_report().unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("COLUMNS");

        assert!(report.contains("rpotato TUI beta - overview"));
        assert!(report.contains("mode: read-only dashboard"));
        assert!(report.contains("[runtime]"));
        assert!(report.contains("beta boundary"));
    }

    #[test]
    fn monitor_renders_model_section() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root =
            std::env::temp_dir().join(format!("rpotato-tui-monitor-test-{}", std::process::id()));
        std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let report = monitor_report().unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");

        assert!(report.contains("rpotato TUI beta - monitor"));
        assert!(report.contains("[resource pressure]"));
        assert!(report.contains("resource samples: 0"));
        assert!(report.contains("No resource samples yet"));
        assert!(report.contains("[models]"));
        assert!(report.contains("No recorded model runs yet"));
    }

    #[test]
    fn monitor_renders_resource_pressure_and_token_throughput() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = test_root("rpotato-tui-resource-monitor-test");
        let project_root = root.join("project");
        std::fs::create_dir_all(&project_root).unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("COLUMNS", "64");
        let identity = ledger::validated_current_identity().unwrap();

        observability::record_model_run(&observability::ModelRunMetric {
            model_run_id: "model-run-tui-resource".to_string(),
            session_id: identity.session_id.clone(),
            workflow_id: None,
            model_id: "qwen-test".to_string(),
            model_artifact_hash: None,
            backend_id: Some("llama.cpp".to_string()),
            backend_version: None,
            quantization: None,
            context_limit_tokens: Some(4096),
            started_at_ms: 1000,
            first_token_latency_ms: Some(25.0),
            total_latency_ms: Some(200.0),
            prompt_eval_ms: None,
            generation_eval_ms: None,
            tokens_per_second: Some(12.5),
            cancelled: false,
            token_usage_complete: true,
            prompt_tokens: 10,
            completion_tokens: 20,
            total_tokens: 30,
            context_tokens_used: 10,
            context_tokens_dropped: 0,
            ontology_tokens: 0,
            tool_summary_tokens: 0,
            max_output_tokens: Some(64),
        })
        .unwrap();
        observability::record_resource_sample(&observability::ResourceSampleMetric {
            resource_sample_id: "resource-sample-tui-resource".to_string(),
            session_id: identity.session_id,
            backend_id: "llama.cpp".to_string(),
            pid: 12345,
            process_cpu_percent: Some(84.2),
            average_rss_bytes: Some(256 * 1024 * 1024),
            peak_rss_bytes: Some(512 * 1024 * 1024),
            disk_bytes: Some(1536),
            sample_count: 3,
            pressure_status: "degraded".to_string(),
            recorded_at_ms: 2000,
        })
        .unwrap();

        let report = monitor_report().unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("COLUMNS");
        let _ = std::fs::remove_dir_all(root);

        assert!(report.contains("[resource pressure]"));
        assert!(report.contains("resource samples: 1"));
        assert!(report.contains("pressure: degraded"));
        assert!(report.contains("cpu: 84.2%"));
        assert!(report.contains("avg rss: 256.0 MiB"));
        assert!(report.contains("peak rss: 512.0 MiB"));
        assert!(report.contains("disk: 1.5 KiB"));
        assert!(report.contains("avg ms | tps"));
        assert!(report.contains("12.5 tok/s"));
    }

    #[test]
    fn sessions_renders_resume_hint() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root =
            std::env::temp_dir().join(format!("rpotato-tui-sessions-test-{}", std::process::id()));
        std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let report = sessions_report().unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");

        assert!(report.contains("rpotato TUI beta - sessions"));
        assert!(report.contains("resume: rpotato session resume <session-id>"));
    }

    #[test]
    fn transcript_renders_session_event_timeline() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = test_root("rpotato-tui-transcript-test");
        std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let session = crate::state::session_new_report().unwrap();
        let session_id = report_value(&session, "session id").unwrap();
        crate::state::record_event("test.first", "first transcript event", "details one").unwrap();
        crate::state::record_event("test.second", "second transcript event", "details two")
            .unwrap();
        let report = transcript_report(&session_id).unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");

        assert!(report.contains("rpotato TUI beta - transcript"));
        assert!(report.contains(&format!("session: {session_id}")));
        assert!(report.contains("[timeline]"));
        assert!(report.contains("test.first"));
        assert!(report.contains("first transcript event"));
        assert!(report.contains("test.second"));
        assert!(report.contains("raw details: not shown"));
    }

    #[test]
    fn approvals_renders_empty_queue() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = test_root("rpotato-tui-approvals-empty-test");
        std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        crate::state::initialize().unwrap();

        let report = approvals_report().unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");

        assert!(report.contains("rpotato TUI beta - approvals"));
        assert!(report.contains("No canonical records are available."));
        assert!(report.contains("continuation: complete"));
    }

    #[test]
    fn one_shot_views_do_not_admit_unbound_directory_only_proposals() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = test_root("rpotato-tui-diff-test");
        let project_root = root.join("project");
        std::fs::create_dir_all(project_root.join("src")).unwrap();
        std::fs::write(project_root.join("src/lib.rs"), "pub const X: i32 = 1;\n").unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        crate::state::initialize().unwrap();

        let preview = patch::preview_report("src/lib.rs", "1", "2").unwrap();
        let proposal_id = report_value(&preview, "proposal id").unwrap();
        let approvals = approvals_report().unwrap();
        let diff = diff_report(&proposal_id).unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");

        assert!(!approvals.contains(&proposal_id));
        assert!(approvals.contains("No canonical records are available."));
        assert!(diff.contains("rpotato TUI beta - diff"));
        assert!(diff.contains("continuation: unavailable"));
        assert!(diff.contains("active workflow canonical binding이 없습니다."));
        assert!(!diff.contains("-pub const X: i32 = 1;"));
        assert!(!diff.contains("+pub const X: i32 = 2;"));
        assert!(!diff.contains("--token "));
    }

    #[test]
    fn approvals_renders_team_admission_request() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = test_root("rpotato-tui-approvals-team-test");
        let project_root = root.join("project");
        std::fs::create_dir_all(&project_root).unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        crate::state::initialize().unwrap();

        crate::observability::record_resource_sample(&crate::observability::ResourceSampleMetric {
            resource_sample_id: "resource-sample-tui-approvals-team".to_string(),
            session_id: "session-tui-approvals-team".to_string(),
            backend_id: "llama.cpp".to_string(),
            pid: 4242,
            process_cpu_percent: Some(12.0),
            average_rss_bytes: Some(512 * 1024 * 1024),
            peak_rss_bytes: Some(512 * 1024 * 1024),
            disk_bytes: Some(2048),
            sample_count: 1,
            pressure_status: "normal".to_string(),
            recorded_at_ms: 1234,
        })
        .unwrap();
        let err =
            crate::team::admission_report(2, &["README.md".to_string()], &[], &[]).unwrap_err();
        let report = approvals_report().unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");

        assert!(err.message.contains("approval request: team-event-"));
        assert!(report.contains("team-admission"));
        assert!(report.contains("pending-approval"), "{report}");
        assert!(report.contains("canonical-event="));
    }

    #[test]
    fn evidence_renders_stop_gate_status_without_mutating() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = test_root("rpotato-tui-evidence-test");
        let project_root = root.join("project");
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("COLUMNS", "68");

        std::fs::create_dir_all(paths::state_dir()).unwrap();
        std::fs::create_dir_all(paths::project_evidence_dir()).unwrap();
        std::fs::write(
            paths::runtime_evidence_file(),
            "{\"evidence_id\":\"one\"}\n",
        )
        .unwrap();
        std::fs::write(paths::project_evidence_dir().join("one.txt"), "one").unwrap();

        let report = evidence_report().unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("COLUMNS");

        assert!(report.contains("rpotato TUI beta - evidence"));
        assert!(report.contains("mode: read-only evidence status"));
        assert!(report.contains("runtime records: 1"));
        assert!(report.contains("project artifacts: 1"));
        assert!(report.contains("[stop gate boundary]"));
        assert!(report.contains("terminal gate: not implemented"));
        assert!(report.contains("validate: rpotato evidence validate <artifact-pointer>"));
        assert!(report.contains("beta boundary"));
    }

    fn test_root(name: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{name}-{}-{nanos}", std::process::id()))
    }

    fn report_value(report: &str, key: &str) -> Option<String> {
        let prefix = format!("- {key}: ");
        report
            .lines()
            .find_map(|line| line.strip_prefix(&prefix).map(|value| value.to_string()))
    }
}
