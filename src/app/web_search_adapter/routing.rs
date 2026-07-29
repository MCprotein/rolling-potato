use super::research::WebResearchStep;
use super::WebGroundingEvidence;
use crate::foundation::error::AppError;

mod grounding_policy;
mod page_intent;
mod protocol;
mod query;
mod text;
mod web_policy;

pub(super) use grounding_policy::{requires_external_grounding, strengthen_search_query};
pub(crate) use page_intent::route_current_page_find;
pub(crate) use protocol::route_tool_request;
pub(crate) use query::contextualize_search_input;
pub(super) use text::{best_query_term, overlap_score};
pub(crate) use web_policy::web_disabled;

pub(crate) fn validate_public_web_step(step: WebResearchStep) -> Result<WebResearchStep, AppError> {
    let candidate = match &step {
        WebResearchStep::Search { query } => Some(query.as_str()),
        WebResearchStep::Open { url } => Some(url.as_str()),
        WebResearchStep::Find { .. } => None,
    };
    if candidate.is_some_and(contains_credential_like_value) {
        return Err(AppError::blocked(
            "검색어나 URL에 인증정보로 보이는 값이 있어 외부 요청을 차단했습니다. 비밀값을 제거한 공개 검색어로 다시 요청하세요.",
        ));
    }
    Ok(step)
}

pub(crate) fn is_grounded_followup_request(request: &str) -> bool {
    has_explicit_prior_web_reference(request) || is_natural_regrounding_request(request)
}

pub(crate) fn can_reuse_prior_grounding(request: &str, grounding: &[WebGroundingEvidence]) -> bool {
    if grounding.is_empty() || !is_grounded_followup_request(request) {
        return false;
    }
    has_explicit_prior_web_reference(request)
        || request_topic_overlaps_grounding(request, grounding)
}

fn has_explicit_prior_web_reference(request: &str) -> bool {
    let lower = request.trim().to_ascii_lowercase();
    [
        "방금 검색",
        "아까 검색",
        "검색한 ",
        "검색 결과",
        "검색결과",
        "그 출처",
        "해당 출처",
        "출처에서",
        "방금 찾",
        "아까 찾",
        "웹에서 찾",
        "방금 연 ",
        "아까 연 ",
    ]
    .iter()
    .any(|signal| request.contains(signal))
        || [
            "the search result",
            "those search results",
            "the source",
            "those sources",
            "you just search",
            "you just searched",
            "you found earlier",
        ]
        .iter()
        .any(|signal| lower.contains(signal))
}

fn is_natural_regrounding_request(request: &str) -> bool {
    let lower = request.to_ascii_lowercase();
    let asks_again = ["다시", "재설명", "재답변", "again"]
        .iter()
        .any(|signal| lower.contains(signal));
    let asks_for_evidence = ["근거", "출처", "evidence", "source"]
        .iter()
        .any(|signal| lower.contains(signal));
    asks_again && asks_for_evidence
}

fn request_topic_overlaps_grounding(request: &str, grounding: &[WebGroundingEvidence]) -> bool {
    grounding.iter().any(|evidence| {
        overlap_score(request, &evidence.title) > 0 || overlap_score(request, &evidence.excerpt) > 0
    })
}

fn contains_credential_like_value(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    if ["authorization:", "bearer ", "basic "]
        .iter()
        .any(|signal| lower.contains(signal))
    {
        return true;
    }
    if [
        "api_key",
        "apikey",
        "api-key",
        "access_token",
        "access-token",
        "password",
        "passwd",
        "client_secret",
        "client-secret",
        "credential",
    ]
    .iter()
    .any(|name| {
        ["=", ":", "%3d"]
            .iter()
            .any(|separator| lower.contains(&format!("{name}{separator}")))
    }) {
        return true;
    }
    lower
        .split(|character: char| {
            character.is_whitespace()
                || matches!(character, '"' | '\'' | '&' | '?' | ',' | ';' | '(' | ')')
        })
        .any(|token| {
            let token = token.trim_matches(|character: char| {
                !character.is_ascii_alphanumeric() && !matches!(character, '-' | '_')
            });
            (token.starts_with("sk-") && token.len() >= 12)
                || (token.starts_with("ghp_") && token.len() >= 12)
                || (token.starts_with("github_pat_") && token.len() >= 20)
                || (token.starts_with("xoxb-") && token.len() >= 12)
                || (token.starts_with("akia") && token.len() >= 16)
        })
}

#[cfg(test)]
#[path = "routing/tests.rs"]
mod tests;
