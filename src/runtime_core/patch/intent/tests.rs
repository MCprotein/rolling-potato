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
