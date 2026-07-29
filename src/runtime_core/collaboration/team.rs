//! Team admission, dispatch, continuation, and governor decision facade.

mod admission;
mod dispatch;
mod events;
mod governor;
mod ownership;
mod policy;
mod types;

pub(crate) use admission::{admission_event_type, admission_summary, overall_status};
pub(crate) use dispatch::{
    continuation_decision, dispatch_event_type, dispatch_status, dispatch_summary,
};
pub(crate) use events::is_team_runtime_event;
pub(crate) use governor::{
    governor_event_type, governor_status, governor_summary, pressure_from_status,
};
pub(crate) use ownership::evaluate_ownership_gate;
pub(crate) use policy::{decision_label, evaluate_policy_gate, policy_write_paths};
pub(crate) use types::{
    ContinuationDecision, OwnershipCheck, OwnershipClaim, OwnershipGate, PolicyCheck, PolicyGate,
};
