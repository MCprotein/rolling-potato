use std::time::Duration;

use crate::foundation::serialization::{parse_value, Value};
use crate::runtime_core::browser::{BrowserAction, BrowserActionResult, BrowserKey, ElementRole};

use super::*;

struct FakePort {
    calls: Vec<(CdpMethod, String)>,
    accessibility_tree: Value,
    closed: bool,
}

impl FakePort {
    fn new() -> Self {
        Self {
            calls: Vec::new(),
            accessibility_tree: value(
                r#"{
                    "nodes":[
                        {
                            "nodeId":"1",
                            "ignored":false,
                            "role":{"type":"role","value":"searchbox"},
                            "name":{"type":"computedString","value":"검색"},
                            "backendDOMNodeId":41
                        },
                        {
                            "nodeId":"2",
                            "ignored":false,
                            "role":{"type":"role","value":"button"},
                            "name":{"type":"computedString","value":"검색"},
                            "backendDOMNodeId":42
                        },
                        {
                            "nodeId":"3",
                            "ignored":false,
                            "role":{"type":"role","value":"button"},
                            "name":{"type":"computedString","value":"로그인"},
                            "backendDOMNodeId":43
                        },
                        {
                            "nodeId":"4",
                            "ignored":false,
                            "role":{"type":"role","value":"StaticText"},
                            "name":{"type":"computedString","value":"월드컵 검색 결과"},
                            "backendDOMNodeId":44
                        },
                        {
                            "nodeId":"5",
                            "ignored":false,
                            "role":{"type":"role","value":"StaticText"},
                            "name":{"type":"computedString","value":"월드컵 검색 결과"},
                            "backendDOMNodeId":45
                        }
                    ]
                }"#,
            ),
            closed: false,
        }
    }
}

