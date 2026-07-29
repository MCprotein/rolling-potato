use super::*;
use std::time::Duration;

fn search(query: &str) -> WebResearchStep {
    WebResearchStep::Search {
        query: query.to_string(),
    }
}

fn open(url: &str) -> WebResearchStep {
    WebResearchStep::Open {
        url: url.to_string(),
    }
}

fn find(query: &str) -> WebResearchStep {
    WebResearchStep::Find {
        query: query.to_string(),
    }
}

#[test]
fn default_budget_matches_the_v050_contract() {
    let budget = WebResearchBudget::default();

    assert_eq!(budget.max_steps, 6);
    assert_eq!(budget.max_searches, 2);
    assert_eq!(budget.max_opens, 3);
    assert_eq!(budget.max_query_revisions, 1);
    assert_eq!(budget.max_finds_per_document, 2);
    assert_eq!(budget.max_network_requests, 6);
    assert_eq!(budget.max_evidence_bytes, 8 * 1024);
    assert_eq!(budget.max_elapsed, Duration::from_secs(45));
    assert_eq!(budget.final_answer_tokens(), 768);
}

#[test]
fn routing_budget_stops_at_search_revision_and_document_find_limits() {
    let mut research = WebResearchSession::default();
    assert_eq!(
        research.admit(search("rust release"), None, Duration::ZERO),
        WebResearchAdmission::Execute(search("rust release"))
    );
    assert_eq!(
        research.admit(search("rust stable release"), None, Duration::ZERO),
        WebResearchAdmission::Execute(search("rust stable release"))
    );
    assert_eq!(
        research.admit(search("rust current"), None, Duration::ZERO),
        WebResearchAdmission::Stop(WebResearchTerminal::BudgetReached(
            WebResearchLimit::Searches
        ))
    );

    let mut research = WebResearchSession::default();
    assert!(matches!(
        research.admit(
            find("ownership"),
            Some("https://example.com/a"),
            Duration::ZERO
        ),
        WebResearchAdmission::Execute(_)
    ));
    assert!(matches!(
        research.admit(
            find("borrowing"),
            Some("https://example.com/a"),
            Duration::ZERO
        ),
        WebResearchAdmission::Execute(_)
    ));
    assert_eq!(
        research.admit(
            find("lifetimes"),
            Some("https://example.com/a"),
            Duration::ZERO
        ),
        WebResearchAdmission::Stop(WebResearchTerminal::BudgetReached(
            WebResearchLimit::FindsPerDocument
        ))
    );
}

#[test]
fn every_step_kind_has_an_independent_budget() {
    let revision_budget = WebResearchBudget {
        max_searches: 3,
        ..WebResearchBudget::default()
    };
    let mut revisions = WebResearchSession::new(revision_budget);
    assert!(matches!(
        revisions.admit(search("one"), None, Duration::ZERO),
        WebResearchAdmission::Execute(_)
    ));
    assert!(matches!(
        revisions.admit(search("two"), None, Duration::ZERO),
        WebResearchAdmission::Execute(_)
    ));
    assert_eq!(
        revisions.admit(search("three"), None, Duration::ZERO),
        WebResearchAdmission::Stop(WebResearchTerminal::BudgetReached(
            WebResearchLimit::QueryRevisions
        ))
    );

    let step_budget = WebResearchBudget {
        max_searches: 6,
        max_query_revisions: 6,
        max_opens: 6,
        max_network_requests: 12,
        ..WebResearchBudget::default()
    };
    let mut steps = WebResearchSession::new(step_budget);
    for query in ["one", "two", "three", "four", "five", "six"] {
        assert!(matches!(
            steps.admit(search(query), None, Duration::ZERO),
            WebResearchAdmission::Execute(_)
        ));
    }
    assert_eq!(
        steps.admit(open("https://example.com/seven"), None, Duration::ZERO),
        WebResearchAdmission::Stop(WebResearchTerminal::BudgetReached(WebResearchLimit::Steps))
    );

    let mut opens = WebResearchSession::default();
    for path in ["one", "two", "three"] {
        assert!(matches!(
            opens.admit(
                open(&format!("https://example.com/{path}")),
                None,
                Duration::ZERO
            ),
            WebResearchAdmission::Execute(_)
        ));
    }
    assert_eq!(
        opens.admit(open("https://example.com/four"), None, Duration::ZERO),
        WebResearchAdmission::Stop(WebResearchTerminal::BudgetReached(WebResearchLimit::Opens))
    );
}

#[test]
fn elapsed_limit_is_sticky_but_evidence_exhaustion_is_a_soft_boundary() {
    let mut elapsed = WebResearchSession::default();
    assert_eq!(
        elapsed.admit(search("rust"), None, Duration::from_secs(45)),
        WebResearchAdmission::Stop(WebResearchTerminal::BudgetReached(
            WebResearchLimit::Elapsed
        ))
    );
    assert_eq!(
        elapsed.admit(search("retry"), None, Duration::ZERO),
        WebResearchAdmission::Stop(WebResearchTerminal::BudgetReached(
            WebResearchLimit::Elapsed
        ))
    );

    let mut evidence = WebResearchSession::default();
    let exact = evidence.take_evidence(&"a".repeat(8 * 1024));
    assert_eq!(exact.len(), 8 * 1024);
    assert_eq!(evidence.take_evidence("b"), "");
    assert!(!evidence.has_evidence_capacity());
    assert!(matches!(
        evidence.admit(open("https://example.com"), None, Duration::ZERO),
        WebResearchAdmission::Execute(_)
    ));

    let mut multibyte = WebResearchSession::default();
    let bounded = multibyte.take_evidence(&"가".repeat(4_000));
    assert!(bounded.len() <= 8 * 1024);
    assert!(bounded.is_char_boundary(bounded.len()));
    assert!(multibyte.take_evidence("가").is_empty());
    assert!(
        !multibyte.has_evidence_capacity(),
        "UTF-8 code point보다 작은 잔여 byte는 소진된 예산으로 처리해야 합니다."
    );
}

