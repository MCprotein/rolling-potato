//! Application wiring boundary.
//!
//! Production startup and dependency construction stay in the legacy modules
//! until their scheduled migration. This private root only reserves ownership.

pub(crate) mod config;
pub(crate) mod startup;
pub(crate) mod uninstall;
