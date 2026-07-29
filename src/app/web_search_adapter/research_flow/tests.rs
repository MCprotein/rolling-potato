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
    assert!(answer.response.contains("OFFICIAL-CORRECT"));
    assert!(!answer.response.contains("SNIPPET-WRONG"));
    assert!(answer.response.contains("https://example.com/release"));
    assert_eq!(answer.grounding.len(), 1);
    assert!(answer.grounding[0].excerpt.contains("OFFICIAL-CORRECT"));
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
    assert!(
        answer.response.contains("웹 검색은 완료했지만"),
        "{answer:?}"
    );
    assert!(answer.response.contains("https://example.com/espr-primary"));
    assert!(!answer.response.contains("웹 근거 상한"));
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
fn final_web_prompt_forbids_turning_predictions_into_completed_results() {
    let input = WebAnswerInput::new(
        "2026 FIFA 월드컵 우승 공식 결과",
        "2026년 월드컵 우승국가 어디냐",
        "2026년 월드컵 우승국가 어디냐",
    );

    let prompt = research_prompt(
        &input,
        "Source ID: source-blog\nDescription: 예상 우승국 전망",
        &[],
    );

    assert!(prompt.contains("예상·전망·예측"));
    assert!(prompt.contains("실제 결과 근거로 사용하지"));
    assert!(prompt.contains("후보 결과를 나열하거나 반복하지 말고"));
    assert!(prompt.contains("출처끼리 결과가 충돌"));
    assert!(prompt.contains("첫 문장은 사용자가 요구한 값에 대한 직접 답"));
    assert!(prompt.contains("검색 문서의 제목이나 범위를 답으로 대신하지"));
    assert!(prompt.contains("사용자가 묻지 않은 라이선스"));
    assert!(prompt.contains("완결된 문장"));
}

#[test]
fn supporting_find_uses_a_bounded_query_term() {
    assert_eq!(
        supporting_query_term("Rust stable release 2026"),
        Some("release".to_string())
    );
    assert_eq!(
        supporting_query_term("2026년 월드컵 우승국가 FIFA World Cup 공식 official result"),
        Some("우승".to_string())
    );
    assert_eq!(supporting_query_term("a 1"), None);
}

#[test]
fn cached_grounding_answers_referential_followups_without_new_network_access() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    std::env::set_var("RPOTATO_TEST_WEB_RESEARCH_NO_MODEL", "1");
    let grounding = vec![
        WebGroundingEvidence {
            source_id: "source-purpose".to_string(),
            title: "EU 에코디자인 규정(ESPR)".to_string(),
            url: "https://example.com/espr-purpose".to_string(),
            excerpt: "제도 개요\n순환경제 전환 촉진과 지속가능 제품 설계를 위한 강제력 있는 법적 기반 마련\n적용 대상".to_string(),
        },
        WebGroundingEvidence {
            source_id: "source-name".to_string(),
            title: "EU 에코디자인 규정(Ecodesign for Sustainable Products Regulation, ESPR)".to_string(),
            url: "https://example.com/espr-name".to_string(),
            excerpt: "2024년 7월 &ldquo;ESPR&rdquo;이 발효되었습니다. ESPR은 EU 역내 출시 제품의 지속가능성과 순환성을 제품 단계부터 관리하는 법적 틀입니다. EU는 제품 수명 연장과 재활용 원료 확대를 목표로 합니다.".to_string(),
        },
    ];

    let answer = answer_from_grounding(
        "방금 검색한 ESPR의 정식 영문명과 핵심 목적을 한 문장으로 말해줘. 내 이름도 불러줘.",
        r#"<RECENT_CONVERSATION>{"role":"user","content":"내 이름은 고구마야. 기억해줘."}{"role":"model","content":"고구마라는 이름을 기억했습니다."}</RECENT_CONVERSATION>"#,
        &grounding,
    )
    .unwrap();

    std::env::remove_var("RPOTATO_TEST_WEB_RESEARCH_NO_MODEL");
    assert!(
        answer.contains("Ecodesign for Sustainable Products Regulation"),
        "{answer}"
    );
    assert!(
        answer.contains("제품 수명 연장과 재활용 원료 확대를 목표"),
        "{answer}"
    );
    assert!(answer.contains("고구마님"), "{answer}");
    assert!(answer.contains("[source-name]"));
    assert!(answer.contains("https://example.com/espr-name"));
    assert!(!answer.contains("이전 검색에서 보존한 원문 내용입니다."));
    assert!(!answer.contains("&ldquo;"), "{answer}");
    assert!(!answer.contains("2024년 7월"), "{answer}");
}
