use super::*;

#[test]
fn external_web_steps_reject_credential_like_current_input() {
    for step in [
        WebResearchStep::Search {
            query: "latest release api_key=SECRET-123".to_string(),
        },
        WebResearchStep::Search {
            query: "Authorization: Bearer eyJhbGciOiJIUzI1NiJ9".to_string(),
        },
        WebResearchStep::Search {
            query: "search sk-1234567890abcdef".to_string(),
        },
        WebResearchStep::Open {
            url: "https://example.com/?access_token=SECRET".to_string(),
        },
    ] {
        let error = validate_public_web_step(step).unwrap_err();
        assert!(error.message.contains("외부 요청을 차단"));
        assert!(!error.message.contains("SECRET"));
    }
}

#[test]
fn ordinary_public_search_terms_and_page_find_are_allowed() {
    assert!(validate_public_web_step(WebResearchStep::Search {
        query: "OAuth access token 보안 모범 사례".to_string(),
    })
    .is_ok());
    assert!(validate_public_web_step(WebResearchStep::Find {
        query: "password policy".to_string(),
    })
    .is_ok());
}

#[test]
fn only_explicitly_referential_questions_use_prior_web_grounding() {
    for request in [
        "방금 검색한 ESPR의 정식 명칭은?",
        "그 출처에서 핵심 목적을 다시 설명해줘",
        "ESPR 정식 영문명과 목적을 근거에 맞춰 다시 답해줘",
        "What did you just search?",
    ] {
        assert!(is_grounded_followup_request(request), "{request}");
    }
    for request in ["오늘 날씨 검색해줘", "내 이름이 뭐였지?", "Rust를 설명해줘"]
    {
        assert!(!is_grounded_followup_request(request), "{request}");
    }
}

#[test]
fn natural_regrounding_requires_topic_overlap_with_cached_evidence() {
    let grounding = vec![WebGroundingEvidence {
        source_id: "source-espr".to_string(),
        title: "Ecodesign for Sustainable Products Regulation (ESPR)".to_string(),
        url: "https://example.com/espr".to_string(),
        excerpt: "ESPR은 지속가능한 제품을 위한 EU 규정입니다.".to_string(),
    }];

    assert!(can_reuse_prior_grounding(
        "ESPR 정식 영문명과 목적을 근거에 맞춰 다시 답해줘",
        &grounding
    ));
    assert!(!can_reuse_prior_grounding(
        "Rust 소유권을 근거에 맞춰 다시 설명해줘",
        &grounding
    ));
    assert!(can_reuse_prior_grounding(
        "그 출처에서 목적을 다시 설명해줘",
        &grounding
    ));
}

#[test]
fn current_page_find_intent_precedes_a_new_public_search() {
    assert_eq!(
        route_current_page_find("이 페이지에서 Rust 찾아줘", true),
        Some(WebResearchStep::Find {
            query: "rust".to_string(),
        })
    );
    assert_eq!(
        route_current_page_find("Find checksum on this page", true),
        Some(WebResearchStep::Find {
            query: "checksum".to_string(),
        })
    );
    assert!(route_current_page_find("이 페이지에서 Rust 찾아줘", false).is_none());
    assert!(route_current_page_find("Rust 최신 릴리스를 찾아줘", true).is_none());
    assert!(route_current_page_find("이 페이지를 요약해줘", true).is_none());
}
