//! Deterministic intent classification and side-effect-free action planning.

use crate::foundation::error::AppError;
use crate::runtime_core::knowledge::context::ContextPack;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentDecision {
    pub skill_id: String,
    pub mode: &'static str,
    pub invocation: &'static str,
    pub signals: Vec<&'static str>,
    pub constraints: Vec<&'static str>,
    pub classifier: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IntentSkill {
    pub id: String,
    pub mode: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ActionCandidate {
    pub kind: &'static str,
    pub approval_required: bool,
    pub next_gate: &'static str,
    pub allowed_side_effects: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedModelAction {
    pub status: &'static str,
    pub kind: String,
    pub source_pointers: String,
    pub next_gate: String,
    pub requested_side_effects: String,
    pub executable_now: bool,
    pub target_path: String,
    pub find_text: String,
    pub replace_text: String,
    pub verification_command: String,
}

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

pub(crate) fn display_list(values: &[&str]) -> String {
    if values.is_empty() {
        "없음".to_string()
    } else {
        values.join(", ")
    }
}

pub(crate) fn display_optional_u32(value: Option<u32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "없음".to_string())
}

pub(crate) fn display_bool(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

pub(crate) fn plan_action_candidate(
    decision: &IntentDecision,
    context_pack: &ContextPack,
) -> ActionCandidate {
    if decision.skill_id == "conversation" {
        return ActionCandidate {
            kind: "answer-only",
            approval_required: false,
            next_gate: "korean-output-guard",
            allowed_side_effects: "none",
        };
    }
    let has_context = !context_pack.source_pointers.is_empty();
    if matches!(decision.mode, "read-only" | "review-only" | "plan-only") {
        return ActionCandidate {
            kind: if has_context {
                "inspect-sources"
            } else {
                "answer-only"
            },
            approval_required: false,
            next_gate: "source-reread-before-claim",
            allowed_side_effects: "none",
        };
    }

    if decision.signals.contains(&"generated-artifact") {
        return ActionCandidate {
            kind: "generated-artifact-plan",
            approval_required: true,
            next_gate: "diff-before-write",
            allowed_side_effects: "none",
        };
    }

    if matches!(decision.skill_id.as_str(), "fix-test" | "small-patch") {
        return ActionCandidate {
            kind: "patch-proposal",
            approval_required: true,
            next_gate: "diff-before-write",
            allowed_side_effects: "none",
        };
    }

    ActionCandidate {
        kind: "answer-only",
        approval_required: false,
        next_gate: "korean-output-guard",
        allowed_side_effects: "none",
    }
}

pub(crate) fn parse_model_action(
    response: &str,
    runtime_candidate: &ActionCandidate,
    context_pack: &ContextPack,
) -> ParsedModelAction {
    let Some(fields) = parse_model_action_fields(response) else {
        if runtime_candidate.kind == "answer-only" {
            return fallback_model_action("runtime-owned-answer", runtime_candidate);
        }
        return parse_model_action_text(response, runtime_candidate, context_pack).unwrap_or_else(
            || fallback_model_action("missing-model-action-line", runtime_candidate),
        );
    };
    let raw_kind = field_value(&fields, &["kind"]).unwrap_or_default();
    let Some(parsed_kind) = normalize_model_action_kind(&raw_kind) else {
        return fallback_model_action("unknown-model-action-kind", runtime_candidate);
    };
    let raw_side_effects = field_value(&fields, &["side_effects", "allowed_side_effects"])
        .unwrap_or_else(|| runtime_candidate.allowed_side_effects.to_string());
    let side_effects = normalize_side_effects(&raw_side_effects);
    if side_effects != "none" {
        let mut blocked = fallback_model_action("blocked-side-effect-request", runtime_candidate);
        blocked.requested_side_effects = side_effects;
        return blocked;
    }
    if parsed_kind != runtime_candidate.kind {
        return fallback_model_action("mismatch-runtime-fallback", runtime_candidate);
    }

    let raw_source_pointers =
        field_value(&fields, &["source_pointers", "sources"]).unwrap_or_else(|| "none".to_string());
    let raw_next_gate = field_value(&fields, &["next_gate"])
        .unwrap_or_else(|| runtime_candidate.next_gate.to_string());

    ParsedModelAction {
        status: "parsed",
        kind: parsed_kind.to_string(),
        source_pointers: normalize_source_pointers(&raw_source_pointers, context_pack),
        next_gate: normalize_next_gate(&raw_next_gate, runtime_candidate),
        requested_side_effects: side_effects,
        executable_now: false,
        target_path: field_value(&fields, &["path", "target_path"]).unwrap_or_default(),
        find_text: decode_action_text(field_value(&fields, &["find_hex"]).as_deref()),
        replace_text: decode_action_text(field_value(&fields, &["replace_hex"]).as_deref()),
        verification_command: field_value(&fields, &["verification", "verification_command"])
            .unwrap_or_default(),
    }
}

fn parse_model_action_text(
    response: &str,
    runtime_candidate: &ActionCandidate,
    context_pack: &ContextPack,
) -> Option<ParsedModelAction> {
    let parsed_kind = normalize_model_action_kind(response)?;
    if parsed_kind != runtime_candidate.kind {
        return Some(fallback_model_action(
            "heuristic-runtime-fallback",
            runtime_candidate,
        ));
    }

    Some(ParsedModelAction {
        status: "heuristic-text",
        kind: parsed_kind.to_string(),
        source_pointers: source_pointers_from_text(response, context_pack),
        next_gate: runtime_candidate.next_gate.to_string(),
        requested_side_effects: runtime_candidate.allowed_side_effects.to_string(),
        executable_now: false,
        target_path: String::new(),
        find_text: String::new(),
        replace_text: String::new(),
        verification_command: String::new(),
    })
}

fn parse_model_action_fields(response: &str) -> Option<Vec<(String, String)>> {
    let line = response.lines().rev().find_map(model_action_body)?;
    let fields = line
        .split(';')
        .filter_map(|part| {
            let (key, value) = part.split_once('=')?;
            let key = key.trim().to_ascii_lowercase().replace('-', "_");
            let value = value.trim().to_string();
            if key.is_empty() {
                None
            } else {
                Some((key, value))
            }
        })
        .collect::<Vec<_>>();

    if fields.is_empty() {
        None
    } else {
        Some(fields)
    }
}

pub(crate) fn model_action_body(line: &str) -> Option<&str> {
    let trimmed = line
        .trim()
        .strip_prefix("- ")
        .unwrap_or_else(|| line.trim())
        .trim()
        .trim_matches('`');
    if let Some((prefix, body)) = trimmed.split_once(':') {
        let normalized_prefix = prefix.trim().to_ascii_lowercase();
        if normalized_prefix == "model action" || prefix.trim() == "모델액션" {
            return Some(body.trim());
        }
    }
    None
}

fn field_value(fields: &[(String, String)], names: &[&str]) -> Option<String> {
    fields
        .iter()
        .find(|(key, _)| names.iter().any(|name| key == name))
        .map(|(_, value)| value.clone())
}

fn normalize_model_action_kind(value: &str) -> Option<&'static str> {
    let lower = value.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return None;
    }
    if lower == "patch-proposal" || lower.contains("patch") || value.contains("패치") {
        Some("patch-proposal")
    } else if lower == "inspect-sources"
        || lower.contains("inspect")
        || lower.contains("source")
        || value.contains("소스")
        || value.contains("원본")
    {
        Some("inspect-sources")
    } else if lower == "generated-artifact-plan"
        || lower.contains("artifact")
        || lower.contains("generate")
        || value.contains("문서")
        || value.contains("생성")
    {
        Some("generated-artifact-plan")
    } else if lower == "answer-only" || lower.contains("answer") || value.contains("답변") {
        Some("answer-only")
    } else {
        None
    }
}

fn normalize_source_pointers(value: &str, context_pack: &ContextPack) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("none")
        || trimmed == "없음"
        || trimmed == "-"
    {
        return "none".to_string();
    }

    let verified = trimmed
        .split(',')
        .map(str::trim)
        .filter(|pointer| {
            context_pack
                .source_pointers
                .iter()
                .any(|source| source.stable_ref == *pointer)
        })
        .take(4)
        .map(str::to_string)
        .collect::<Vec<_>>();

    if verified.is_empty() {
        "unverified".to_string()
    } else {
        verified.join(", ")
    }
}

