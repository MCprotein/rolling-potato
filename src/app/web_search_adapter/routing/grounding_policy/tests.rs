use super::*;

#[test]
fn required_grounding_is_narrow_and_respects_local_scope() {
    for request in [
        "2026년 국제 대회 우승국가 어디냐",
        "alpha-model vs beta-model 성능 비교해봐",
        "현재 Rust stable 버전이 뭐야?",
        "최신 llama.cpp 릴리스를 알려줘",
    ] {
        assert!(requires_external_grounding(request), "{request}");
    }
    for request in [
        "대한민국의 수도는?",
        "두 모델의 이름만 비교해줘",
        "현재 파일의 두 함수 성능을 비교해줘",
        "이 저장소에서 최신 release 코드를 찾아줘",
    ] {
        assert!(!requires_external_grounding(request), "{request}");
    }
    assert!(requires_external_grounding(
        "이 코드 관련 최신 Rust API를 웹에서 검색해줘"
    ));
}

#[test]
fn query_strengthening_uses_generic_evidence_hints_only() {
    let outcome = strengthen_search_query(
        "2026년 국제 대회 우승국가",
        "2026년 국제 대회 우승국가 어디냐",
    );
    assert!(outcome.contains("2026년 국제 대회"), "{outcome}");
    assert!(outcome.contains("official"), "{outcome}");
    assert!(!outcome.contains("FIFA"), "{outcome}");

    let comparison = strengthen_search_query(
        "alpha-model vs beta-model 성능 비교",
        "alpha-model vs beta-model 성능 비교해봐",
    );
    assert!(comparison.contains("alpha-model"), "{comparison}");
    assert!(comparison.contains("beta-model"), "{comparison}");
    assert!(comparison.contains("benchmark"), "{comparison}");
    assert!(comparison.contains("methodology"), "{comparison}");
    for vendor in ["Google", "Alibaba"] {
        assert!(!comparison.contains(vendor), "{comparison}");
    }
}
