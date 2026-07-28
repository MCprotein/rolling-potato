use super::*;

#[test]
fn contextual_followup_query_uses_only_recent_user_requests() {
    let prior = ["월드컵 우승국가가 어디야", "2026년은?"];
    assert_eq!(
        contextualize_search_input("2026 월드컵 우승 국가", "검색해봐 끝낫어", &prior),
        Some("2026 월드컵 우승 국가".to_string())
    );
    assert!(
        contextualize_search_input("SECRET attachment value", "검색해봐 끝낫어", &prior).is_none()
    );
}

#[test]
fn raw_meta_search_request_is_rewritten_with_recent_topic() {
    let query = contextualize_search_input(
        "검색해봐 끝낫어",
        "검색해봐 끝낫어",
        &["월드컵 우승국가가 어디야", "2026년은?"],
    )
    .unwrap();

    assert!(query.contains("월드컵"));
    assert!(query.contains("2026"));
    assert!(!query.contains("끝낫어"));
    assert!(!query.contains("검색해봐"));
}

#[test]
fn unrelated_older_user_values_never_join_the_search_query() {
    let query = contextualize_search_input(
        "월드컵 우승 국가",
        "검색해봐 끝낫어",
        &[
            "내 고객 코드 ORION-42 기억해",
            "월드컵 우승국가가 어디야",
            "2026년은?",
        ],
    )
    .unwrap();

    assert_eq!(query, "2026 월드컵 우승 국가");
    assert!(!query.contains("ORION"));
    assert!(!query.contains("고객"));
}

#[test]
fn missing_year_qualifier_is_restored_from_the_relevant_followup() {
    assert_eq!(
        contextualize_search_input(
            "월드컵 우승 국가",
            "검색해봐 끝낫어",
            &["월드컵 우승국가가 어디야", "2026년은?"],
        ),
        Some("2026 월드컵 우승 국가".to_string())
    );
}

#[test]
fn correction_after_meta_search_keeps_only_the_relevant_chain() {
    let query = contextualize_search_input(
        "아니 우승국가 찾아보라고",
        "아니 우승국가 찾아보라고",
        &["월드컵 우승국가가 어디야", "2026년은?", "검색해봐 끝낫어"],
    )
    .unwrap();

    assert!(query.contains("월드컵"));
    assert!(query.contains("2026"));
    assert!(query.contains("우승국가"));
    assert!(!query.contains("검색해봐"));
}

#[test]
fn context_free_meta_search_is_not_sent_as_a_literal_query() {
    assert!(contextualize_search_input("검색해봐", "검색해봐", &[]).is_none());
    assert!(
        contextualize_search_input("검색해봐", "검색해봐", &["내 고객 코드 ORION-42 기억해"],)
            .is_none()
    );
}
