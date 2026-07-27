use super::*;
use crate::adapters::terminal::native::ScriptedTerminal;
use crate::foundation::error::AppError;
use crate::surfaces::tui::controller::{run_controller, TuiRuntimePort};
use crate::surfaces::tui::render::{
    display_cell_width, render_interactive_frame, render_interactive_frame_with_options,
};
use crate::surfaces::tui::runtime_bridge::{
    SelectionLease, TuiAttachment, TuiAttachmentKind, TuiConversationRole, TuiConversationTurn,
    TuiFreshness, TuiGateKind, TuiIntent, TuiModelOption, TuiReadContinuation, TuiReadPage,
    TuiReadRequest, TuiSessionOption, TuiSessionTransition, TuiStatusSnapshot, TuiWebSourceOption,
};
use crate::surfaces::tui::view_model::{ConversationRole, InteractiveState};

#[test]
fn default_interactive_frame_is_conversation_first_and_hides_runtime_internals() {
    let state = InteractiveState::new();
    let page = TuiReadPage {
        title: "overview".to_string(),
        lines: vec![
            "current: revision=21 hash=secret-current-hash".to_string(),
            "ledger: sequence=145 hash=secret-ledger-hash".to_string(),
            "projected workflows: 1".to_string(),
        ],
        page: 0,
        has_previous: false,
        has_next: false,
        freshness: TuiFreshness::Fresh,
        continuation: TuiReadContinuation::Truncated,
        authority: crate::surfaces::tui::runtime_bridge::TuiReadAuthority::default(),
    };

    let frame = render_interactive_frame(&state, &page, 120, 40);

    assert!(frame.contains("╭─ rpotato v"));
    assert!(frame.contains("│ model"));
    assert!(frame.contains("│ project"));
    assert!(frame.contains("╰─ /help 명령 · /model 변경"));
    assert!(frame.contains("› "));
    let welcome_footer = frame
        .lines()
        .position(|line| line.contains("╰─ /help 명령 · /model 변경"))
        .expect("welcome footer");
    assert_eq!(
        frame.lines().nth(welcome_footer + 1),
        Some(""),
        "welcome footer and first notice need one blank row"
    );
    for hidden in [
        "freshness",
        "continuation",
        "secret-current-hash",
        "secret-ledger-hash",
        "projected workflows",
    ] {
        assert!(!frame.contains(hidden), "default frame leaked {hidden}");
    }
}

#[test]
fn ordinary_input_renders_as_user_and_assistant_turns() {
    let mut terminal = ScriptedTerminal::new(["안녕", "/quit"]);
    let mut runtime = ConversationRuntime::default();

    run_controller(&mut terminal, &mut runtime).unwrap();

    let rendered = terminal.frames.join("\n");
    assert_eq!(runtime.requests, ["안녕"]);
    assert_eq!(runtime.page_reads, 0, "default chat must not read overview");
    assert!(terminal.frames[1].contains("› 안녕"));
    assert!(!terminal.frames[1].contains("● 안녕하세요."));
    assert!(terminal.frames[1].contains("◇ ⠋ 처리 중"));
    assert!(!terminal.frames[1].contains("notice:"));
    assert!(rendered.contains("› 안녕"));
    assert!(rendered.contains("● 안녕하세요."));
    assert!(!rendered.contains("ledger: must stay hidden"));
    assert!(!rendered.contains("patch proposal"));
}

#[test]
fn browser_request_renders_structured_progress_before_synchronous_execution() {
    let mut terminal = ScriptedTerminal::new(["네이버를 열고 검색창에 월드컵을 입력해", "/quit"]);
    let mut runtime = ConversationRuntime {
        progress_hint: Some(
            "브라우저 조사 · 공개 검색 페이지 여는 중\n페이지 열기 ● → 검색창 확인 ○ → 검색어 입력 ○ → 결과 읽기 ○"
                .to_string(),
        ),
        ..ConversationRuntime::default()
    };

    run_controller(&mut terminal, &mut runtime).unwrap();

    assert!(terminal.frames[1].contains("브라우저 조사"));
    assert!(terminal.frames[1].contains("페이지 열기 ●"));
    assert!(terminal.frames[1].contains("결과 읽기 ○"));
}

