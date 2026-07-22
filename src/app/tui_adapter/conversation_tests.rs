use super::*;
use crate::adapters::terminal::native::ScriptedTerminal;
use crate::foundation::error::AppError;
use crate::surfaces::tui::controller::{run_controller, TuiRuntimePort};
use crate::surfaces::tui::render::render_interactive_frame;
use crate::surfaces::tui::runtime_bridge::{
    SelectionLease, TuiFreshness, TuiGateKind, TuiIntent, TuiModelOption, TuiReadContinuation,
    TuiReadPage, TuiReadRequest, TuiStatusSnapshot,
};
use crate::surfaces::tui::view_model::InteractiveState;

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

    assert!(frame.contains("rpotato"));
    assert!(frame.contains("로컬 코딩 에이전트"));
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
    assert!(rendered.contains("› 안녕"));
    assert!(rendered.contains("● 안녕하세요."));
    assert!(!rendered.contains("ledger: must stay hidden"));
    assert!(!rendered.contains("patch proposal"));
}

#[derive(Default)]
struct ConversationRuntime {
    requests: Vec<String>,
}

impl TuiRuntimePort for ConversationRuntime {
    fn startup_update_notice(&mut self) -> Option<String> {
        None
    }

    fn apply_update(&mut self) -> Result<String, AppError> {
        unreachable!()
    }

    fn read_tui_page(&mut self, _request: TuiReadRequest) -> Result<TuiReadPage, AppError> {
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
