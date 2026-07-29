mod fallback;
mod session;
mod types;

#[cfg(test)]
pub(crate) use fallback::deterministic_freshness_fallback;
pub(crate) use fallback::deterministic_freshness_fallback_for_context;
pub(crate) use session::WebResearchSession;
#[cfg(test)]
pub(crate) use types::{FailedInputAction, WebResearchLimit, WebResearchTerminal};
pub(crate) use types::{WebResearchAdmission, WebResearchBudget, WebResearchStep};

#[cfg(test)]
#[path = "research/tests.rs"]
mod tests;
