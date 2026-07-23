use super::*;
use crate::adapters::terminal::native::ScriptedTerminal;
use crate::foundation::error::AppError;
use crate::surfaces::tui::controller::{run_controller, TuiRuntimePort};
use crate::surfaces::tui::render::{display_cell_width, render_interactive_frame};
use crate::surfaces::tui::runtime_bridge::{
    SelectionLease, TuiAttachment, TuiAttachmentKind, TuiFreshness, TuiGateKind, TuiIntent,
    TuiModelOption, TuiReadContinuation, TuiReadPage, TuiReadRequest, TuiStatusSnapshot,
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
    assert!(terminal.frames[1].contains("◇ 작업 중"));
    assert!(!terminal.frames[1].contains("notice:"));
    assert!(rendered.contains("› 안녕"));
    assert!(rendered.contains("● 안녕하세요."));
    assert!(!rendered.contains("ledger: must stay hidden"));
    assert!(!rendered.contains("patch proposal"));
}

#[test]
fn search_command_routes_the_question_and_renders_the_answer() {
    let mut terminal = ScriptedTerminal::new(["/search Rust 공식 웹사이트는?", "/quit"]);
    let mut runtime = ConversationRuntime::default();

    run_controller(&mut terminal, &mut runtime).unwrap();

    let rendered = terminal.frames.join("\n");
    assert_eq!(runtime.requests, ["/search Rust 공식 웹사이트는?"]);
    assert!(rendered.contains("› /search Rust 공식 웹사이트는?"));
    assert!(rendered.contains("검색 중 · 최신 웹 자료를 확인하고 있습니다…"));
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
fn natural_requests_use_agent_progress_until_the_model_selects_a_tool() {
    let request = "2026년 월드컵 결과 검색해서 알려줘";
    let mut terminal = ScriptedTerminal::new([request, "/quit"]);
    let mut runtime = ConversationRuntime::default();

    run_controller(&mut terminal, &mut runtime).unwrap();

    assert_eq!(runtime.requests, [request]);
    assert!(terminal.frames[1].contains("작업 중 · 에이전트가 요청을 처리하고 있습니다…"));
}

#[test]
fn model_command_uses_keyboard_choices_and_applies_the_selection() {
    let mut terminal = ScriptedTerminal::new(["/model", "2", "1", "/quit"]);
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
fn conversation_frame_sanitizes_project_path_and_respects_terminal_cell_width() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let previous_root = std::env::var_os("RPOTATO_PROJECT_ROOT");
    std::env::set_var("RPOTATO_PROJECT_ROOT", "/\u{001b}[31m위험\n프로젝트");
    let mut state = InteractiveState::new();
    state.push_turn(
        ConversationRole::Assistant,
        "한국어 응답이 좁은 터미널에서도 입력창을 밀어내지 않습니다.",
    );

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
    requests: Vec<String>,
    page_reads: usize,
    model_options: Vec<TuiModelOption>,
    setup_models: Vec<String>,
    captured_paths: Vec<String>,
    submitted_attachment_counts: Vec<usize>,
}

impl TuiRuntimePort for ConversationRuntime {
    fn startup_update_notice(&mut self) -> Option<String> {
        None
    }

    fn apply_update(&mut self) -> Result<String, AppError> {
        unreachable!()
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

    fn submit_request(
        &mut self,
        request: &str,
        attachments: &[TuiAttachment],
    ) -> Result<String, AppError> {
        self.requests.push(request.to_string());
        self.submitted_attachment_counts.push(attachments.len());
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
        context_length: Some(4096),
        ram: "4 GiB".to_string(),
        license: "Apache-2.0".to_string(),
        note: "test model".to_string(),
        current,
        recommended,
    }
}
