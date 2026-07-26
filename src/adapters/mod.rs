//! Volatile platform and infrastructure implementations.

#[allow(dead_code)] // B2 boundary is connected to the TUI by the B3 delivery unit.
pub(crate) mod browser;
pub(crate) mod filesystem;
pub(crate) mod github_release;
pub(crate) mod llama_cpp;
pub(crate) mod process;
pub(crate) mod sqlite;
pub(crate) mod system_install;
pub(crate) mod terminal;
pub(crate) mod web_search;
