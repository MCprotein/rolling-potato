//! Filesystem artifact adapter for verified subagent results.

#[path = "subagent_result/storage.rs"]
mod storage;
#[path = "subagent_result/types.rs"]
mod types;
#[path = "subagent_result/validation.rs"]
mod validation;
#[path = "subagent_result/verification.rs"]
mod verification;

pub(crate) use crate::runtime_core::collaboration::subagent_result::SubagentResultV1;
pub use storage::parse_and_store;
#[allow(unused_imports)]
pub use types::StoredSubagentResult;
pub use verification::{
    load_completed_result, verify_completed_artifacts, verify_completed_source_freshness,
    verify_stored_artifacts,
};

#[cfg(test)]
#[path = "subagent_result/tests.rs"]
mod tests;
