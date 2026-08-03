//! Bounded read-only web search implemented with direct public HTML retrieval.

use crate::foundation::error::AppError;
use std::time::{Duration, Instant};

mod browser_policy;
mod evidence;
mod find;
mod html;
mod page;
mod policy;
mod transport;

pub(crate) use browser_policy::{resolve_public_browser_target, validate_browser_navigation_url};
use evidence::evidence_from_results;
pub(crate) use evidence::{WebOpenResult, WebPageEvidence, WebSearchEvidence, WebSourceEvidence};
pub(crate) use find::{find_in_page, WebFindEvidence};
use html::{parse_html_search_results, parse_lite_search_results};
use page::parse_page_document;
use policy::{resolve_redirect_url, same_web_origin, validate_open_url, validate_query};
use transport::{
    fetch_page_response_with_timeout, fetch_search_document_with_timeout, PageResponse,
    SearchEndpoint,
};

const MAX_PAGE_REDIRECTS: usize = 10;
const SEARCH_REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
#[cfg(test)]
const SEARCH_OPERATION_TIMEOUT: Duration = Duration::from_secs(40);
const PAGE_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
#[cfg(test)]
const PAGE_OPERATION_TIMEOUT: Duration = Duration::from_secs(30 * (MAX_PAGE_REDIRECTS as u64 + 1));

#[cfg(test)]
use evidence::{stable_source_id, SearchResult, MAX_SEARCH_CONTEXT_CHARS};
#[cfg(test)]
use html::normalize_result_url;
#[cfg(test)]
use page::{normalize_page_text, MAX_PAGE_CONTEXT_CHARS};
#[cfg(test)]
use policy::{
    canonicalize_source_url, is_valid_https_source_url, socket_addresses_are_public,
    MAX_QUERY_CHARS, MAX_QUERY_WORDS,
};
#[cfg(test)]
use transport::{direct_agent_config, map_search_error, page_agent_config};

#[cfg(test)]
pub(crate) fn search(
    query: &str,
    allow_lite_fallback: bool,
) -> Result<WebSearchEvidence, AppError> {
    search_with_timeout(query, allow_lite_fallback, SEARCH_OPERATION_TIMEOUT)
}

pub(crate) fn search_with_timeout(
    query: &str,
    allow_lite_fallback: bool,
    timeout: Duration,
) -> Result<WebSearchEvidence, AppError> {
    let query = validate_query(query)?.to_string();

    #[cfg(debug_assertions)]
    {
        let html_fixture = std::env::var_os("RPOTATO_TEST_WEB_SEARCH_HTML")
            .map(|fixture| fixture.to_string_lossy().into_owned());
        let lite_fixture = std::env::var_os("RPOTATO_TEST_WEB_SEARCH_LITE")
            .map(|fixture| fixture.to_string_lossy().into_owned());
        if html_fixture.is_some() || lite_fixture.is_some() {
            return evidence_from_documents(
                &query,
                html_fixture.as_deref(),
                lite_fixture.as_deref(),
                allow_lite_fallback,
            );
        }
    }

    let started = Instant::now();
    let html = fetch_search_document_with_timeout(
        &query,
        SearchEndpoint::Html,
        remaining_timeout(started, timeout)?.min(SEARCH_REQUEST_TIMEOUT),
    );
    if let Ok(evidence) = html.and_then(|document| {
        parse_html_search_results(&document)
            .and_then(|results| evidence_from_results(&query, results))
    }) {
        return Ok(evidence);
    }
    if !allow_lite_fallback {
        return Err(AppError::blocked(
            "직접 웹 검색 HTML 결과를 사용할 수 없고 lite fallback 요청 예산이 없습니다.",
        ));
    }
    fetch_search_document_with_timeout(
        &query,
        SearchEndpoint::Lite,
        remaining_timeout(started, timeout)?.min(SEARCH_REQUEST_TIMEOUT),
    )
    .and_then(|document| parse_lite_search_results(&document))
    .and_then(|results| evidence_from_results(&query, results))
    .map_err(|_| {
        AppError::runtime(
            "직접 웹 검색 HTML과 lite 결과를 모두 사용할 수 없어 검색을 종료했습니다.",
        )
    })
}

#[cfg(any(test, debug_assertions))]
fn evidence_from_documents(
    query: &str,
    html: Option<&str>,
    lite: Option<&str>,
    allow_lite_fallback: bool,
) -> Result<WebSearchEvidence, AppError> {
    if let Some(document) = html {
        if let Ok(evidence) = parse_html_search_results(document)
            .and_then(|results| evidence_from_results(query, results))
        {
            return Ok(evidence);
        }
    }
    if !allow_lite_fallback {
        return Err(AppError::blocked(
            "직접 웹 검색 HTML fixture가 parser contract를 만족하지 않고 lite fallback이 비활성화되었습니다.",
        ));
    }
    let document = lite.ok_or_else(|| {
        AppError::runtime("직접 웹 검색 lite fallback fixture가 준비되지 않았습니다.")
    })?;
    parse_lite_search_results(document)
        .and_then(|results| evidence_from_results(query, results))
        .map_err(|_| {
            AppError::runtime(
                "직접 웹 검색 HTML과 lite fixture가 모두 parser contract를 만족하지 않습니다.",
            )
        })
}

#[cfg(test)]
pub(crate) fn open(url: &str) -> Result<WebOpenResult, AppError> {
    open_with_timeout(url, PAGE_OPERATION_TIMEOUT)
}

pub(crate) fn open_with_timeout(url: &str, timeout: Duration) -> Result<WebOpenResult, AppError> {
    let requested_url = validate_open_url(url)?;

    #[cfg(debug_assertions)]
    if let Some(fixture) = std::env::var_os("RPOTATO_TEST_WEB_OPEN_HTML") {
        return parse_page_document(
            &requested_url,
            &requested_url,
            &fixture.to_string_lossy(),
            "text/html",
        )
        .map(WebOpenResult::Opened);
    }

    let started = Instant::now();
    let mut current_url = requested_url.clone();
    for redirect_count in 0..=MAX_PAGE_REDIRECTS {
        match fetch_page_response_with_timeout(
            &current_url,
            remaining_timeout(started, timeout)?.min(PAGE_REQUEST_TIMEOUT),
        )? {
            PageResponse::Document { content_type, body } => {
                return parse_page_document(&requested_url, &current_url, &body, &content_type)
                    .map(WebOpenResult::Opened);
            }
            PageResponse::Redirect { location } => {
                let target_url = resolve_redirect_url(&current_url, &location)?;
                if !same_web_origin(&current_url, &target_url) {
                    return Ok(WebOpenResult::Redirect {
                        from_url: current_url,
                        target_url,
                    });
                }
                if redirect_count == MAX_PAGE_REDIRECTS {
                    return Err(AppError::blocked(
                        "WebOpen 동일 host redirect가 10회를 초과했습니다.",
                    ));
                }
                current_url = target_url;
            }
        }
    }
    unreachable!("redirect loop returns at its bounded terminal state")
}

fn remaining_timeout(started: Instant, timeout: Duration) -> Result<Duration, AppError> {
    timeout
        .checked_sub(started.elapsed())
        .filter(|remaining| !remaining.is_zero())
        .ok_or_else(|| AppError::blocked("웹 transport 시간 상한에 도달했습니다."))
}

pub(crate) fn configuration_summary() -> String {
    "사용 가능; API key 없는 WebSearch·WebOpen·WebFind".to_string()
}

#[cfg(test)]
mod tests;
