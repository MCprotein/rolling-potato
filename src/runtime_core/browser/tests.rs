use std::time::Duration;

use super::*;

fn seed(target_ref: u64, role: ElementRole, name: &str, sensitive: bool) -> ObservedTargetSeed {
    ObservedTargetSeed {
        target_ref,
        role,
        name: name.to_string(),
        disabled: false,
        sensitive,
    }
}

#[test]
fn observation_issues_opaque_handles_and_navigation_expires_them() {
    let mut session = BrowserInteractionSession::default();
    session
        .admit(BrowserAction::Observe, Duration::ZERO)
        .unwrap();
    let observation = session.install_observation([
        seed(982_451, ElementRole::SearchBox, "검색", false),
        seed(982_452, ElementRole::Button, "검색", false),
    ]);
    let handle = observation.elements[0].handle.clone();

    assert_eq!(handle.as_str(), "element-1-1");
    assert!(!handle.as_str().contains("982451"));
    assert!(matches!(
        session
            .admit(
                BrowserAction::Type {
                    handle: handle.clone(),
                    text: "월드컵".to_string(),
                },
                Duration::ZERO,
            )
            .unwrap(),
        AdmittedBrowserAction::Type { target, .. } if target.target_ref == 982_451
    ));

    session.invalidate_handles();
    assert_eq!(
        session.admit(
            BrowserAction::Type {
                handle,
                text: "다시 입력".to_string(),
            },
            Duration::ZERO,
        ),
        Err(BrowserActionBlock::StaleHandle)
    );
}

#[test]
fn sensitive_and_role_mismatched_targets_are_blocked() {
    let mut session = BrowserInteractionSession::default();
    let observation = session.install_observation([
        seed(1, ElementRole::TextField, "비밀번호", true),
        seed(2, ElementRole::Button, "로그인", true),
        seed(3, ElementRole::Other, "광고 영역", false),
    ]);

    assert_eq!(
        session.admit(
            BrowserAction::Type {
                handle: observation.elements[0].handle.clone(),
                text: "secret".to_string(),
            },
            Duration::ZERO,
        ),
        Err(BrowserActionBlock::ForbiddenTarget)
    );
    assert_eq!(
        session.admit(
            BrowserAction::Click {
                handle: observation.elements[1].handle.clone(),
            },
            Duration::ZERO,
        ),
        Err(BrowserActionBlock::ForbiddenTarget)
    );
    assert_eq!(
        session.admit(
            BrowserAction::Click {
                handle: observation.elements[2].handle.clone(),
            },
            Duration::ZERO,
        ),
        Err(BrowserActionBlock::ForbiddenTarget)
    );
}

#[test]
fn text_scroll_extract_and_elapsed_budgets_are_bounded() {
    let mut session = BrowserInteractionSession::default();
    let handle = session
        .install_observation([seed(5, ElementRole::SearchBox, "검색", false)])
        .elements[0]
        .handle
        .clone();

    assert_eq!(
        session.admit(
            BrowserAction::Type {
                handle,
                text: "x".repeat(1_001),
            },
            Duration::ZERO,
        ),
        Err(BrowserActionBlock::InvalidAction)
    );
    assert_eq!(
        session.admit(
            BrowserAction::Scroll {
                direction: ScrollDirection::Down,
                viewports: 4,
            },
            Duration::ZERO,
        ),
        Err(BrowserActionBlock::InvalidAction)
    );
    assert!(session
        .admit(BrowserAction::Extract { max_chars: 8_192 }, Duration::ZERO,)
        .is_ok());
    assert!(session
        .admit(BrowserAction::Extract { max_chars: 8_192 }, Duration::ZERO,)
        .is_ok());
    assert_eq!(
        session.admit(BrowserAction::Extract { max_chars: 1 }, Duration::ZERO,),
        Err(BrowserActionBlock::BudgetReached(
            BrowserActionLimit::ExtractedText
        ))
    );
    assert_eq!(
        BrowserInteractionSession::default()
            .admit(BrowserAction::Observe, Duration::from_secs(45),),
        Err(BrowserActionBlock::BudgetReached(
            BrowserActionLimit::Elapsed
        ))
    );
}

#[test]
fn close_is_terminal_and_handle_parser_never_accepts_selector_syntax() {
    for invalid in ["", "#login", "input[name=q]", "//button", "element 1"] {
        assert!(ElementHandle::parse(invalid).is_none());
    }
    assert!(ElementHandle::parse("element-7-2").is_some());

    let mut session = BrowserInteractionSession::default();
    assert_eq!(
        session.admit(BrowserAction::Close, Duration::ZERO),
        Ok(AdmittedBrowserAction::Close)
    );
    assert_eq!(
        session.admit(BrowserAction::Observe, Duration::ZERO),
        Err(BrowserActionBlock::Closed)
    );
}

#[test]
fn browser_keys_are_a_closed_runtime_enum() {
    let keys = [
        BrowserKey::Enter,
        BrowserKey::Escape,
        BrowserKey::Tab,
        BrowserKey::Backspace,
        BrowserKey::ArrowUp,
        BrowserKey::ArrowDown,
        BrowserKey::ArrowLeft,
        BrowserKey::ArrowRight,
    ];
    assert_eq!(keys.len(), 8);
}

#[test]
fn rejected_action_attempts_still_consume_the_session_budget() {
    let mut session = BrowserInteractionSession::default();
    for _ in 0..12 {
        assert_eq!(
            session.admit(
                BrowserAction::Navigate { url: String::new() },
                Duration::ZERO,
            ),
            Err(BrowserActionBlock::InvalidAction)
        );
    }
    assert_eq!(
        session.admit(BrowserAction::Observe, Duration::ZERO),
        Err(BrowserActionBlock::BudgetReached(
            BrowserActionLimit::Actions
        ))
    );
}