#[test]
fn explicit_browser_request_uses_small_model_fallback_before_inference() {
    let decision =
        conversation::decide_request("네이버를 열고 검색창에 월드컵을 입력해", &[], 4096, true)
            .unwrap();

    let conversation::RequestDecision::BrowserTool(request) = decision else {
        panic!("explicit browser request did not route to BrowserTool");
    };
    assert_eq!(request.url, "https://www.naver.com/");
    assert_eq!(request.query, "월드컵");
}

#[test]
fn ansi_conversation_distinguishes_failures_from_assistant_answers() {
    let mut state = InteractiveState::new();
    state.push_turn(ConversationRole::Assistant, "정상 답변");
    state.push_turn(ConversationRole::Error, "복구 가능한 오류");

    let frame = render_interactive_frame_with_options(
        &state,
        &TuiReadPage::conversation_placeholder(),
        &TuiStatusSnapshot::unavailable(),
        120,
        40,
        true,
        true,
    );

    assert!(frame.contains("\u{001b}[1;32m● \u{001b}[0m정상 답변"));
    assert!(frame.contains("\u{001b}[31m× \u{001b}[0m복구 가능한 오류"));
    assert!(!frame.contains("\u{001b}[1;32m× "));
}

#[test]
fn assistant_markdown_is_presented_without_literal_emphasis_markers() {
    let mut state = InteractiveState::new();
    state.push_turn(
        ConversationRole::Assistant,
        "## 비교\n* **Qwen**: `코딩`에 강함\n* **Gemma**: 대화에 강함",
    );

    let frame = render_interactive_frame_with_options(
        &state,
        &TuiReadPage::conversation_placeholder(),
        &TuiStatusSnapshot::unavailable(),
        120,
        40,
        true,
        true,
    );

    assert!(frame.contains("비교"));
    assert!(frame.contains("• "));
    assert!(frame.contains("\u{001b}[1mQwen\u{001b}[22m"));
    assert!(frame.contains("\u{001b}[36m코딩\u{001b}[0m"));
    assert!(!strip_ansi(&frame).contains("**"));
}

#[test]
fn controller_starts_fresh_until_a_session_is_explicitly_resumed() {
    let mut terminal = ScriptedTerminal::new(["/quit"]);
    let mut runtime = ConversationRuntime {
        history: vec![
            TuiConversationTurn {
                role: TuiConversationRole::User,
                content: "이전 질문".to_string(),
            },
            TuiConversationTurn {
                role: TuiConversationRole::Assistant,
                content: "이전 답변".to_string(),
            },
        ],
        ..ConversationRuntime::default()
    };

    run_controller(&mut terminal, &mut runtime).unwrap();

    assert_eq!(runtime.reconcile_backend_calls, 1);
    assert!(!terminal.frames[0].contains("이전 질문"));
    assert!(!terminal.frames[0].contains("이전 답변"));
    assert!(terminal.frames[0].contains("새 대화"));
}

#[test]
fn resume_command_uses_a_picker_and_rehydrates_only_the_selected_session() {
    let mut terminal = ScriptedTerminal::new(["/resume", "2", "/quit"]);
    let mut runtime = ConversationRuntime {
        history: vec![
            TuiConversationTurn {
                role: TuiConversationRole::User,
                content: "선택한 세션 질문".to_string(),
            },
            TuiConversationTurn {
                role: TuiConversationRole::Assistant,
                content: "선택한 세션 답변".to_string(),
            },
        ],
        session_options: vec![
            session_option("session-current", "현재 세션", true),
            session_option("session-older", "계산기 작업", false),
        ],
        ..ConversationRuntime::default()
    };

    run_controller(&mut terminal, &mut runtime).unwrap();

    assert_eq!(runtime.resumed_sessions, ["session-older"]);
    let rendered = terminal.frames.join("\n");
    assert!(rendered.contains("세션 재개"));
    assert!(rendered.contains("계산기 작업"));
    assert!(rendered.contains("선택한 세션 질문"));
    assert!(rendered.contains("선택한 세션 답변"));
}

