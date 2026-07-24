//! Deterministic request classification and request-constraint detection.

use crate::foundation::error::AppError;

use super::{IntentDecision, IntentSkill};

pub(crate) fn classify(
    request: &str,
    resolve_skill: impl FnOnce(&str) -> Option<IntentSkill>,
) -> Result<IntentDecision, AppError> {
    let trimmed = request.trim();
    if trimmed.is_empty() {
        return Err(AppError::usage("분류할 user request가 필요합니다."));
    }

    if let Some(skill_id) = explicit_skill(trimmed) {
        let Some(manifest) = resolve_skill(skill_id) else {
            return Err(AppError::usage(format!(
                "explicit skill을 찾지 못했습니다: {skill_id}"
            )));
        };

        return Ok(IntentDecision {
            skill_id: manifest.id,
            mode: manifest.mode,
            invocation: "explicit-skill",
            signals: vec!["explicit-invocation"],
            constraints: detect_constraints(trimmed),
            classifier: "deterministic-rules-only",
        });
    }

    let lower = trimmed.to_ascii_lowercase();
    let mut signals = Vec::new();
    let has_test_signal =
        has_any(&lower, &["test", "cargo test", "pytest"]) || has_any(trimmed, &["테스트"]);
    let has_failure_signal = has_any(&lower, &["failed", "failure", "panic", "error"])
        || has_any(trimmed, &["실패", "에러", "오류"]);
    let has_explanation_signal = has_any(&lower, &["explain", "why", "analyze"])
        || has_any(trimmed, &["설명", "왜", "분석", "원인"]);
    let has_diagnostic_output = has_any(
        &lower,
        &["error:", "error[", "panicked at", "traceback", "exception:"],
    ) || has_any(trimmed, &["에러 로그:", "오류 출력:", "예외:"]);
    let has_specific_failure_reference = has_any(
        &lower,
        &[
            "this error",
            "that error",
            "the error above",
            "error occurred",
            "error message",
            "failed because",
        ],
    ) || has_any(
        trimmed,
        &[
            "이 오류",
            "그 오류",
            "위 오류",
            "해당 오류",
            "이 에러",
            "그 에러",
            "실패했",
            "오류가 발생",
            "에러가 발생",
        ],
    );
    let has_change_signal = has_ascii_word(
        &lower,
        &[
            "fix",
            "change",
            "update",
            "edit",
            "implement",
            "refactor",
            "add",
            "remove",
            "delete",
            "create",
            "write",
            "apply",
            "patch",
            "proceed",
            "continue",
        ],
    ) || has_any(&lower, &["do it"])
        || has_any(
            trimmed,
            &[
                "고쳐",
                "수정",
                "변경",
                "바꿔",
                "바꾸",
                "구현",
                "리팩터",
                "리팩토링",
                "추가",
                "제거",
                "삭제",
                "만들어",
                "생성",
                "작성",
                "진행해",
                "계속해",
                "마저 진행",
                "이어서 진행",
                "이것 좀 해줘",
                "그거 해줘",
            ],
        );
    let (skill_id, mode) = if has_test_signal && has_failure_signal {
        signals.push("test-signal");
        ("fix-test", "execute")
    } else if has_any(&lower, &["review", "code review"]) || has_any(trimmed, &["리뷰", "검토"])
    {
        signals.push("review-only");
        ("code-review", "review-only")
    } else if has_any(&lower, &["plan", "roadmap"]) || has_any(trimmed, &["계획", "로드맵", "설계"])
    {
        signals.push("plan-only");
        ("ontology-refresh", "plan-only")
    } else if has_diagnostic_output || (has_specific_failure_reference && has_explanation_signal) {
        signals.push("explain-error");
        ("explain-error", "read-only")
    } else if has_any(&lower, &["map", "find", "search", "analyze"])
        || has_any(trimmed, &["찾아", "검색", "분석", "구조", "어디"])
    {
        signals.push("read-only");
        ("repo-map", "read-only")
    } else if has_change_signal {
        signals.push("small-patch-request");
        ("small-patch", "execute")
    } else {
        signals.push("conversation-default");
        ("conversation", "read-only")
    };

    if has_any(&lower, &["read-only", "no edit", "do not edit"])
        || has_any(trimmed, &["읽기만", "수정하지마", "건드리지마"])
    {
        signals.push("read-only-constraint");
    }

    if has_any(&lower, &["test spec", "acceptance criteria"])
        || has_any(trimmed, &["테스트 명세", "인수 기준"])
    {
        signals.push("test-spec");
    }

    if has_any(
        &lower,
        &["generate", "create file", "write doc", "make document"],
    ) || has_any(trimmed, &["문서 만들어", "파일 만들어", "생성해", "작성해"])
    {
        signals.push("generated-artifact");
    }

    Ok(IntentDecision {
        skill_id: skill_id.to_string(),
        mode,
        invocation: "deterministic-phrase",
        signals,
        constraints: detect_constraints(trimmed),
        classifier: "deterministic-rules-only; optional model classifier disabled",
    })
}

pub(crate) fn detect_constraints(request: &str) -> Vec<&'static str> {
    let lower = request.to_ascii_lowercase();
    let mut constraints = Vec::new();

    if has_any(&lower, &["no external contributor", "no pr"])
        || has_any(request, &["외부기여자", "외부 PR"])
    {
        constraints.push("no-external-contribution");
    }

    if has_any(&lower, &["korean", "hangul"]) || has_any(request, &["한국어", "한글"]) {
        constraints.push("korean-output");
    }

    if has_any(&lower, &["do not browse", "offline"]) || has_any(request, &["검색하지마"]) {
        constraints.push("no-network-retrieval");
    }

    constraints
}

fn explicit_skill(request: &str) -> Option<&str> {
    let rest = request.strip_prefix('$')?;
    let skill_id = rest.split_whitespace().next()?;
    if skill_id.is_empty() {
        None
    } else {
        Some(skill_id)
    }
}

pub(crate) fn has_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

fn has_ascii_word(text: &str, words: &[&str]) -> bool {
    text.split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .any(|token| words.contains(&token))
}
