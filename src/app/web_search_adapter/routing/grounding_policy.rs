//! Generic policy for deciding when public web evidence is required.
//!
//! This owner deliberately knows nothing about particular products, vendors, events, or
//! answers. It classifies request features and delegates query construction to a separate plan.

mod features;
mod query_plan;

use features::GroundingSignals;

pub(in crate::app::web_search_adapter) fn requires_external_grounding(request: &str) -> bool {
    GroundingSignals::from_request(request).requires_external_grounding()
}

pub(in crate::app::web_search_adapter) fn strengthen_search_query(
    query: &str,
    request: &str,
) -> String {
    query_plan::strengthen(query, GroundingSignals::from_request(request))
}

#[cfg(test)]
#[path = "grounding_policy/tests.rs"]
mod tests;
