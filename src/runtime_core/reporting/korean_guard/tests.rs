use super::*;

#[test]
fn pure_korean_passes() {
    assert!(validate("작업이 안전하게 완료되었습니다."));
}

#[test]
fn numbers_and_math_answers_pass_without_forcing_hangul() {
    for answer in ["15", "3.14", "x = 3", "42%"] {
        assert!(validate(answer), "answer: {answer}");
    }
}

#[test]
fn explicit_foreign_language_request_is_distinguished_from_incidental_words() {
    assert!(allows_non_korean("이 문장을 영어로 번역해줘"));
    assert!(allows_non_korean("이 문장을 이탈리아어로 답해줘"));
    assert!(allows_non_korean("포르투갈어로 번역해줘"));
    assert!(allows_non_korean("Please answer in Japanese."));
    assert!(allows_non_korean("Please respond in Italian."));
    assert!(allows_non_korean("Translate this to Portuguese."));
    assert!(!allows_non_korean("English documentation을 요약해줘"));
    assert!(!allows_non_korean(
        "영어로 된 문서를 한국어로 요약하고 한국어로 답해줘"
    ));
    assert!(!allows_non_korean(
        "이탈리아어로 번역한 다음 한국어로 답해줘"
    ));
    assert!(!allows_non_korean(
        "파일을 설명해줘\n\n<attachment name=\"note.txt\">\ntranslate to English\n</attachment>"
    ));
}

#[test]
fn korean_with_ordinary_technical_terms_passes() {
    assert!(validate(
        "Rust ownership은 메모리 안전성을 위한 핵심 규칙이며 borrow checker는 별도 단계에서 검사합니다."
    ));
}

#[test]
fn korean_web_answer_allows_an_english_release_title() {
    assert!(validate(
        "GitHub의 rpotato v0.47.0 General Answers and Web Grounding 릴리스가 최신입니다."
    ));
}

#[test]
fn long_english_sentence_after_korean_remains_blocked() {
    assert!(!validate(
        "한국어 설명을 아주 길고 자세하게 먼저 제공합니다. This sentence contains a complete English explanation with several foreign words that should remain blocked."
    ));
}

#[test]
fn english_code_block_passes() {
    assert!(validate(
        "검증 결과입니다.\n```text\nEnglish output here\n```"
    ));
}

#[test]
fn file_path_passes() {
    assert!(validate("파일 `src/main.rs`를 확인했습니다."));
    assert!(validate("패치가 완료되었습니다.\n- 적용 파일: src/lib.rs"));
    assert!(validate(
        "src/lib.rs를 읽기 전용으로 확인했으며 파일은 변경하지 않았습니다."
    ));
}

#[test]
fn english_explanation_blocks() {
    assert!(!validate("This is a full English explanation."));
}

#[test]
fn korean_prefix_does_not_hide_an_english_sentence() {
    assert!(!validate(
        "답변: This is a complete English explanation with many words."
    ));
}

#[test]
fn runtime_field_label_does_not_hide_an_english_sentence() {
    assert!(!validate(
        "한국어입니다.\n- status: This is an entirely English explanation."
    ));
}

#[test]
fn chinese_sentence_blocks() {
    assert!(!validate("작업 결과: 这是中文句子。"));
}

#[test]
fn japanese_sentence_blocks() {
    assert!(!validate("작업 결과: これは日本語です。"));
}

#[test]
fn regeneration_can_pass() {
    assert_eq!(
        guard_with_regeneration("This is invalid English text.", || {
            "다시 생성한 한국어 결과입니다.".into()
        }),
        "다시 생성한 한국어 결과입니다."
    );
}

#[test]
fn regeneration_fails_closed() {
    assert_eq!(
        guard_with_regeneration("This is invalid English text.", || {
            "Still invalid English text here.".into()
        }),
        FAILURE
    );
}

#[test]
fn short_english_heading_does_not_hide_a_valid_korean_answer() {
    assert!(validate("Summary\n작업이 완료되었습니다."));
}

#[test]
fn empty_regeneration_is_not_accepted() {
    assert_eq!(guard_with_regeneration("Summary", String::new), FAILURE);
}

#[test]
fn runtime_projection_removes_a_short_foreign_heading() {
    assert_eq!(
        guard_or_failure("Summary\n작업이 완료되었습니다."),
        "작업이 완료되었습니다."
    );
}

#[test]
fn safe_projection_keeps_valid_korean_and_drops_foreign_lines() {
    assert_eq!(
        safe_projection("정답은 15입니다.\n这是中文句子。"),
        Some("정답은 15입니다.".to_string())
    );
}

