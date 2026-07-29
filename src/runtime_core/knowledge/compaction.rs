//! Bounded context compaction contracts for small local models.

mod artifact;
mod checkpoint;
mod policy;
mod recent_tail;
mod token_budget;

pub(crate) use artifact::{
    parse_artifact, render_artifact, render_artifact_payload, CompactionArtifact,
    COMPACTION_SCHEMA_VERSION,
};
pub(crate) use checkpoint::CompactionCheckpoint;
#[allow(unused_imports)]
pub(crate) use policy::CompactionPlan;
pub(crate) use policy::{CompactionMode, CompactionPolicy, CompactionRecord};
pub(crate) use token_budget::{
    estimate_tokens, truncate_head_to_tokens, truncate_tail_to_estimated_tokens,
};

#[cfg(test)]
mod tests;