#[test]
fn new_command_starts_an_empty_session_instead_of_clearing_old_history() {
    let mut terminal = ScriptedTerminal::new(["/new", "/quit"]);
    let mut runtime = ConversationRuntime {
        history: vec![TuiConversationTurn {
            role: TuiConversationRole::Assistant,
            content: "이전 세션 답변".to_string(),
        }],
        ..ConversationRuntime::default()
    };

    run_controller(&mut terminal, &mut runtime).unwrap();

    assert_eq!(runtime.new_session_calls, 1);
    assert_eq!(runtime.clear_history_calls, 0);
    assert!(!terminal.frames.last().unwrap().contains("이전 세션 답변"));
    assert!(terminal.frames.last().unwrap().contains("새 세션"));
}

#[test]
fn clear_command_clears_canonical_and_rendered_conversation() {
    let mut terminal = ScriptedTerminal::new(["/clear", "/quit"]);
    let mut runtime = ConversationRuntime {
        history: vec![
            TuiConversationTurn {
                role: TuiConversationRole::User,
                content: "지울 질문".to_string(),
            },
            TuiConversationTurn {
                role: TuiConversationRole::Assistant,
                content: "지울 답변".to_string(),
            },
        ],
        ..ConversationRuntime::default()
    };

    run_controller(&mut terminal, &mut runtime).unwrap();

    assert_eq!(runtime.clear_history_calls, 1);
    assert!(!terminal.frames.last().unwrap().contains("지울 답변"));
}

#[test]
fn search_command_routes_the_question_and_renders_the_answer() {
    let mut terminal = ScriptedTerminal::new(["/search Rust 공식 웹사이트는?", "/quit"]);
    let mut runtime = ConversationRuntime::default();

    run_controller(&mut terminal, &mut runtime).unwrap();

    let rendered = terminal.frames.join("\n");
    assert_eq!(runtime.requests, ["/search Rust 공식 웹사이트는?"]);
    assert!(rendered.contains("› /search Rust 공식 웹사이트는?"));
    assert!(rendered.contains("웹 조사 · 검색 중"));
    assert!(rendered.contains("검색 ● → 결과 평가 ○ → 문서 읽기 ○ → 증거 구성 ○ → 답변 ○"));
    assert!(rendered.contains("● 안녕하세요."));
}

#[test]
fn web_open_and_find_commands_route_through_the_conversation_runtime() {
    let mut terminal =
        ScriptedTerminal::new(["/open https://example.com/docs", "/find ownership", "/quit"]);
    let mut runtime = ConversationRuntime::default();

    run_controller(&mut terminal, &mut runtime).unwrap();

    assert_eq!(
        runtime.requests,
        ["/open https://example.com/docs", "/find ownership"]
    );
    let rendered = terminal.frames.join("\n");
    assert!(rendered.contains("페이지 여는 중"));
    assert!(rendered.contains("페이지 찾는 중"));
}

#[test]
fn sources_command_uses_a_picker_and_changes_the_current_document() {
    let mut terminal = ScriptedTerminal::new(["/sources", "2", "/quit"]);
    let mut runtime = ConversationRuntime {
        web_source_options: vec![
            web_source_option("source-one", "첫 문서", "https://example.com/one", true),
            web_source_option("source-two", "둘째 문서", "https://example.com/two", false),
        ],
        ..ConversationRuntime::default()
    };

    run_controller(&mut terminal, &mut runtime).unwrap();

    assert_eq!(runtime.selected_web_sources, ["source-two"]);
    assert!(terminal
        .frames
        .join("\n")
        .contains("현재 웹 출처를 변경했습니다: source-two"));
}

