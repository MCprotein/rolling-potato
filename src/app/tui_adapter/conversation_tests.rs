use super::*;
use crate::adapters::terminal::native::ScriptedTerminal;
use crate::foundation::error::AppError;
use crate::surfaces::tui::controller::{run_controller, TuiRuntimePort};
use crate::surfaces::tui::render::{display_cell_width, render_interactive_frame};
use crate::surfaces::tui::runtime_bridge::{
    SelectionLease, TuiFreshness, TuiGateKind, TuiIntent, TuiModelOption, TuiReadContinuation,
    TuiReadPage, TuiReadRequest, TuiStatusSnapshot,
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
        Vec::new()
    }

    fn setup_model(&mut self, _id: &str) -> Result<String, AppError> {
        unreachable!()
    }

    fn doctor_report(&mut self) -> String {
        String::new()
    }

    fn compact_context(&mut self) -> Result<String, AppError> {
        unreachable!()
    }

    fn submit_request(&mut self, request: &str) -> Result<String, AppError> {
        self.requests.push(request.to_string());
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
