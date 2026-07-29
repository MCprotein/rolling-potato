use super::*;
use crate::adapters::filesystem::layout as paths;
use crate::adapters::terminal::native::{ScriptedTerminal, TerminalFault};
use crate::surfaces::tui::controller::{consume_outcome, run_controller};
use crate::surfaces::tui::outcome::verification_credential_issued;
use crate::surfaces::tui::render::{
    conversation_page_count, display_cell_width, render_interactive_frame,
    render_interactive_frame_with_options, sanitize_terminal_text,
};
use crate::surfaces::tui::runtime_bridge::{
    OneShotSecret, TuiFreshness, TuiReadBudget, TuiReadContinuation,
};
use crate::surfaces::tui::runtime_bridge::{TuiBackendStatus, TuiStatusSnapshot};
use crate::surfaces::tui::view_model::{ConversationRole, InteractiveState, InteractiveView};

include!("tests/controller.rs");
include!("tests/outcome.rs");
include!("tests/render.rs");
include!("tests/reports.rs");
include!("tests/view_state.rs");

fn test_root(name: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("{name}-{}-{nanos}", std::process::id()))
}
