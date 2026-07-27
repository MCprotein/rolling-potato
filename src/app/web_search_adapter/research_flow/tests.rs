use super::*;

#[test]
fn opened_primary_document_overrides_conflicting_search_snippet() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    std::env::set_var(
        "RPOTATO_TEST_WEB_SEARCH_HTML",
        r#"<html><body><div class="result results_links web-result">
            <h2 class="result__title"><a class="result__a" href="https://example.com/release">Official release</a></h2>
            <a class="result__snippet">SNIPPET-WRONG release claim</a>
        </div></body></html>"#,
    );
    std::env::set_var(
        "RPOTATO_TEST_WEB_OPEN_HTML",
        "<html><title>Official release</title><main>OFFICIAL-CORRECT release claim</main></html>",
    );
    std::env::set_var("RPOTATO_TEST_WEB_RESEARCH_NO_MODEL", "1");
    let mut research = WebResearchSession::default();
    let mut pages = WebPageSession::default();
    assert!(matches!(
        research.admit(
            WebToolRoute::Search {
                query: "official release".to_string(),
            },
            None,
            Duration::ZERO,
        ),
        WebResearchAdmission::Execute(_)
    ));

    let answer = super::answer(
        WebAnswerInput::new(
            "official release",
            "official release 검색해줘",
            "official release 검색해줘",
        ),
        &mut research,
        &mut pages,
        Duration::ZERO,
    )
    .unwrap();

    for name in [
        "RPOTATO_TEST_WEB_SEARCH_HTML",
        "RPOTATO_TEST_WEB_OPEN_HTML",
        "RPOTATO_TEST_WEB_RESEARCH_NO_MODEL",
    ] {
        std::env::remove_var(name);
    }
    assert!(answer.contains("OFFICIAL-CORRECT"));
    assert!(!answer.contains("SNIPPET-WRONG"));
    assert!(answer.contains("https://example.com/release"));
    assert_eq!(pages.len(), 1);
}

#[test]
fn long_korean_evidence_is_softly_truncated_and_still_returns_grounded_answer() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let long_snippet = "ESPR 관련 검색 문맥입니다. ".repeat(600);
    let search_html = format!(
        r#"<html><body>
            <div class="result results_links web-result">
                <h2 class="result__title"><a class="result__a" href="https://example.com/espr-primary">ESPR primary</a></h2>
                <a class="result__snippet">{long_snippet}</a>
            </div>
            <div class="result results_links web-result">
                <h2 class="result__title"><a class="result__a" href="https://example.com/espr-secondary">ESPR secondary</a></h2>
                <a class="result__snippet">{long_snippet}</a>
            </div>
            <div class="result results_links web-result">
                <h2 class="result__title"><a class="result__a" href="https://example.com/espr-tertiary">ESPR tertiary</a></h2>
                <a class="result__snippet">{long_snippet}</a>
            </div>
        </body></html>"#
    );
    std::env::set_var("RPOTATO_TEST_WEB_SEARCH_HTML", search_html);
    std::env::set_var(
        "RPOTATO_TEST_WEB_OPEN_HTML",
        format!(
            "<html><title>ESPR 원문</title><main>{}</main></html>",
            "ESPR 원문에서 확인한 설명입니다. ".repeat(600)
        ),
    );
    std::env::set_var("RPOTATO_TEST_WEB_RESEARCH_NO_MODEL", "1");
    let mut research = WebResearchSession::with_evidence_limit(1_024);
    let mut pages = WebPageSession::default();
    assert!(matches!(
        research.admit(
            WebToolRoute::Search {
                query: "ESPR".to_string(),
            },
            None,
            Duration::ZERO,
        ),
        WebResearchAdmission::Execute(_)
    ));

    let answer = super::answer(
        WebAnswerInput::new("ESPR", "ESPR이 뭔지 검색해봐", "ESPR이 뭔지 검색해봐")
            .with_conversation_context(
                r#"<RECENT_CONVERSATION>{"role":"runtime","content":"이전 검색은 근거 한도에서 중단됨"}</RECENT_CONVERSATION>"#,
            ),
        &mut research,
        &mut pages,
        Duration::ZERO,
    )
    .unwrap();

    for name in [
        "RPOTATO_TEST_WEB_SEARCH_HTML",
        "RPOTATO_TEST_WEB_OPEN_HTML",
        "RPOTATO_TEST_WEB_RESEARCH_NO_MODEL",
    ] {
        std::env::remove_var(name);
    }
    assert!(answer.contains("웹 검색은 완료했지만"), "{answer}");
    assert!(answer.contains("https://example.com/espr-primary"));
    assert!(!answer.contains("웹 근거 상한"));
    assert!(!research.has_evidence_capacity());
}

#[test]
fn final_web_prompt_keeps_prior_runtime_failure_context() {
    let input = WebAnswerInput::new("ESPR", "다시 검색해봐", "다시 검색해봐")
        .with_conversation_context(
            r#"<RECENT_CONVERSATION>{"role":"user","content":"ESPR이 뭔지 검색해봐"}{"role":"runtime","content":"이전 검색 실패"}</RECENT_CONVERSATION>"#,
        );

    let prompt = research_prompt(&input, "검색 문맥", &[]);

    assert!(prompt.contains(r#""role":"user","content":"ESPR이 뭔지 검색해봐""#));
    assert!(prompt.contains(r#""role":"runtime","content":"이전 검색 실패""#));
    assert!(prompt.contains("<CONVERSATION_CONTEXT untrusted=\"true\">"));
}

#[test]
fn supporting_find_uses_a_bounded_query_term() {
    assert_eq!(
        longest_query_term("Rust stable release 2026"),
        Some("release".to_string())
    );
    assert_eq!(longest_query_term("a 1"), None);
}