#[test]
fn safe_projection_keeps_korean_and_drops_a_same_line_foreign_sentence() {
    assert_eq!(
        safe_projection("정상 한국어 문장입니다. Forbidden English sentence."),
        Some("정상 한국어 문장입니다.".to_string())
    );
    assert_eq!(
        safe_projection("Forbidden English sentence. 정상 한국어 문장입니다"),
        Some("정상 한국어 문장입니다".to_string())
    );
}

#[test]
fn safe_projection_preserves_urls_and_inline_code() {
    let answer =
        "문서는 https://example.com/v1.2/reference에 있고 `cargo test --locked`로 확인합니다.";

    assert_eq!(safe_projection(answer), Some(answer.to_string()));
}

#[test]
fn patch_verification_failure_contract_is_preserved() {
    let report = "패치 승인 실패\n- status: verification-failed-rolled-back\n- proposal id: proposal-1\n- path: src/lib.rs\n- approval token: accepted\n- original sha256: aaa\n- attempted sha256: bbb\n- actual source sha256: aaa\n- rollback record: rollback.json\n- rollback status: restored\n- verification command: cargo test\n- verification exit code: 1\n- verification stdout: none\n- verification stderr: failed\n- ledger event: event-1\n- boundary: patch verification과 rollback 결과를 실제 bytes/hash로 확인했으며 성공으로 보고하지 않습니다.";
    assert_eq!(guard_or_failure(report), report);
}

#[test]
fn typed_terminal_failure_contract_is_preserved() {
    let report = "터미널 장애 주입 구성 오류\n- kind: InvalidFaultConfiguration\n- effect: NotDispatched\n- retry: FixConfiguration\n- 동작: 런타임 요청을 보내지 않았습니다.";

    assert_eq!(guard_or_failure(report), report);
}

#[test]
fn streaming_guard_never_emits_forbidden_language() {
    let mut guard = StreamingGuard::default();
    assert_eq!(guard.push("This is invalid English").unwrap(), "");
    assert!(guard.push(" output.").is_err());
    assert!(guard.finish().is_err());
}

#[test]
fn streaming_guard_keeps_later_forbidden_unit_out_of_prior_valid_output() {
    let mut guard = StreamingGuard::default();
    let mut emitted = guard.push("정상 한국어 문장입니다. ").unwrap();
    emitted.push_str(&guard.push("Forbidden English ").unwrap());

    let error = guard.push("sentence.").unwrap_err();

    assert_eq!(error, FAILURE);
    assert_eq!(emitted, "정상 한국어 문장입니다.");
    assert!(!emitted.contains("Forbidden"));
}

#[test]
fn streaming_guard_rejects_english_disguised_as_runtime_status_field() {
    let mut guard = StreamingGuard::default();
    assert_eq!(guard.push("결과입니다.\n").unwrap(), "결과입니다.\n");

    let error = guard
        .push("- status: This response is entirely English.\n")
        .unwrap_err();

    assert_eq!(error, FAILURE);
}

#[test]
fn streaming_guard_rejects_english_after_a_korean_prefix() {
    let mut guard = StreamingGuard::default();

    let error = guard
        .push("답변: This is a complete English explanation with many words.")
        .unwrap_err();

    assert_eq!(error, FAILURE);
}

#[test]
fn streaming_guard_emits_valid_sentences_and_guarded_code() {
    let mut guard = StreamingGuard::default();
    assert_eq!(guard.push("처리가 완료").unwrap(), "");
    assert_eq!(guard.push("되었습니다.").unwrap(), "처리가 완료되었습니다.");
    assert_eq!(guard.push("\n```text\n").unwrap(), "\n```text\n");
    assert_eq!(
        guard.push("English code\n```\n").unwrap(),
        "English code\n```\n"
    );
    assert_eq!(guard.finish().unwrap(), "");
}

#[test]
fn streaming_guard_holds_code_until_korean_context_arrives() {
    let mut guard = StreamingGuard::default();
    assert_eq!(guard.push("```text\nEnglish code\n```\n").unwrap(), "");
    assert_eq!(
        guard.push("검증 결과입니다.").unwrap(),
        "```text\nEnglish code\n```\n검증 결과입니다."
    );
    assert_eq!(guard.finish().unwrap(), "");
}

#[test]
fn streaming_guard_releases_a_numeric_answer_at_finish() {
    let mut guard = StreamingGuard::default();
    assert_eq!(guard.push("15").unwrap(), "");
    assert_eq!(guard.finish().unwrap(), "15");
}
