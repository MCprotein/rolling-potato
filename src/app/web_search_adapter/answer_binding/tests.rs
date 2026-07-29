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
        "공식 릴리스를 알려줘",
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
        "두 근거를 요약해줘",
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
        "최신 릴리스를 알려줘",
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
    let rendered = render_grounded_answer(
        "최신 릴리스를 알려줘",
        None,
        None,
        std::slice::from_ref(&source),
    );

    assert!(rendered.contains("웹 검색은 완료했지만"));
    assert!(rendered.contains("검증된 출처"));
    assert!(rendered.contains("- [source-release] Release notes — https://example.com/releases/v1"));
}

#[test]
fn winner_questions_reject_cited_document_titles_that_do_not_answer_the_question() {
    let source = source(
        "source-results",
        "2026 tournament schedule and results",
        "https://example.com/results",
    );
    let rendered = render_grounded_answer(
        "2026년 월드컵 우승국가 어디냐",
        Some("검색 문서에는 대회 일정과 결과가 있습니다. [source-results]".to_string()),
        Some("열린 공식 원문에서 우승국을 확인할 수 없습니다. [source-results]".to_string()),
        std::slice::from_ref(&source),
    );

    assert!(!rendered.contains("대회 일정과 결과가 있습니다"));
    assert!(rendered.contains("우승국을 확인할 수 없습니다"));
}

#[test]
fn performance_comparisons_reject_off_topic_or_unfinished_generated_answers() {
    let source = source(
        "source-benchmark",
        "Gemma and Qwen benchmark",
        "https://example.com/benchmark",
    );
    for generated in [
        "두 모델은 Apache 라이선스입니다. [source-benchmark]",
        "주요 비교는 다음과 같습니다: [source-benchmark]",
    ] {
        let rendered = render_grounded_answer(
            "gemma vs qwen 성능 비교해봐",
            Some(generated.to_string()),
            Some(
                "검색된 근거만으로 성능 우열을 단정할 수 없습니다. [source-benchmark]".to_string(),
            ),
            std::slice::from_ref(&source),
        );
        assert!(!rendered.contains("Apache"), "{rendered}");
        assert!(!rendered.contains("다음과 같습니다:"), "{rendered}");
        assert!(rendered.contains("성능 우열을 단정할 수 없습니다"));
    }
}