impl BrowserActionPort for FakePort {
    fn call(&mut self, method: CdpMethod, params_json: String) -> Result<Value, AppError> {
        self.calls.push((method, params_json));
        match method {
            CdpMethod::AccessibilityGetFullAxTree => Ok(self.accessibility_tree.clone()),
            CdpMethod::DomGetBoxModel => Ok(value(
                r#"{"model":{"content":[10,20,110,20,110,60,10,60]}}"#,
            )),
            CdpMethod::PageGetNavigationHistory => Ok(value(
                r#"{
                    "currentIndex":1,
                    "entries":[
                        {"id":1,"url":"https://www.google.com/"},
                        {"id":2,"url":"https://www.google.com/search?q=rust"}
                    ]
                }"#,
            )),
            CdpMethod::PageCaptureScreenshot => Ok(value(r#"{"data":"iVBORw0KGgo="}"#)),
            _ => Ok(value("{}")),
        }
    }

    fn close_target(&mut self) -> Result<(), AppError> {
        self.closed = true;
        Ok(())
    }
}

#[test]
fn observes_types_presses_extracts_and_expires_handles() {
    let mut driver = BrowserActionDriver::new(FakePort::new());
    let observation = match driver
        .execute(BrowserAction::Observe, Duration::ZERO)
        .unwrap()
    {
        BrowserActionResult::Observation(observation) => observation,
        other => panic!("unexpected result: {other:?}"),
    };
    assert_eq!(observation.elements.len(), 3);
    let search = observation
        .elements
        .iter()
        .find(|element| element.role == ElementRole::SearchBox)
        .unwrap()
        .handle
        .clone();

    assert_eq!(
        driver
            .execute(
                BrowserAction::Type {
                    handle: search.clone(),
                    text: "월드컵 \"결과\"".to_string(),
                },
                Duration::ZERO,
            )
            .unwrap(),
        BrowserActionResult::Typed
    );
    assert!(driver.transport.calls.iter().any(|(method, params)| {
        *method == CdpMethod::DomFocus && params == "{\"backendNodeId\":41}"
    }));
    assert!(driver.transport.calls.iter().any(|(method, params)| {
        *method == CdpMethod::InputInsertText && params == "{\"text\":\"월드컵 \\\"결과\\\"\"}"
    }));
    assert!(driver
        .execute(
            BrowserAction::Type {
                handle: search,
                text: "stale".to_string(),
            },
            Duration::ZERO,
        )
        .unwrap_err()
        .message
        .contains("만료"));

    assert_eq!(
        driver
            .execute(
                BrowserAction::Press {
                    key: BrowserKey::Enter,
                },
                Duration::ZERO,
            )
            .unwrap(),
        BrowserActionResult::KeyPressed
    );
    let extracted = driver
        .execute(BrowserAction::Extract { max_chars: 128 }, Duration::ZERO)
        .unwrap();
    assert_eq!(
        extracted,
        BrowserActionResult::Extracted {
            text: "월드컵 검색 결과".to_string()
        }
    );
    assert_eq!(
        driver
            .execute(BrowserAction::CurrentUrl, Duration::ZERO)
            .unwrap(),
        BrowserActionResult::CurrentUrl {
            url: "https://www.google.com/search?q=rust".to_string()
        }
    );
}

#[test]
fn click_uses_an_observed_backend_node_without_selectors_or_javascript() {
    let mut driver = BrowserActionDriver::new(FakePort::new());
    let observation = match driver
        .execute(BrowserAction::Observe, Duration::ZERO)
        .unwrap()
    {
        BrowserActionResult::Observation(observation) => observation,
        other => panic!("unexpected result: {other:?}"),
    };
    let search_button = observation
        .elements
        .iter()
        .find(|element| element.role == ElementRole::Button && element.name == "검색")
        .unwrap()
        .handle
        .clone();

    assert_eq!(
        driver
            .execute(
                BrowserAction::Click {
                    handle: search_button,
                },
                Duration::ZERO,
            )
            .unwrap(),
        BrowserActionResult::Clicked
    );
    let transcript = driver
        .transport
        .calls
        .iter()
        .map(|(method, params)| format!("{method:?} {params}"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(transcript.contains("DomGetBoxModel {\"backendNodeId\":42}"));
    assert!(transcript.contains("\"x\":60.00,\"y\":40.00"));
    assert!(!transcript.contains("selector"));
    assert!(!transcript.contains("xpath"));
    assert!(!transcript.contains("Runtime.evaluate"));
}

#[test]
fn forbidden_targets_and_url_schemes_never_reach_the_protocol() {
    let mut driver = BrowserActionDriver::new(FakePort::new());
    let observation = match driver
        .execute(BrowserAction::Observe, Duration::ZERO)
        .unwrap()
    {
        BrowserActionResult::Observation(observation) => observation,
        other => panic!("unexpected result: {other:?}"),
    };
    let login = observation
        .elements
        .iter()
        .find(|element| element.name == "로그인")
        .unwrap()
        .handle
        .clone();
    let calls_before = driver.transport.calls.len();
    assert!(driver
        .execute(BrowserAction::Click { handle: login }, Duration::ZERO)
        .unwrap_err()
        .message
        .contains("민감"));
    assert_eq!(driver.transport.calls.len(), calls_before);

    for url in [
        "file:///etc/passwd",
        "http://example.com/",
        "http://localhost:8080",
        "https://127.0.0.1/private",
        "https://user:password@example.com/",
    ] {
        assert!(driver
            .execute(
                BrowserAction::Navigate {
                    url: url.to_string(),
                },
                Duration::ZERO,
            )
            .is_err());
    }
    assert_eq!(driver.transport.calls.len(), calls_before);
}

#[test]
fn screenshot_and_close_use_bounded_typed_results() {
    let mut driver = BrowserActionDriver::new(FakePort::new());

    assert_eq!(
        driver
            .execute(BrowserAction::Screenshot, Duration::ZERO)
            .unwrap(),
        BrowserActionResult::Screenshot {
            png_base64: "iVBORw0KGgo=".to_string()
        }
    );
    assert_eq!(
        driver
            .execute(BrowserAction::Close, Duration::ZERO)
            .unwrap(),
        BrowserActionResult::Closed
    );
    assert!(driver.transport.closed);
    assert!(driver
        .execute(BrowserAction::Observe, Duration::ZERO)
        .is_err());
}

fn value(input: &str) -> Value {
    parse_value(input, "browser action fixture").unwrap()
}