#[test]
fn interactive_web_open_keeps_page_available_for_followup_find() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-interactive-web-tools-test-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_TEST_SKIP_UPDATE_CHECK", "1");
    std::env::set_var(
        "RPOTATO_TEST_WEB_OPEN_HTML",
        "<html><title>Rust Guide</title><body>Ownership is a Rust feature.</body></html>",
    );
    std::fs::create_dir_all(root.join("project")).unwrap();
    crate::app::workflow_adapter::state::initialize().unwrap();
    let mut terminal = ScriptedTerminal::new([
        "/open https://example.com/guide",
        "/find ownership",
        "/quit",
    ]);

    run_controller(&mut terminal, &mut TuiRuntimeAdapter::default()).unwrap();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_TEST_SKIP_UPDATE_CHECK");
    std::env::remove_var("RPOTATO_TEST_WEB_OPEN_HTML");
    let _ = std::fs::remove_dir_all(root);
    let rendered = terminal.frames.join("\n");
    assert!(rendered.contains("Rust Guide"));
    assert!(rendered.contains("일치: 1개"));
    assert!(rendered.contains("Ownership is a Rust feature."));
}

#[test]
fn search_results_enter_the_source_picker_and_open_for_followup_find() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-search-source-picker-test-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_TEST_SKIP_UPDATE_CHECK", "1");
    std::env::set_var(
        "RPOTATO_TEST_WEB_SEARCH_HTML",
        include_str!("../../../tests/fixtures/web_search/ddg-html.html"),
    );
    std::env::set_var(
        "RPOTATO_TEST_WEB_OPEN_HTML",
        "<html><title>Selected Source</title><main>Verified checksum evidence.</main></html>",
    );
    std::fs::create_dir_all(root.join("project")).unwrap();
    crate::app::workflow_adapter::state::initialize().unwrap();
    let mut terminal = ScriptedTerminal::new([
        "/search Rust official release",
        "/sources",
        "1",
        "/find checksum",
        "/quit",
    ]);

    run_controller(&mut terminal, &mut TuiRuntimeAdapter::default()).unwrap();

    for name in [
        "RPOTATO_PROJECT_ROOT",
        "RPOTATO_DATA_HOME",
        "RPOTATO_TEST_SKIP_UPDATE_CHECK",
        "RPOTATO_TEST_WEB_SEARCH_HTML",
        "RPOTATO_TEST_WEB_OPEN_HTML",
    ] {
        std::env::remove_var(name);
    }
    let _ = std::fs::remove_dir_all(root);
    let rendered = terminal.frames.join("\n");
    assert!(rendered.contains("Selected Source"));
    assert!(rendered.contains("일치: 1개"));
    assert!(rendered.contains("Verified checksum evidence."));
}

#[test]
fn natural_requests_use_agent_progress_until_the_model_selects_a_tool() {
    let request = "2026년 월드컵 결과 검색해서 알려줘";
    let mut terminal = ScriptedTerminal::new([request, "/quit"]);
    let mut runtime = ConversationRuntime::default();

    run_controller(&mut terminal, &mut runtime).unwrap();

    assert_eq!(runtime.requests, [request]);
    assert!(terminal.frames[1].contains("처리 중"));
    assert!(terminal.frames[1].contains("에이전트가 요청을 처리하고 있습니다…"));
}

#[test]
fn slow_requests_refresh_a_spinner_and_live_context_estimate() {
    let mut terminal = ScriptedTerminal::new(["느린 요청", "/quit"]);
    let mut runtime = ConversationRuntime {
        submit_delay_ms: 280,
        context_estimate: Some(321),
        ..ConversationRuntime::default()
    };

    run_controller(&mut terminal, &mut runtime).unwrap();

    let rendered = terminal.frames.join("\n");
    assert!(
        terminal
            .frames
            .iter()
            .filter(|frame| frame.contains("처리 중"))
            .count()
            >= 2
    );
    assert!(rendered.contains("ctx ~321/"));
    assert!(rendered.contains("경과"));
}

