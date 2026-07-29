//! Non-mutating conversation path for general questions that do not need agent tools.

mod decision;
mod local_facts;
mod presentation;
mod reply;

pub(super) use decision::{decide_request, RequestDecision};
pub(super) use local_facts::{is_conversational_request, local_reply};
pub(super) use presentation::{ensure_public_answer, present_agent_report};
pub(super) use reply::{
    estimate_context_tokens, render_web_conversation_context, reply_with_context, reply_with_images,
};

#[cfg(test)]
use decision::{
    decide_generated_candidate, recent_user_requests, request_decision_from_agent_tool,
    structured_tool_call,
};
#[cfg(test)]
use presentation::contains_private_tool_protocol;

#[cfg(test)]
mod tests;
