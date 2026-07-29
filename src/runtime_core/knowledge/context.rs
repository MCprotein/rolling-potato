//! Bounded, surface-neutral source and resume context.

#[path = "context/assembly.rs"]
mod assembly;
#[path = "context/budget.rs"]
mod budget;
#[path = "context/resume.rs"]
mod resume;
#[path = "context/sources.rs"]
mod sources;
#[path = "context/types.rs"]
mod types;

pub(crate) use assembly::assemble_agent_prompt;
pub(crate) use budget::{AgentPromptBudget, ResumeContextBudget};
pub use sources::enforce_shared_source_budget;
pub(crate) use sources::{
    truncate_chars, MAX_CONTEXT_CHARS, MAX_CONTEXT_FILES, MAX_FILE_BYTES, MAX_FILE_CHARS,
};
pub(crate) use types::AgentPromptParts;
pub use types::{ContextPack, ResumeContext, SourcePointer};

#[cfg(test)]
#[path = "context/tests.rs"]
mod tests;
