use super::*;

#[test]
fn configured_manifest_id_matches_its_backend_artifact_stem() {
    assert!(same_active_model(
        "gemma-4-e4b",
        Some("gemma-4-E4B_q4_0-it"),
        "gemma-4-E4B_q4_0-it"
    ));
    assert!(same_active_model(
        "qwen3.5-4b",
        Some("Qwen3.5-4B-Q4_K_M"),
        "Qwen3.5-4B-Q4_K_M"
    ));
    assert!(!same_active_model(
        "qwen3.5-4b",
        Some("Qwen3.5-4B-Q4_K_M"),
        "gemma-4-E4B_q4_0-it"
    ));
}

#[test]
fn retained_context_grows_for_success_and_runtime_error_turns() {
    use crate::surfaces::tui::runtime_bridge::{TuiConversationRole, TuiConversationTurn};

    let successful = vec![
        TuiConversationTurn {
            role: TuiConversationRole::User,
            content: "첫 질문".to_string(),
        },
        TuiConversationTurn {
            role: TuiConversationRole::Assistant,
            content: "첫 답변".to_string(),
        },
    ];
    let mut with_failure = successful.clone();
    with_failure.extend([
        TuiConversationTurn {
            role: TuiConversationRole::User,
            content: "검색해줘".to_string(),
        },
        TuiConversationTurn {
            role: TuiConversationRole::Error,
            content: "웹 검색 근거를 찾지 못했습니다.".to_string(),
        },
    ]);

    let input = super::super::super::attachment::compose_request("", &[], Some(262_144))
        .expect("empty request without attachments is a valid prompt projection");
    let first = super::super::super::conversation::estimate_context_tokens(
        "",
        &input,
        &successful,
        262_144,
    )
    .unwrap();
    let second = super::super::super::conversation::estimate_context_tokens(
        "",
        &input,
        &with_failure,
        262_144,
    )
    .unwrap();

    assert!(first > 0);
    assert!(second > first);
}

#[test]
fn backend_observation_precedes_prompt_projection() {
    assert_eq!(resolve_context_tokens(Some(777), Some(999)), Some(777));
    assert_eq!(resolve_context_tokens(None, Some(999)), Some(999));
    assert_eq!(resolve_context_tokens(None, None), None);
}
