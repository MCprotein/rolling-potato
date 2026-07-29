//! Facade for ontology, declared-file, and session context assembly.

mod compaction;
mod declared_context;
mod discovery;
mod ontology_context;
mod resume_context;

pub use crate::runtime_core::knowledge::context::{
    enforce_shared_source_budget, ContextPack, ResumeContext, SourcePointer,
};
pub(crate) use compaction::{compact_automatically, compact_manually};
pub use declared_context::{build_declared_context_pack, verify_declared_context_pack};
pub use ontology_context::build_context_pack;
#[cfg(test)]
pub(crate) use resume_context::build_active_conversation_context_for_limit;
#[cfg(test)]
pub(crate) use resume_context::rebuild_resume_context_for_limit;
pub use resume_context::{build_active_conversation_context, rebuild_resume_context};

#[cfg(test)]
#[path = "context_adapter/tests.rs"]
mod tests;
