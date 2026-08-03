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

#[test]
fn conversation_policy_keeps_short_answers_direct_and_proportional() {
    let policy = super::super::prompt_policy::assistant_and_answer_contract();
    let cue = super::super::prompt_policy::direct_answer_cue();

    assert!(policy.contains("한두 문장으로 답한다"));
    assert!(policy.contains("첫 문장을 반드시 평가 대상의 이름으로 시작"));
    assert!(policy.contains("구체적인 강점 하나와 한계 하나"));
    assert!(policy.contains("첫 문장을 가능 여부로 시작"));
    assert!(policy.contains("질문의 복잡도와 어조"));
    assert!(policy.contains("실제로 도움이 될 때만 사용"));
    assert!(!policy.contains("비교 질문에는 목적·근거·불확실성을 구분"));
    assert!(!policy.contains("감정이나 개인적 선호가 있는 척하지 마라"));
    assert!(cue.contains("`좋아?`나 `어때?`는 실용적 평가"));
    assert!(cue.contains("능력 질문은 가능 여부부터"));
}

#[test]
fn plain_reply_repeats_the_small_model_output_shape_next_to_the_answer_cue() {
    let prompt = super::super::reply::prompt::assemble_plain_prompt(
        "Gemma 좋아?",
        "Gemma 좋아?",
        &[],
        &[],
        131_072,
    )
    .unwrap();

    assert!(prompt.text.ends_with(
        "`좋아?`나 `어때?`는 실용적 평가로 답하고 대상 이름부터 구체적인 장점과 한계를 말한다. 능력 질문은 가능 여부부터 말한다.\n답변:"
    ));
}
