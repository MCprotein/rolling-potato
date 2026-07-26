use crate::runtime_core::browser::{
    BrowserAction, BrowserActionResult, BrowserKey, BrowserObservation, ElementHandle, ElementRole,
    ObservedElement,
};

use super::search_form::{execute_with, BrowserControl, ReadinessPhase};
use super::*;

#[test]
fn agent_browser_contract_accepts_only_a_bounded_search_form_request() {
    assert_eq!(
        parse_agent_browser_tool(
            "BROWSER TOOL: search-form\nBROWSER URL: https://www.google.com/\nBROWSER INPUT: Rust 2026"
        ),
        Some(BrowserSearchRequest {
            url: "https://www.google.com/".to_string(),
            query: "Rust 2026".to_string(),
        })
    );
    for invalid in [
        "BROWSER TOOL: click\nBROWSER URL: https://example.com/\nBROWSER INPUT: submit",
        "BROWSER TOOL: search-form\nBROWSER INPUT: missing URL",
        "BROWSER TOOL: search-form\nBROWSER URL: https://example.com/\nBROWSER INPUT:",
    ] {
        assert!(parse_agent_browser_tool(invalid).is_none(), "{invalid}");
    }
}

#[test]
fn explicit_naver_and_google_requests_have_a_small_model_fallback() {
    assert_eq!(
        deterministic_browser_fallback("네이버를 열고 검색창에 월드컵을 입력해"),
        Some(BrowserSearchRequest {
            url: "https://www.naver.com/".to_string(),
            query: "월드컵".to_string(),
        })
    );
    assert_eq!(
        deterministic_browser_fallback("Open Google and search for Rust ownership"),
        Some(BrowserSearchRequest {
            url: "https://www.google.com/".to_string(),
            query: "Rust ownership".to_string(),
        })
    );
    assert!(deterministic_browser_fallback("네이버가 뭔지 설명해줘").is_none());
    assert!(deterministic_browser_fallback(
        "인터넷 쓰지 말고 네이버를 열고 검색창에 월드컵을 입력해"
    )
    .is_none());
}

#[test]
fn generic_search_form_e2e_uses_opaque_handles_and_always_closes() {
    let mut browser = FakeBrowser::successful();
    let report = execute_with(
        &mut browser,
        &BrowserSearchRequest {
            url: "https://www.naver.com/".to_string(),
            query: "월드컵".to_string(),
        },
    )
    .unwrap();

    assert_eq!(
        browser.steps,
        [
            "navigate",
            "wait-initial",
            "observe",
            "type",
            "press-enter",
            "wait-result",
            "observe",
            "extract",
            "current-url",
            "close",
        ]
    );
    assert!(report.contains("검색어: 월드컵"));
    assert!(report.contains("https://search.example.com/?q=worldcup"));
    assert!(report.contains("월드컵 검색 결과"));
}

#[test]
fn missing_search_field_fails_closed_and_still_cleans_up() {
    let mut browser = FakeBrowser::without_search_field();
    let error = execute_with(
        &mut browser,
        &BrowserSearchRequest {
            url: "https://www.google.com/".to_string(),
            query: "월드컵".to_string(),
        },
    )
    .unwrap_err();

    assert!(error.message.contains("검색 field"));
    assert_eq!(
        browser.steps,
        [
            "navigate",
            "wait-initial",
            "observe",
            "wait-initial",
            "observe",
            "close"
        ]
    );
}

#[test]
fn delayed_initial_page_readiness_is_polled_before_typing() {
    let mut browser = FakeBrowser::successful();
    browser.initial_observations_before_ready = 1;

    let report = execute_with(
        &mut browser,
        &BrowserSearchRequest {
            url: "https://www.naver.com/".to_string(),
            query: "월드컵".to_string(),
        },
    )
    .unwrap();

    assert!(report.contains("월드컵 검색 결과"));
    assert_eq!(
        browser.steps,
        [
            "navigate",
            "wait-initial",
            "observe",
            "wait-initial",
            "observe",
            "type",
            "press-enter",
            "wait-result",
            "observe",
            "extract",
            "current-url",
            "close",
        ]
    );
}

#[test]
fn delayed_result_page_readiness_is_polled_before_reporting() {
    let mut browser = FakeBrowser::successful();
    browser.result_observations_before_ready = 1;

    let report = execute_with(
        &mut browser,
        &BrowserSearchRequest {
            url: "https://www.naver.com/".to_string(),
            query: "월드컵".to_string(),
        },
    )
    .unwrap();

    assert!(report.contains("월드컵 검색 결과"));
    assert_eq!(
        browser
            .steps
            .iter()
            .filter(|step| **step == "wait-result")
            .count(),
        2
    );
    assert_eq!(
        browser
            .steps
            .iter()
            .filter(|step| **step == "extract")
            .count(),
        2
    );
    assert_eq!(browser.steps.last(), Some(&"close"));
}

#[test]
fn private_redirect_result_is_rejected_and_browser_is_closed() {
    let mut browser = FakeBrowser::successful();
    browser.current_url = "https://127.0.0.1/private";
    let error = execute_with(
        &mut browser,
        &BrowserSearchRequest {
            url: "https://www.google.com/".to_string(),
            query: "월드컵".to_string(),
        },
    )
    .unwrap_err();

    assert!(
        error.message.contains("내부·로컬"),
        "unexpected error: {}",
        error.message
    );
    assert_eq!(browser.steps.last(), Some(&"close"));
}

