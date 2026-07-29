use crate::native_terminal_support::{self, tree_snapshot, NativeTerminalFixture};

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
use crate::native_terminal_support::NativePty;

use crate::native_terminal_support::trace_stage;

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn uses_live_terminal_controls() -> bool {
    std::env::var_os("NO_COLOR").is_none()
        && std::env::var_os("TERM").as_deref() != Some(std::ffi::OsStr::new("dumb"))
}

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn confirm_picker(terminal: &mut NativePty, title: &str) {
    terminal.wait_for(title);
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    if uses_live_terminal_controls() {
        terminal.send("2");
    } else {
        terminal.send("2\n");
    }
    #[cfg(windows)]
    terminal.send("2\n");
}

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn submit_visible_command(terminal: &mut NativePty, command: &str) {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let mark = terminal.mark();
        if uses_live_terminal_controls() {
            terminal.send(&format!("\u{1b}[200~{command}\u{1b}[201~"));
        } else {
            terminal.send(command);
        }
        terminal.wait_for_after(mark, command);
        terminal.send("\r");
    }
    #[cfg(windows)]
    terminal.send(&format!("{command}\n"));
}

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn select_workflow(terminal: &mut NativePty, workflow_id: &str) {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let mark = terminal.mark();
    submit_visible_command(terminal, &format!("select {workflow_id}"));
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    terminal.wait_for_ordered_after(
        mark,
        &format!("선택: {workflow_id}"),
        if uses_live_terminal_controls() {
            "\u{1b}[?2004h"
        } else {
            "›"
        },
    );
    #[cfg(windows)]
    terminal.wait_for(&format!("선택: {workflow_id}"));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
struct LiveTerminalEnvironment {
    no_color: Option<std::ffi::OsString>,
    term: Option<std::ffi::OsString>,
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
impl LiveTerminalEnvironment {
    fn enable() -> Self {
        let environment = Self {
            no_color: std::env::var_os("NO_COLOR"),
            term: std::env::var_os("TERM"),
        };
        std::env::remove_var("NO_COLOR");
        std::env::set_var("TERM", "xterm-256color");
        environment
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
impl Drop for LiveTerminalEnvironment {
    fn drop(&mut self) {
        match self.no_color.take() {
            Some(value) => std::env::set_var("NO_COLOR", value),
            None => std::env::remove_var("NO_COLOR"),
        }
        match self.term.take() {
            Some(value) => std::env::set_var("TERM", value),
            None => std::env::remove_var("TERM"),
        }
    }
}

#[path = "native_terminal/adapter_matrix.rs"]
mod adapter_matrix;
#[path = "native_terminal/interaction.rs"]
mod interaction;
#[path = "native_terminal/lifecycle.rs"]
mod lifecycle;
#[path = "native_terminal/web.rs"]
mod web;

use adapter_matrix::{assert_clean_restart, assert_tree_unchanged};
