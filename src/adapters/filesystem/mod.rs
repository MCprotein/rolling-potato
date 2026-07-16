//! Filesystem layout, atomic replacement, cache, and lease implementations.

pub(crate) mod cache;
pub(crate) mod config;
pub(crate) mod layout;
pub(crate) mod lease;
pub(crate) mod model_artifact;
#[cfg(windows)]
pub(crate) mod windows_replace;
