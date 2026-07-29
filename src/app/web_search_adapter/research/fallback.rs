use std::time::Duration;

use super::{
    session::WebResearchSession,
    types::{WebResearchAdmission, WebResearchStep},
};

#[cfg(test)]
pub(crate) fn deterministic_freshness_fallback(request: &str) -> Option<WebResearchStep> {
    deterministic_freshness_fallback_for_context(request, &[])
}

pub(crate) fn deterministic_freshness_fallback_for_context(
    request: &str,
    prior_user_requests: &[&str],
) -> Option<WebResearchStep> {
    let request = request.trim();
    if request.is_empty()
        || super::super::routing::web_disabled(request)
        || !super::super::routing::requires_external_grounding(request)
    {
        return None;
    }
    let query =
        super::super::routing::contextualize_search_input(request, request, prior_user_requests)?;
    let query = super::super::routing::strengthen_search_query(&query, request);
    let mut research = WebResearchSession::default();
    match research.deterministic_fallback(&query, &[], Duration::ZERO) {
        WebResearchAdmission::Execute(step) => Some(step),
        WebResearchAdmission::Stop(_) => None,
    }
}