#[test]
fn progress_frame_failure_after_dispatch_never_invites_request_replay() {
    let mut terminal = ScriptedTerminal::new(["느린 요청", "/quit"]);
    terminal.frame_fault_at = Some((
        crate::runtime_core::terminal::FrameWriteBoundary::PostDispatch,
        crate::runtime_core::terminal::TerminalFault::FrameWrite,
    ));
    let mut runtime = ConversationRuntime {
        submit_delay_ms: 280,
        ..ConversationRuntime::default()
    };

    let error = run_controller(&mut terminal, &mut runtime).unwrap_err();

    assert_eq!(runtime.requests, ["느린 요청"]);
    assert!(error.message.contains("terminal.frame-write.post-dispatch"));
    assert!(error.message.contains("요청을 다시 보내지 않습니다"));
    assert!(!error.message.contains("런타임 요청을 보내지 않았습니다"));
}

#[test]
fn model_command_uses_keyboard_choices_and_applies_the_selection() {
    let mut terminal = ScriptedTerminal::new(["/model", "2", "2", "/quit"]);
    let mut runtime = ConversationRuntime {
        model_options: vec![
            model_option("small", "Small", true, false),
            model_option("recommended", "Recommended", false, true),
        ],
        ..ConversationRuntime::default()
    };

    run_controller(&mut terminal, &mut runtime).unwrap();

    assert_eq!(runtime.setup_models, ["recommended"]);
    let rendered = terminal.frames.join("\n");
    assert!(rendered.contains("모델 선택"));
    assert!(rendered.contains("Recommended"));
    assert!(rendered.contains("모델 변경 확인"));
    assert!(rendered.contains("모델 적용 완료: recommended"));
}

#[test]
fn model_confirmation_defaults_to_cancel_without_applying_the_selection() {
    let mut terminal = ScriptedTerminal::new(["/model", "2", "1", "/quit"]);
    let mut runtime = ConversationRuntime {
        model_options: vec![
            model_option("small", "Small", true, false),
            model_option("recommended", "Recommended", false, true),
        ],
        ..ConversationRuntime::default()
    };

    run_controller(&mut terminal, &mut runtime).unwrap();

    assert!(runtime.setup_models.is_empty());
    let rendered = terminal.frames.join("\n");
    assert!(rendered.contains("모델 변경 확인"));
    assert!(rendered.contains("1. 취소"));
    assert!(rendered.contains("2. 다운로드하고 적용"));
    assert!(rendered.contains("모델 변경을 취소했습니다."));
}

#[test]
fn cached_model_switch_is_labeled_as_reuse_instead_of_a_new_download() {
    let mut cached = model_option("cached", "Cached", false, true);
    cached.model_cached = true;
    cached.vision_projector_cached = true;
    let mut terminal = ScriptedTerminal::new(["/model", "2", "2", "/quit"]);
    let mut runtime = ConversationRuntime {
        model_options: vec![model_option("current", "Current", true, false), cached],
        ..ConversationRuntime::default()
    };

    run_controller(&mut terminal, &mut runtime).unwrap();

    assert_eq!(runtime.setup_models, ["cached"]);
    let rendered = terminal.frames.join("\n");
    assert!(rendered.contains("local cache"));
    assert!(rendered.contains("기존 모델로 전환"));
    assert!(rendered.contains("기존 모델 cache/SHA-256 검증"));
    assert!(!rendered.contains("Cached · download"));
}

