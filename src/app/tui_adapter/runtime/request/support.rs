mod context;
pub(super) mod local_loop_state;
mod web_execution;

pub(super) use context::{required_context_limit, web_conversation_context};
pub(super) use web_execution::{execute_web_turn, plain_execution};

#[cfg(test)]
#[path = "support/tests.rs"]
mod tests;