#[test]
fn unchanged_home_page_is_not_misreported_as_search_evidence() {
    let mut browser = FakeBrowser::successful();
    browser.current_url = "https://www.google.com/";
    browser.extracted_text = "검색 전 홈 화면";
    let error = execute_with(
        &mut browser,
        &BrowserSearchRequest {
            url: "https://www.google.com/".to_string(),
            query: "월드컵".to_string(),
        },
    )
    .unwrap_err();

    assert!(error.message.contains("결과 페이지 전환"));
    assert_eq!(
        browser
            .steps
            .iter()
            .filter(|step| **step == "extract")
            .count(),
        2
    );
    assert_eq!(browser.steps.last(), Some(&"close"));
}

#[test]
fn explicit_browser_request_exposes_structured_tui_progress() {
    let notice = progress_notice("네이버를 열고 검색창에 월드컵을 입력해").unwrap();
    assert!(notice.contains("브라우저 조사"));
    assert!(notice.contains("페이지 열기 ●"));
    assert!(notice.contains("결과 읽기 ○"));
    assert!(progress_notice("네이버가 뭔지 설명해줘").is_none());
}

#[test]
#[ignore = "requires an installed Chromium-family browser and public network access"]
fn live_chromium_search_form_smoke_is_explicit_opt_in() {
    let report = super::search_form(BrowserSearchRequest {
        url: "https://www.google.com/".to_string(),
        query: "Rust programming language".to_string(),
    })
    .unwrap();

    assert!(report.contains("검색어: Rust programming language"));
    assert!(report.contains("결과 URL: https://"));
}

struct FakeBrowser {
    steps: Vec<&'static str>,
    has_search_field: bool,
    initial_observations_before_ready: usize,
    result_observations_before_ready: usize,
    submitted: bool,
    result_page_ready: bool,
    current_url: &'static str,
    extracted_text: &'static str,
}

impl FakeBrowser {
    fn successful() -> Self {
        Self {
            steps: Vec::new(),
            has_search_field: true,
            initial_observations_before_ready: 0,
            result_observations_before_ready: 0,
            submitted: false,
            result_page_ready: true,
            current_url: "https://search.example.com/?q=worldcup",
            extracted_text: "월드컵 검색 결과",
        }
    }

    fn without_search_field() -> Self {
        Self {
            steps: Vec::new(),
            has_search_field: false,
            initial_observations_before_ready: 0,
            result_observations_before_ready: 0,
            submitted: false,
            result_page_ready: true,
            current_url: "https://search.example.com/?q=worldcup",
            extracted_text: "월드컵 검색 결과",
        }
    }
}

impl BrowserControl for FakeBrowser {
    fn execute(
        &mut self,
        action: BrowserAction,
    ) -> Result<BrowserActionResult, crate::foundation::error::AppError> {
        match action {
            BrowserAction::Navigate { url } => {
                self.steps.push("navigate");
                Ok(BrowserActionResult::Navigated { url })
            }
            BrowserAction::Observe => {
                self.steps.push("observe");
                if self.submitted {
                    self.result_page_ready = self.result_observations_before_ready == 0;
                    self.result_observations_before_ready =
                        self.result_observations_before_ready.saturating_sub(1);
                }
                let initial_ready = if self.submitted {
                    true
                } else if self.initial_observations_before_ready == 0 {
                    true
                } else {
                    self.initial_observations_before_ready -= 1;
                    false
                };
                let elements = (self.has_search_field && initial_ready).then(|| ObservedElement {
                    handle: ElementHandle::parse("element-1-1").unwrap(),
                    role: ElementRole::SearchBox,
                    name: "검색".to_string(),
                    disabled: false,
                });
                Ok(BrowserActionResult::Observation(BrowserObservation {
                    revision: 1,
                    elements: elements.into_iter().collect(),
                }))
            }
            BrowserAction::Type { handle, text } => {
                assert_eq!(handle.as_str(), "element-1-1");
                assert_eq!(text, "월드컵");
                self.steps.push("type");
                Ok(BrowserActionResult::Typed)
            }
            BrowserAction::Press {
                key: BrowserKey::Enter,
            } => {
                self.submitted = true;
                self.steps.push("press-enter");
                Ok(BrowserActionResult::KeyPressed)
            }
            BrowserAction::Extract { max_chars } => {
                assert_eq!(max_chars, 4 * 1024);
                self.steps.push("extract");
                Ok(BrowserActionResult::Extracted {
                    text: if self.result_page_ready {
                        self.extracted_text.to_string()
                    } else {
                        "검색 전 홈 화면".to_string()
                    },
                })
            }
            BrowserAction::CurrentUrl => {
                self.steps.push("current-url");
                Ok(BrowserActionResult::CurrentUrl {
                    url: if self.result_page_ready {
                        self.current_url.to_string()
                    } else {
                        "https://www.naver.com/".to_string()
                    },
                })
            }
            BrowserAction::Close => {
                self.steps.push("close");
                Ok(BrowserActionResult::Closed)
            }
            other => panic!("unexpected browser action: {other:?}"),
        }
    }

    fn wait_for_readiness(&mut self, phase: ReadinessPhase) {
        self.steps.push(match phase {
            ReadinessPhase::InitialPage => "wait-initial",
            ReadinessPhase::ResultPage => "wait-result",
        });
    }
}