fn source_pointers_from_text(response: &str, context_pack: &ContextPack) -> String {
    let pointers = context_pack
        .source_pointers
        .iter()
        .filter(|source| response.contains(&source.stable_ref))
        .take(4)
        .map(|source| source.stable_ref.clone())
        .collect::<Vec<_>>();

    if pointers.is_empty() {
        "none".to_string()
    } else {
        pointers.join(", ")
    }
}

fn normalize_next_gate(value: &str, runtime_candidate: &ActionCandidate) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "-" {
        runtime_candidate.next_gate.to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_side_effects(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches('.');
    if trimmed.eq_ignore_ascii_case("none") || trimmed == "없음" || trimmed == "-" {
        "none".to_string()
    } else {
        trimmed.to_string()
    }
}

fn fallback_model_action(
    status: &'static str,
    runtime_candidate: &ActionCandidate,
) -> ParsedModelAction {
    ParsedModelAction {
        status,
        kind: runtime_candidate.kind.to_string(),
        source_pointers: "none".to_string(),
        next_gate: runtime_candidate.next_gate.to_string(),
        requested_side_effects: runtime_candidate.allowed_side_effects.to_string(),
        executable_now: false,
        target_path: String::new(),
        find_text: String::new(),
        replace_text: String::new(),
        verification_command: String::new(),
    }
}

fn decode_action_text(value: Option<&str>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    if !value.len().is_multiple_of(2) {
        return String::new();
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for pair in value.as_bytes().chunks_exact(2) {
        let Ok(hex) = std::str::from_utf8(pair) else {
            return String::new();
        };
        let Ok(byte) = u8::from_str_radix(hex, 16) else {
            return String::new();
        };
        bytes.push(byte);
    }
    String::from_utf8(bytes).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::runtime_core::knowledge::context::SourcePointer;

    #[test]
    fn classification_and_planning_are_deterministic() {
        let decision = classify("테스트 실패 고쳐줘", |_| None).unwrap();
        let candidate = plan_action_candidate(&decision, &empty_context());

        assert_eq!(decision.skill_id, "fix-test");
        assert_eq!(candidate.kind, "patch-proposal");
        assert!(candidate.approval_required);
        assert_eq!(candidate.allowed_side_effects, "none");
        assert_eq!(candidate.next_gate, "diff-before-write");
    }

    #[test]
    fn model_action_cannot_enable_requested_side_effects() {
        let decision = classify("작은 수정 해줘", |_| None).unwrap();
        let context = empty_context();
        let candidate = plan_action_candidate(&decision, &context);

        let action = parse_model_action(
            "MODEL ACTION: kind=patch-proposal; source_pointers=none; next_gate=diff-before-write; side_effects=write-file",
            &candidate,
            &context,
        );

        assert_eq!(action.status, "blocked-side-effect-request");
        assert_eq!(action.kind, "patch-proposal");
        assert_eq!(action.requested_side_effects, "write-file");
        assert!(!action.executable_now);
    }

    #[test]
    fn casual_conversation_defaults_to_non_mutating_answer() {
        let decision = classify("안녕", |_| None).unwrap();
        let mut context = empty_context();
        context.source_pointers.push(SourcePointer {
            path: "README.md".to_string(),
            stable_ref: "README.md:1".to_string(),
            chars: 8,
            fingerprint: "a".repeat(64),
            snippet: "# project".to_string(),
        });
        let candidate = plan_action_candidate(&decision, &context);

        assert_eq!(decision.skill_id, "conversation");
        assert_eq!(decision.mode, "read-only");
        assert_eq!(candidate.kind, "answer-only");
        assert!(!candidate.approval_required);
    }

    #[test]
    fn general_explanations_do_not_require_error_context() {
        for request in [
            "5 * 3은? 숫자만 답하지 말고 짧게 한국어로 설명해줘.",
            "왜 하늘은 파란가요?",
            "Rust ownership을 쉽게 설명해줘",
            "What is an error?",
            "Explain error handling in Rust",
            "Rust의 오류 처리를 설명해줘",
        ] {
            let decision = classify(request, |_| None).unwrap();
            assert_eq!(decision.skill_id, "conversation", "request: {request}");
        }

        for request in [
            "error: mismatched types\n왜 실패했는지 설명해줘",
            "이 오류를 분석해줘",
        ] {
            let decision = classify(request, |_| None).unwrap();
            assert_eq!(decision.skill_id, "explain-error", "request: {request}");
        }
    }

    #[test]
    fn explicit_changes_and_read_only_search_keep_their_routes() {
        for request in [
            "fix",
            "apply this patch",
            "이것 좀 해줘",
            "상수 값을 안전한 값으로 바꿔줘",
        ] {
            let decision = classify(request, |_| None).unwrap();
            assert_eq!(decision.skill_id, "small-patch", "request: {request}");
        }

        let search = classify("검색해라", |_| None).unwrap();
        assert_eq!(search.skill_id, "repo-map");
        assert_eq!(search.mode, "read-only");
    }

    #[test]
    fn plain_conversation_response_uses_runtime_owned_answer_action() {
        let decision = classify("안녕", |_| None).unwrap();
        let context = empty_context();
        let candidate = plan_action_candidate(&decision, &context);

        let action = parse_model_action("안녕하세요! 무엇을 도와드릴까요?", &candidate, &context);

        assert_eq!(action.status, "runtime-owned-answer");
        assert_eq!(action.kind, "answer-only");
        assert_eq!(action.requested_side_effects, "none");

        let hostile = parse_model_action(
            "MODEL ACTION: kind=answer-only; side_effects=write-file",
            &candidate,
            &context,
        );
        assert_eq!(hostile.status, "blocked-side-effect-request");
        assert_eq!(hostile.requested_side_effects, "write-file");
    }

    fn empty_context() -> ContextPack {
        ContextPack {
            project_root: PathBuf::from("/tmp/project"),
            origin: "test".to_string(),
            ontology_records_selected: 0,
            ontology_stale_rejected: 0,
            files_considered: 0,
            files_read: 0,
            chars_read: 0,
            dropped_files: 0,
            source_pointers: Vec::new(),
        }
    }
}