#[test]
fn update_confirmation_defaults_to_cancel_without_calling_the_updater() {
    let mut terminal = ScriptedTerminal::new(["/update", "1", "/quit"]);
    let mut runtime = ConversationRuntime::default();

    run_controller(&mut terminal, &mut runtime).unwrap();

    assert_eq!(runtime.update_calls, 0);
    let rendered = terminal.frames.join("\n");
    assert!(rendered.contains("업데이트 확인"));
    assert!(rendered.contains("1. 취소"));
    assert!(rendered.contains("2. 업데이트 시작"));
    assert!(rendered.contains("업데이트를 취소했습니다."));
    assert!(!rendered.contains("yes를 입력"));
}

#[test]
fn pasted_image_path_becomes_an_attachment_instead_of_an_unknown_command() {
    let path = "/private/tmp/rpotato-screen.png";
    let mut terminal = ScriptedTerminal::new([path, "이 이미지 봐줘", "/quit"]);
    let mut runtime = ConversationRuntime::default();

    run_controller(&mut terminal, &mut runtime).unwrap();

    assert_eq!(runtime.captured_paths, [path]);
    assert_eq!(runtime.requests, ["이 이미지 봐줘"]);
    assert_eq!(runtime.submitted_attachment_counts, [1]);
    let rendered = terminal.frames.join("\n");
    assert!(rendered.contains("[image:"));
    assert!(!rendered.contains("알 수 없는 TUI 명령"));
}

#[test]
fn failed_request_keeps_attachments_until_a_successful_retry() {
    let path = "/private/tmp/rpotato-retry.png";
    let mut terminal =
        ScriptedTerminal::new([path, "첫 요청", "재시도", "첨부 없는 요청", "/quit"]);
    let mut runtime = ConversationRuntime {
        submit_failures_remaining: 1,
        ..ConversationRuntime::default()
    };

    run_controller(&mut terminal, &mut runtime).unwrap();

    assert_eq!(runtime.requests, ["첫 요청", "재시도", "첨부 없는 요청"]);
    assert_eq!(runtime.submitted_attachment_counts, [1, 1, 0]);
    assert!(terminal
        .frames
        .join("\n")
        .contains("첨부는 재시도를 위해 유지했습니다."));
}

#[test]
fn failed_request_is_rendered_as_an_error_instead_of_a_green_assistant_turn() {
    let mut terminal = ScriptedTerminal::new(["실패 요청", "/quit"]);
    let mut runtime = ConversationRuntime {
        submit_failures_remaining: 1,
        ..ConversationRuntime::default()
    };

    run_controller(&mut terminal, &mut runtime).unwrap();

    let rendered = terminal.frames.join("\n");
    assert!(rendered.contains("× 요청을 완료하지 못했습니다."));
    assert!(!rendered.contains("● 요청을 완료하지 못했습니다."));
}

#[test]
fn wide_terminal_uses_the_full_viewport_for_chrome_and_a_bounded_reading_column() {
    let mut state = InteractiveState::new();
    state.push_turn(
        ConversationRole::Assistant,
        "긴 답변은 읽기 좋은 폭으로 유지하지만 입력창은 터미널 전체를 사용합니다.",
    );

    let frame = render_interactive_frame_with_options(
        &state,
        &TuiReadPage::conversation_placeholder(),
        &TuiStatusSnapshot::unavailable(),
        160,
        40,
        true,
        true,
    );
    let visible = strip_ansi(&frame);
    let composer = visible
        .lines()
        .find(|line| line.contains("─ 요청 "))
        .expect("composer top rule");
    assert_eq!(display_cell_width(composer), 160);
    let transcript = visible.split("╭─ 요청").next().expect("transcript");
    assert!(transcript
        .lines()
        .filter(|line| line.starts_with("● ") || line.starts_with("│ "))
        .all(|line| display_cell_width(line) <= 120));
}

