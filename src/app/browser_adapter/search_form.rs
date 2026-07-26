use std::thread;
use std::time::Duration;

use crate::adapters::browser::{BrowserSessionOptions, RestrictedBrowser};
use crate::adapters::web_search::validate_browser_navigation_url;
use crate::foundation::error::AppError;
use crate::runtime_core::browser::{
    BrowserAction, BrowserActionResult, BrowserKey, BrowserObservation, ElementRole,
};

use super::BrowserSearchRequest;

const RESULT_SETTLE_DELAY: Duration = Duration::from_millis(400);
const RESULT_EXTRACT_CHARS: usize = 4 * 1024;
const RESULT_ATTEMPTS: usize = 2;

pub(super) trait BrowserControl {
    fn execute(&mut self, action: BrowserAction) -> Result<BrowserActionResult, AppError>;

    fn settle_after_submit(&mut self) {
        thread::sleep(RESULT_SETTLE_DELAY);
    }
}

struct LiveBrowser {
    inner: RestrictedBrowser,
}

impl LiveBrowser {
    fn launch() -> Result<Self, AppError> {
        RestrictedBrowser::launch(BrowserSessionOptions::default()).map(|inner| Self { inner })
    }
}

impl BrowserControl for LiveBrowser {
    fn execute(&mut self, action: BrowserAction) -> Result<BrowserActionResult, AppError> {
        self.inner.execute(action)
    }
}

pub(super) fn execute(request: BrowserSearchRequest) -> Result<String, AppError> {
    let request = validate_request(request)?;
    let mut browser = LiveBrowser::launch()?;
    execute_with(&mut browser, &request)
}

fn validate_request(request: BrowserSearchRequest) -> Result<BrowserSearchRequest, AppError> {
    let url = validate_browser_navigation_url(&request.url)?;
    let query = request.query.trim();
    if query.is_empty() || query.chars().count() > 200 || query.chars().any(char::is_control) {
        return Err(AppError::usage(
            "브라우저 검색어는 제어 문자 없이 1~200자여야 합니다.",
        ));
    }
    Ok(BrowserSearchRequest {
        url,
        query: query.to_string(),
    })
}

pub(super) fn execute_with(
    browser: &mut impl BrowserControl,
    request: &BrowserSearchRequest,
) -> Result<String, AppError> {
    let outcome = execute_sequence(browser, request);
    let close = browser.execute(BrowserAction::Close);
    match outcome {
        Ok(report) => {
            close?;
            Ok(report)
        }
        Err(error) => {
            let _ = close;
            Err(error)
        }
    }
}

fn execute_sequence(
    browser: &mut impl BrowserControl,
    request: &BrowserSearchRequest,
) -> Result<String, AppError> {
    expect(
        browser.execute(BrowserAction::Navigate {
            url: request.url.clone(),
        })?,
        "페이지 navigation",
        |result| matches!(result, BrowserActionResult::Navigated { .. }),
    )?;
    let initial_observation = observation(browser.execute(BrowserAction::Observe)?)?;
    let handle = search_field(&initial_observation)?;
    expect(
        browser.execute(BrowserAction::Type {
            handle,
            text: request.query.clone(),
        })?,
        "검색어 입력",
        |result| *result == BrowserActionResult::Typed,
    )?;
    expect(
        browser.execute(BrowserAction::Press {
            key: BrowserKey::Enter,
        })?,
        "검색 제출",
        |result| *result == BrowserActionResult::KeyPressed,
    )?;

    let mut result = None;
    for _ in 0..RESULT_ATTEMPTS {
        browser.settle_after_submit();
        observation(browser.execute(BrowserAction::Observe)?)?;
        let BrowserActionResult::Extracted { text } = browser.execute(BrowserAction::Extract {
            max_chars: RESULT_EXTRACT_CHARS,
        })?
        else {
            return Err(AppError::runtime(
                "격리 브라우저가 예상한 페이지 text 결과를 반환하지 않았습니다.",
            ));
        };
        let BrowserActionResult::CurrentUrl { url } = browser.execute(BrowserAction::CurrentUrl)?
        else {
            return Err(AppError::runtime(
                "격리 브라우저가 현재 공개 URL을 반환하지 않았습니다.",
            ));
        };
        let url = validate_browser_navigation_url(&url)?;
        let normalized_text = text.to_lowercase();
        let normalized_query = request.query.to_lowercase();
        let result_ready = url != request.url || normalized_text.contains(&normalized_query);
        if !text.trim().is_empty() && result_ready {
            result = Some((url, text));
            break;
        }
    }
    let Some((url, extracted)) = result else {
        return Err(AppError::runtime(
            "검색 결과 페이지 전환과 읽을 수 있는 공개 text를 확인하지 못했습니다.",
        ));
    };
    Ok(format!(
        "브라우저 검색을 완료했습니다.\n- 검색어: {}\n- 결과 URL: {}\n\n{}",
        request.query,
        url,
        extracted.trim()
    ))
}

fn observation(result: BrowserActionResult) -> Result<BrowserObservation, AppError> {
    let BrowserActionResult::Observation(observation) = result else {
        return Err(AppError::runtime(
            "격리 브라우저가 예상한 접근성 관찰 결과를 반환하지 않았습니다.",
        ));
    };
    Ok(observation)
}

fn search_field(
    observation: &BrowserObservation,
) -> Result<crate::runtime_core::browser::ElementHandle, AppError> {
    observation
        .elements
        .iter()
        .find(|element| element.role == ElementRole::SearchBox)
        .or_else(|| {
            observation.elements.iter().find(|element| {
                element.role == ElementRole::TextField
                    && ["search", "검색", "찾기"]
                        .iter()
                        .any(|signal| element.name.to_lowercase().contains(signal))
            })
        })
        .map(|element| element.handle.clone())
        .ok_or_else(|| {
            AppError::blocked(
                "공개 페이지에서 안전하게 확인된 검색 field를 찾지 못해 입력하지 않았습니다.",
            )
        })
}

fn expect(
    result: BrowserActionResult,
    context: &str,
    predicate: impl FnOnce(&BrowserActionResult) -> bool,
) -> Result<(), AppError> {
    predicate(&result)
        .then_some(())
        .ok_or_else(|| AppError::runtime(format!("격리 브라우저 {context} 결과가 다릅니다.")))
}
