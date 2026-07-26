//! Volatile platform and infrastructure implementations.

#[allow(dead_code)] // B1 boundary is consumed by the B2 action owner in the next delivery unit.
pub(crate) mod browser;
pub(crate) mod filesystem;
pub(crate) mod github_release;
pub(crate) mod llama_cpp;
pub(crate) mod process;
pub(crate) mod sqlite;
pub(crate) mod system_install;
pub(crate) mod terminal;
pub(crate) mod web_search;
