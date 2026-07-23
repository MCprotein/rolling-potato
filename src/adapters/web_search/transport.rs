use std::time::Duration;

use crate::foundation::error::AppError;
use ureq::unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::{DefaultConnector, NextTimeout};

use super::policy::socket_addresses_are_public;

const DIRECT_SEARCH_ENDPOINT: &str = "https://html.duckduckgo.com/html/";
const MAX_SEARCH_RESPONSE_BYTES: u64 = 512 * 1024;
const MAX_PAGE_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;

pub(super) enum PageResponse {
    Document { content_type: String, body: String },
    Redirect { location: String },
}

pub(super) fn fetch_search_document(query: &str) -> Result<String, AppError> {
    let agent = ureq::Agent::new_with_config(direct_agent_config());
    let mut response = agent
        .get(DIRECT_SEARCH_ENDPOINT)
        .query("q", query)
        .query("kl", "kr-kr")
        .header("Accept", "text/html,application/xhtml+xml")
        .header("User-Agent", concat!("rpotato/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(map_search_error)?;
    if response.status().is_redirection() {
        return Err(AppError::blocked(
            "직접 웹 검색 endpoint가 redirect를 반환해 요청을 중단했습니다.",
        ));
    }
    response
        .body_mut()
        .with_config()
        .limit(MAX_SEARCH_RESPONSE_BYTES)
        .read_to_string()
        .map_err(|_| AppError::runtime("웹 검색 문서를 제한된 크기로 읽지 못했습니다."))
}

pub(super) fn direct_agent_config() -> ureq::config::Config {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .https_only(true)
        .max_redirects(0)
        .build()
}

pub(super) fn fetch_page_response(url: &str) -> Result<PageResponse, AppError> {
    let agent = ureq::Agent::with_parts(
        page_agent_config(),
        DefaultConnector::default(),
        PublicWebResolver::default(),
    );
    let mut response = agent
        .get(url)
        .header(
            "Accept",
            "text/html,application/xhtml+xml,text/plain,application/json;q=0.8",
        )
        .header("User-Agent", concat!("rpotato/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(map_page_error)?;
    let status = response.status();
    if matches!(status.as_u16(), 301 | 302 | 303 | 307 | 308) {
        let location = response
            .headers()
            .get("location")
            .and_then(|value| value.to_str().ok())
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| AppError::blocked("WebOpen redirect에 Location URL이 없습니다."))?;
        return Ok(PageResponse::Redirect {
            location: location.to_string(),
        });
    }
    if status.is_client_error() || status.is_server_error() {
        return Err(map_page_status(status.as_u16()));
    }
    if !status.is_success() {
        return Err(AppError::runtime(format!(
            "WebOpen 대상이 지원하지 않는 HTTP status를 반환했습니다: {}",
            status.as_u16()
        )));
    }
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("text/html")
        .to_string();
    let body = response
        .body_mut()
        .with_config()
        .limit(MAX_PAGE_RESPONSE_BYTES)
        .read_to_string()
        .map_err(|_| AppError::runtime("WebOpen 문서를 제한된 크기로 읽지 못했습니다."))?;
    Ok(PageResponse::Document { content_type, body })
}

pub(super) fn page_agent_config() -> ureq::config::Config {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .https_only(true)
        .http_status_as_error(false)
        .max_redirects(0)
        .proxy(None)
        .build()
}

pub(super) fn map_search_error(error: ureq::Error) -> AppError {
    match error {
        ureq::Error::StatusCode(403 | 429) => AppError::runtime(
            "직접 웹 검색 요청이 일시적으로 제한되었습니다. 잠시 뒤 다시 시도하세요.",
        ),
        ureq::Error::StatusCode(400..=499) => {
            AppError::runtime("직접 웹 검색 endpoint가 요청을 거부했습니다.")
        }
        ureq::Error::StatusCode(500..=599) => {
            AppError::runtime("직접 웹 검색 endpoint가 일시적으로 응답하지 않습니다.")
        }
        _ => AppError::runtime("공개 웹 검색 페이지에 연결하지 못했습니다."),
    }
}

fn map_page_error(error: ureq::Error) -> AppError {
    match error {
        ureq::Error::HostNotFound => {
            AppError::blocked("WebOpen 대상 host가 공개 IP로 해석되지 않아 연결을 차단했습니다.")
        }
        _ => AppError::runtime("WebOpen 대상 페이지에 연결하지 못했습니다."),
    }
}

fn map_page_status(status: u16) -> AppError {
    match status {
        401 | 403 => AppError::blocked("WebOpen 대상 페이지가 접근을 거부했습니다."),
        404 => AppError::blocked("WebOpen 대상 페이지를 찾지 못했습니다."),
        429 => AppError::runtime(
            "WebOpen 대상 페이지가 요청을 일시적으로 제한했습니다. 잠시 뒤 다시 시도하세요.",
        ),
        400..=499 => AppError::blocked(format!(
            "WebOpen 대상 페이지 요청이 거부되었습니다: HTTP {status}"
        )),
        _ => AppError::runtime(format!(
            "WebOpen 대상 페이지가 일시적으로 응답하지 않습니다: HTTP {status}"
        )),
    }
}

#[derive(Debug, Default)]
struct PublicWebResolver {
    inner: DefaultResolver,
}

impl Resolver for PublicWebResolver {
    fn resolve(
        &self,
        uri: &ureq::http::Uri,
        config: &ureq::config::Config,
        timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        let addresses = self.inner.resolve(uri, config, timeout)?;
        if socket_addresses_are_public(&addresses) {
            Ok(addresses)
        } else {
            Err(ureq::Error::HostNotFound)
        }
    }
}
