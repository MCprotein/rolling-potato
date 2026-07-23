use std::time::Duration;

use crate::foundation::error::AppError;

const DIRECT_SEARCH_ENDPOINT: &str = "https://html.duckduckgo.com/html/";
const MAX_SEARCH_RESPONSE_BYTES: u64 = 512 * 1024;

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