#[test]
fn conversation_frame_sanitizes_project_path_and_respects_terminal_cell_width() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let previous_root = std::env::var_os("RPOTATO_PROJECT_ROOT");
    std::env::set_var("RPOTATO_PROJECT_ROOT", "/\u{001b}[31m위험\n프로젝트");
    let mut state = InteractiveState::new();
    state.push_turn(
        ConversationRole::Assistant,
        "한국어 응답이 좁은 터미널에서도 입력창을 밀어내지 않습니다.",
    );
    state.notice =
        "웹 조사 · 검색 중\n검색 ● → 결과 평가 ○ → 문서 읽기 ○ → 증거 구성 ○ → 답변 ○".to_string();

    let frame = render_interactive_frame(&state, &TuiReadPage::conversation_placeholder(), 40, 12);

    if let Some(previous_root) = previous_root {
        std::env::set_var("RPOTATO_PROJECT_ROOT", previous_root);
    } else {
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
    }
    assert!(!frame.contains('\u{001b}'));
    assert!(frame.contains("<esc>"));
    assert!(!frame.contains("\n프로젝트"));
    assert!(
        frame.lines().all(|line| display_cell_width(line) <= 40),
        "narrow frame exceeded terminal cell width:\n{frame}"
    );
    assert!(frame.ends_with("› "));
}

#[derive(Default)]
struct ConversationRuntime {
    history: Vec<TuiConversationTurn>,
    reconcile_backend_calls: usize,
    clear_history_calls: usize,
    requests: Vec<String>,
    page_reads: usize,
    update_calls: usize,
    model_options: Vec<TuiModelOption>,
    setup_models: Vec<String>,
    captured_paths: Vec<String>,
    submitted_attachment_counts: Vec<usize>,
    submit_failures_remaining: usize,
    session_options: Vec<TuiSessionOption>,
    resumed_sessions: Vec<String>,
    new_session_calls: usize,
    web_source_options: Vec<TuiWebSourceOption>,
    selected_web_sources: Vec<String>,
    progress_hint: Option<String>,
    submit_delay_ms: u64,
    context_estimate: Option<u32>,
}

impl TuiRuntimePort for ConversationRuntime {
    fn startup_update_notice(&mut self) -> Option<String> {
        None
    }

    fn reconcile_existing_backend(&mut self) -> Result<(), AppError> {
        self.reconcile_backend_calls += 1;
        Ok(())
    }

    fn clear_conversation_history(&mut self) -> Result<(), AppError> {
        self.clear_history_calls += 1;
        self.history.clear();
        Ok(())
    }

    fn apply_update(&mut self) -> Result<String, AppError> {
        self.update_calls += 1;
        Ok("업데이트 완료".to_string())
    }

    fn read_tui_page(&mut self, _request: TuiReadRequest) -> Result<TuiReadPage, AppError> {
        self.page_reads += 1;
        Ok(TuiReadPage {
            title: "overview".to_string(),
            lines: vec!["ledger: must stay hidden".to_string()],
            page: 0,
            has_previous: false,
            has_next: false,
            freshness: TuiFreshness::Fresh,
            continuation: TuiReadContinuation::Complete,
            authority: crate::surfaces::tui::runtime_bridge::TuiReadAuthority::default(),
        })
    }

    fn read_tui_status(&mut self) -> Result<TuiStatusSnapshot, AppError> {
        Ok(TuiStatusSnapshot::unavailable())
    }

    fn model_options(&mut self) -> Vec<TuiModelOption> {
        self.model_options.clone()
    }

    fn session_options(&mut self) -> Result<Vec<TuiSessionOption>, AppError> {
        Ok(self.session_options.clone())
    }

    fn web_source_options(&mut self) -> Vec<TuiWebSourceOption> {
        self.web_source_options.clone()
    }

    fn select_web_source(&mut self, source_id: &str) -> Result<String, AppError> {
        self.selected_web_sources.push(source_id.to_string());
        Ok(format!("현재 웹 출처를 변경했습니다: {source_id}"))
    }

    fn start_new_session(&mut self) -> Result<TuiSessionTransition, AppError> {
        self.new_session_calls += 1;
        self.history.clear();
        Ok(TuiSessionTransition {
            session_id: "session-new".to_string(),
            notice: "새 세션을 시작했습니다.".to_string(),
            turns: Vec::new(),
        })
    }

