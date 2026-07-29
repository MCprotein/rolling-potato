use super::super::*;
use crate::surfaces::tui::runtime_bridge::{TuiConversationRole, TuiConversationTurn};

#[test]
fn web_conversation_context_rejects_an_invalid_model_window() {
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

    let error =
        render_web_conversation_context(&history, "방금 오류가 무슨 뜻이야?", 1_024).unwrap_err();

    assert!(error
        .message
        .contains("context length가 prompt를 조립하기에 너무 작습니다"));
}
