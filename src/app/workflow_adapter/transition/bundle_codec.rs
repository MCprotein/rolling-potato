//! Canonical transition-bundle codec facade.

mod additional_members;
mod semantic;
mod source_members;

pub(super) use additional_members::{parse_additional_members, PreparedMemberParseContext};
pub(super) use semantic::{
    parse_event_chain_plan, parse_projection_lag_reference, parse_semantic_events,
    prepared_member_order, render_event_chain_plan, render_semantic_event, render_semantic_events,
};
pub(super) use source_members::{parse_source_members, render_source_members};