    fn resume_session(&mut self, session_id: &str) -> Result<TuiSessionTransition, AppError> {
        self.resumed_sessions.push(session_id.to_string());
        Ok(TuiSessionTransition {
            session_id: session_id.to_string(),
            notice: "세션을 재개했습니다.".to_string(),
            turns: self.history.clone(),
        })
    }

    fn setup_model(&mut self, id: &str) -> Result<String, AppError> {
        self.setup_models.push(id.to_string());
        Ok(format!("모델 적용 완료: {id}"))
    }

    fn doctor_report(&mut self) -> String {
        String::new()
    }

    fn compact_context(&mut self) -> Result<String, AppError> {
        unreachable!()
    }

    fn capture_attachment(&mut self, path: &str) -> Result<TuiAttachment, AppError> {
        self.captured_paths.push(path.to_string());
        Ok(TuiAttachment {
            id: "attachment-test".to_string(),
            display_name: path.to_string(),
            stored_path: path.to_string(),
            size_bytes: 1,
            kind: TuiAttachmentKind::Image,
        })
    }

    fn request_progress_hint(&mut self, _request: &str) -> Option<String> {
        self.progress_hint.clone()
    }

    fn request_context_tokens_hint(
        &mut self,
        _request: &str,
        _attachments: &[TuiAttachment],
    ) -> Option<u32> {
        self.context_estimate
    }

    fn submit_request(
        &mut self,
        request: &str,
        attachments: &[TuiAttachment],
    ) -> Result<String, AppError> {
        if self.submit_delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(self.submit_delay_ms));
        }
        self.requests.push(request.to_string());
        self.submitted_attachment_counts.push(attachments.len());
        if self.submit_failures_remaining > 0 {
            self.submit_failures_remaining -= 1;
            return Err(AppError::runtime("테스트 요청 실패"));
        }
        Ok("안녕하세요.".to_string())
    }

    fn new_tui_intent_id(&mut self) -> String {
        "intent-test".to_string()
    }

    fn tui_selection_lease(
        &mut self,
        _selected_object_id: &str,
    ) -> Result<SelectionLease, AppError> {
        unreachable!()
    }

    fn tui_gate_descriptor(
        &mut self,
        _workflow_id: &str,
    ) -> Result<(String, TuiGateKind), AppError> {
        unreachable!()
    }

    fn dispatch_tui_intent(&mut self, _intent: TuiIntent) -> Result<TuiOutcome, AppError> {
        unreachable!()
    }
}

fn model_option(id: &str, display_name: &str, current: bool, recommended: bool) -> TuiModelOption {
    TuiModelOption {
        id: id.to_string(),
        display_name: display_name.to_string(),
        quantization: "Q4".to_string(),
        download_bytes: 1024,
        model_cached: false,
        vision_projector_bytes: Some(512),
        vision_projector_cached: false,
        context_length: Some(4096),
        ram: "4 GiB".to_string(),
        license: "Apache-2.0".to_string(),
        note: "test model".to_string(),
        current,
        recommended,
    }
}

fn session_option(session_id: &str, preview: &str, current: bool) -> TuiSessionOption {
    TuiSessionOption {
        session_id: session_id.to_string(),
        preview: preview.to_string(),
        current,
    }
}

fn web_source_option(source_id: &str, title: &str, url: &str, current: bool) -> TuiWebSourceOption {
    TuiWebSourceOption {
        source_id: source_id.to_string(),
        title: title.to_string(),
        url: url.to_string(),
        opened: current,
        current,
    }
}

fn strip_ansi(value: &str) -> String {
    let mut output = String::new();
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\u{001b}' {
            output.push(ch);
            continue;
        }
        if chars.next_if_eq(&'[').is_none() {
            continue;
        }
        for next in chars.by_ref() {
            if ('@'..='~').contains(&next) {
                break;
            }
        }
    }
    output
}
