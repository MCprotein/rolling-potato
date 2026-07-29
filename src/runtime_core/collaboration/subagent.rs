//! Subagent domain facade.

pub(crate) use crate::runtime_core::inference::backend::MAX_CHAT_TIMEOUT_MS;

mod launch;
mod record;
mod record_codec;
mod record_validation;
mod types;

pub(crate) use launch::normalize_relative_path;
pub use launch::validate_launch;
pub(crate) use record::{create_record_at, NewRecordBinding};
pub(crate) use record_codec::{parse_record, render_payload, render_record};
pub(crate) use record_validation::{
    immutable_binding_changed, is_sha256, validate_record, validate_subagent_id,
};
pub(crate) use types::MAX_RECORD_REVISIONS;
pub use types::{SubagentRecordV1, SubagentRole, SubagentStatus, ValidatedLaunch};
#[cfg(test)]
pub use types::{DEFAULT_MAX_TOKENS, DEFAULT_TIMEOUT_MS, MAX_MAX_TOKENS, MAX_TASK_BYTES};
