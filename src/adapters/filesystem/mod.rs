//! Filesystem layout, atomic replacement, cache, and lease implementations.

pub(crate) mod atomic_write;
pub(crate) mod backend_state;
pub(crate) mod benchmark_artifact;
pub(crate) mod cache;
pub(crate) mod config;
pub(crate) mod layout;
pub(crate) mod lease;
pub(crate) mod model_artifact;
pub(crate) mod uninstall;
#[cfg(windows)]
pub(crate) mod windows_replace;