#[test]
fn failed_input_retries_once_then_uses_a_deterministic_fallback() {
    let mut research = WebResearchSession::default();
    let failed = search("latest Rust");

    assert_eq!(
        research.record_failed_input(&failed),
        FailedInputAction::Retry
    );
    assert_eq!(
        research.record_failed_input(&failed),
        FailedInputAction::UseFallback
    );
    assert_eq!(
        research.deterministic_fallback("latest Rust", &[], Duration::ZERO),
        WebResearchAdmission::Execute(failed)
    );

    research.record_opened_document("https://example.com/already-open");
    let candidates = vec![
        "http://insecure.example".to_string(),
        "https://example.com/already-open".to_string(),
        "https://example.com/next".to_string(),
    ];
    assert_eq!(
        research.deterministic_fallback("latest Rust", &candidates, Duration::ZERO),
        WebResearchAdmission::Execute(open("https://example.com/next"))
    );
    research.record_opened_document("https://example.com/next");
    assert_eq!(
        research.deterministic_fallback("latest Rust", &candidates, Duration::ZERO),
        WebResearchAdmission::Stop(WebResearchTerminal::NoUsableEvidence)
    );
}

#[test]
fn freshness_fallback_is_bounded_and_respects_web_opt_out() {
    assert_eq!(
        deterministic_freshness_fallback("최신 Rust 릴리스를 찾아줘"),
        Some(search("최신 Rust 릴리스를 공식 official"))
    );
    assert!(
        deterministic_freshness_fallback("인터넷 검색하지 말고 최신 Rust 릴리스를 설명해줘")
            .is_none()
    );
    assert!(deterministic_freshness_fallback("현재 파일의 함수를 설명해줘").is_none());
    let long = format!("최신 {}", "가".repeat(600));
    let Some(WebResearchStep::Search { query }) = deterministic_freshness_fallback(&long) else {
        panic!("freshness query should route to search");
    };
    assert_eq!(query.chars().count(), types::MAX_TOOL_INPUT_CHARS);
}

#[test]
fn grounding_fallback_covers_generic_freshness_and_comparison_signals() {
    for request in [
        "2026년 국제 대회 결과가 뭐야",
        "alpha-model vs beta-model 성능 비교해봐",
        "현재 Rust stable 버전이 뭐야?",
    ] {
        let Some(WebResearchStep::Search { query }) = deterministic_freshness_fallback(request)
        else {
            panic!("required grounding request did not route to search: {request}");
        };
        assert!(
            query.contains("공식") || query.contains("official"),
            "{query}"
        );
    }

    for request in [
        "대한민국의 수도는?",
        "현재 파일의 함수를 설명해줘",
        "인터넷 없이 alpha-model vs beta-model 성능 비교해봐",
    ] {
        assert!(
            deterministic_freshness_fallback(request).is_none(),
            "{request}"
        );
    }
}

#[test]
fn freshness_fallback_resolves_meta_search_from_recent_user_topic() {
    let Some(WebResearchStep::Search { query }) = deterministic_freshness_fallback_for_context(
        "검색해봐 끝낫어",
        &["월드컵 우승국가가 어디야", "2026년은?"],
    ) else {
        panic!("contextual freshness request did not route to search");
    };

    assert!(query.contains("월드컵"));
    assert!(query.contains("2026"));
    assert!(!query.contains("검색해봐"));
}

#[test]
fn additional_network_requests_share_the_global_budget() {
    let budget = WebResearchBudget {
        max_steps: 8,
        max_searches: 8,
        max_query_revisions: 8,
        ..WebResearchBudget::default()
    };
    let mut research = WebResearchSession::new(budget);
    for query in ["one", "two", "three", "four", "five", "six"] {
        assert!(matches!(
            research.admit(search(query), None, Duration::ZERO),
            WebResearchAdmission::Execute(_)
        ));
    }
    assert_eq!(
        research.admit(search("seven"), None, Duration::ZERO),
        WebResearchAdmission::Stop(WebResearchTerminal::BudgetReached(
            WebResearchLimit::NetworkRequests
        ))
    );
}

#[test]
fn optional_search_fallback_reservation_stays_within_the_network_budget() {
    let budget = WebResearchBudget {
        max_network_requests: 2,
        ..WebResearchBudget::default()
    };
    let mut research = WebResearchSession::new(budget);

    assert!(matches!(
        research.admit(search("primary"), None, Duration::ZERO),
        WebResearchAdmission::Execute(_)
    ));
    assert!(research.reserve_optional_network_request(Duration::ZERO));
    assert!(!research.reserve_optional_network_request(Duration::ZERO));
}

#[test]
fn complete_is_an_explicit_terminal_state() {
    let mut research = WebResearchSession::default();
    assert_eq!(research.complete(), WebResearchTerminal::Complete);
    assert_eq!(
        research.admit(search("after completion"), None, Duration::ZERO),
        WebResearchAdmission::Stop(WebResearchTerminal::Complete)
    );
}
