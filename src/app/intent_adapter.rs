//! Intent classification and agent-loop application adapter.

mod context_requirements;
mod execution;
mod lifecycle;
mod outcomes;
mod prompt;
mod reporting;
mod routing;

#[allow(unused_imports)]
pub use crate::runtime_core::patch::intent::IntentDecision;
#[allow(unused_imports)]
pub use routing::{classify, classify_report, routes_report, run_report, run_skill_report};

#[cfg(test)]
use crate::app::context_adapter::{ContextPack, ResumeContext};
#[cfg(test)]
use crate::app::extensions_adapter::skill;
#[cfg(test)]
use crate::app::workflow_adapter::state;
#[cfg(test)]
use crate::foundation::error::AppError;
#[cfg(test)]
use crate::runtime_core::knowledge::context::AgentPromptBudget;
use context_requirements::available_context_labels;
use lifecycle::{dispatch_skill_hook, fail_skill_workflow, plugin_completion_fault};
use outcomes::record_non_mutating_outcomes;
use prompt::agent_loop_prompt;
#[cfg(test)]
use prompt::agent_loop_prompt_for_context;
#[cfg(test)]
use reporting::model_answer;
use reporting::{is_non_mutating_action, model_transcript_content, render_non_mutating_report};

#[cfg(test)]
#[path = "intent_adapter/tests.rs"]
mod tests;
