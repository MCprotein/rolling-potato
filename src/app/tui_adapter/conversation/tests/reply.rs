use super::super::*;
use crate::surfaces::tui::runtime_bridge::{TuiConversationRole, TuiConversationTurn};

#[test]
fn web_conversation_context_uses_a_model_relative_output_reserve() {
    let history = vec![
        TuiConversationTurn {
            role: TuiConversationRole::User,
            content: "ESPR이 뭔지 검색해줘".to_string(),
        },
        TuiConversationTurn {
            role: TuiConversationRole::Error,
            content: "웹 검색 근거를 찾지 못했습니다.".to_string(),
        },
    ];

    let context =
        render_web_conversation_context(&history, &[], "방금 오류가 무슨 뜻이야?", 1_024).unwrap();

    assert!(context.contains("ESPR이 뭔지 검색해줘"));
    assert!(context.contains("웹 검색 근거를 찾지 못했습니다."));
}

#[test]
fn web_conversation_context_rejects_a_zero_model_window() {
    let error = render_web_conversation_context(&[], &[], "질문", 0).unwrap_err();

    assert!(error
        .message
        .contains("context length가 prompt를 조립하기에 너무 작습니다"));
}
