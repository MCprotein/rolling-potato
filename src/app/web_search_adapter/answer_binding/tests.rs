use super::*;

fn source(id: &str, title: &str, url: &str) -> WebSourceEvidence {
    WebSourceEvidence {
        source_id: id.to_string(),
        title: title.to_string(),
        url: url.to_string(),
    }
}

#[test]
fn invalid_citations_and_model_urls_cannot_replace_runtime_sources() {
    let answer = render_grounded_answer(
        Some(
            "확인된 주장 [source-good](https://evil.example/swap). 가짜 [source-bad]. 숫자 [1], 배열 [1, 2], a[1]."
                .to_string(),
        ),
        None,
        &[source(
            "source-good",
            "Primary document",
            "https://example.com/verified",
        )],
    );

    assert!(answer.contains("[source-good]"));
    assert!(answer.contains("https://example.com/verified"));
    assert!(!answer.contains("source-bad"));
    assert!(!answer.contains("evil.example"));
    assert!(!answer.contains("숫자 [1]"));
    assert!(answer.contains("[1, 2]"));
    assert!(answer.contains("a[1]"));
}

#[test]
fn verified_sources_are_attached_to_the_paragraph_that_cites_them() {
    let answer = render_grounded_answer(
        Some("첫 주장 [source-one]\n\n둘째 주장은 불확실합니다 [source-two]".to_string()),
        None,
        &[
            source("source-one", "One", "https://example.com/one"),
            source("source-two", "Two", "https://example.com/two"),
        ],
    );

    assert!(
        answer.contains("첫 주장 [source-one]\n근거 · [source-one] One — https://example.com/one")
    );
    assert!(answer.contains(
        "둘째 주장은 불확실합니다 [source-two]\n근거 · [source-two] Two — https://example.com/two"
    ));
    assert!(!answer.contains("\n\n검증된 출처"));
}

#[test]
fn uncited_generated_answer_is_replaced_by_a_grounded_runtime_fallback() {
    let source = source(
        "source-release",
        "Release notes",
        "https://example.com/releases/v1",
    );
    let rendered = render_grounded_answer(
        Some("요약은 생성됐지만 marker가 없습니다.".to_string()),
        Some("열린 원문에서 확인한 내용입니다. [source-release]".to_string()),
        std::slice::from_ref(&source),
    );

    assert!(!rendered.contains("요약은 생성됐지만"));
    assert!(rendered.contains("열린 원문에서 확인한 내용입니다. [source-release]"));
    assert!(rendered
        .contains("근거 · [source-release] Release notes — https://example.com/releases/v1"));
}

#[test]
fn missing_grounded_candidates_show_only_runtime_owned_sources() {
    let source = source(
        "source-release",
        "Release notes",
        "https://example.com/releases/v1",
    );
    let rendered = render_grounded_answer(None, None, std::slice::from_ref(&source));

    assert!(rendered.contains("웹 검색은 완료했지만"));
    assert!(rendered.contains("검증된 출처"));
    assert!(rendered.contains("- [source-release] Release notes — https://example.com/releases/v1"));
}
