//! Interactive TUI runtime composition.

mod backend;
mod model_setup;
mod port;
mod request;
#[cfg(test)]
mod session_tests;
mod state;
mod status;
mod web_sources;

pub(super) use state::TuiRuntimeAdapter;
