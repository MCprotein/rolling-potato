//! Application orchestration for anonymous, read-only browser research.

mod routing;
mod search_form;

pub(crate) use routing::{deterministic_browser_fallback, BrowserSearchRequest};

pub(crate) fn progress_notice(request: &str) -> Option<String> {
    deterministic_browser_fallback(request).map(|_| {
        "브라우저 조사 · 공개 검색 페이지 여는 중\n페이지 열기 ● → 검색창 확인 ○ → 검색어 입력 ○ → 결과 읽기 ○"
            .to_string()
    })
}

pub(crate) fn search_form(
    request: BrowserSearchRequest,
) -> Result<String, crate::foundation::error::AppError> {
    search_form::execute(request)
}

#[cfg(test)]
#[path = "browser_adapter/tests.rs"]
mod tests;
