//! Application wiring boundary.
//!
//! Startup, command dispatch, and inference orchestration live here while
//! application adapters, domain behavior, and infrastructure stay with their owners.

pub(crate) mod config;
pub(crate) mod dispatch;
pub(crate) mod inference;
pub(crate) mod startup;
pub(crate) mod tui_action;
pub(crate) mod tui_read;
pub(crate) mod uninstall;
