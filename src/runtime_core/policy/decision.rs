//! Surface-neutral policy decision facade.

mod command;
mod path_policy;
mod schema;
mod types;

pub use command::{classify_command, parse_patch_verification};
pub(crate) use path_policy::classify_path;
pub use schema::schema_report;
#[cfg(test)]
pub(crate) use types::ActionKind;
pub(crate) use types::PathPolicyPort;
pub use types::{Decision, PathMode, PolicyDecision};
