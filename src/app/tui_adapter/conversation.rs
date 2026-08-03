//! Non-mutating conversation path for general questions that do not need agent tools.

mod decision;
mod local_facts;
mod presentation;
mod prompt_policy;
mod reply;

pub(super) use decision::{
    decide_request_with_cancel, decide_web_observation_with_cancel, RequestDecision,
    WebObservationDecisionContext,
};
pub(super) use local_facts::{is_conversational_request, local_reply};
pub(super) use presentation::{ensure_public_answer, present_agent_report};
pub(super) use reply::{
    estimate_context_tokens, render_web_conversation_context, reply_with_context_and_cancel,
    reply_with_images_and_cancel,
};

#[cfg(test)]
pub(super) use decision::decide_request;
#[cfg(test)]
use decision::{
    decide_generated_candidate, recent_user_requests, request_decision_from_agent_tool,
    request_decision_from_observation_tool, structured_tool_call,
};
#[cfg(test)]
use presentation::contains_private_tool_protocol;

#[cfg(test)]
mod tests;
