use super::super::*;
use crate::runtime_core::agent::AgentToolName;
use crate::runtime_core::inference::backend::ResponseLanguage;
use crate::surfaces::tui::runtime_bridge::{TuiConversationRole, TuiConversationTurn};

#[test]
fn history_only_secret_cannot_become_network_tool_input() {
    let history_secret = "HISTORY-ONLY-SECRET-42";
    let current_request = "2026년 월드컵 결과를 검색해서 알려줘";

    assert!(request_decision_from_agent_tool(
        structured_tool_call(AgentToolName::Search, history_secret),
        current_request,
        &[],
    )
    .is_none());
    assert!(matches!(
        request_decision_from_agent_tool(
            structured_tool_call(AgentToolName::Search, "2026년 월드컵 결과"),
            current_request,
            &[],
        ),
        Some(RequestDecision::WebTool(
            crate::app::web_search_adapter::WebToolRoute::Search { .. }
        ))
    ));
}

#[test]
fn structured_model_turn_routes_tools_and_visible_answers_without_text_protocols() {
    let tool_candidate = crate::app::inference_adapter::answer::GeneratedCandidate {
        response_language: ResponseLanguage::KoreanDefault,
        visible: r#"{"decision":"web_search","input":"2026년 월드컵 결과","answer":""}"#
            .to_string(),
    };
    assert!(matches!(
        decide_generated_candidate(
            tool_candidate,
            "2026년 월드컵 결과를 검색해서 알려줘",
            &[],
            true,
            true,
        )
        .unwrap(),
        RequestDecision::WebTool(crate::app::web_search_adapter::WebToolRoute::Search { .. })
    ));

    let answer_candidate = crate::app::inference_adapter::answer::GeneratedCandidate {
        response_language: ResponseLanguage::KoreanDefault,
        visible: r#"{"decision":"answer","input":"","answer":"대한민국의 수도는 서울입니다."}"#
            .to_string(),
    };
    assert_eq!(
        decide_generated_candidate(answer_candidate, "대한민국의 수도는?", &[], true, true)
            .unwrap(),
        RequestDecision::Answer("대한민국의 수도는 서울입니다.".to_string()),
    );
}

#[test]
fn valid_model_tool_choice_wins_before_the_grounding_safety_fallback() {
    let candidate = crate::app::inference_adapter::answer::GeneratedCandidate {
        response_language: ResponseLanguage::KoreanDefault,
        visible:
            r#"{"decision":"web_open","input":"https://blog.example.net/rust-release","answer":""}"#
                .to_string(),
    };

    assert_eq!(
        decide_generated_candidate(
            candidate,
            "현재 Rust stable 정보를 https://blog.example.net/rust-release 에서 확인해줘",
            &[],
            true,
            true,
        )
        .unwrap(),
        RequestDecision::WebTool(crate::app::web_search_adapter::WebToolRoute::Open {
            url: "https://blog.example.net/rust-release".to_string(),
        })
    );
}

#[test]
fn valid_model_answer_is_never_overridden_by_freshness_recovery() {
    for request in [
        "2026년 월드컵 우승국가 어디냐",
        "gemma vs qwen 성능 비교해봐",
        "현재 Rust stable 버전이 뭐야?",
    ] {
        let candidate = crate::app::inference_adapter::answer::GeneratedCandidate {
            response_language: ResponseLanguage::KoreanDefault,
            visible: r#"{"decision":"answer","input":"","answer":"근거 없는 답변"}"#.to_string(),
        };
        assert_eq!(
            decide_generated_candidate(candidate, request, &[], true, true).unwrap(),
            RequestDecision::Answer("근거 없는 답변".to_string()),
            "{request}"
        );
    }

    let offline_candidate = crate::app::inference_adapter::answer::GeneratedCandidate {
        response_language: ResponseLanguage::KoreanDefault,
        visible: r#"{"decision":"answer","input":"","answer":"오프라인 비교"}"#.to_string(),
    };
    assert_eq!(
        decide_generated_candidate(
            offline_candidate,
            "인터넷 없이 gemma vs qwen 성능 비교해봐",
            &[],
            false,
            true,
        )
        .unwrap(),
        RequestDecision::Answer("오프라인 비교".to_string())
    );
}

#[test]
fn malformed_or_invalid_model_tool_output_uses_freshness_recovery() {
    for visible in [
        "구조화되지 않은 답변",
        r#"{"decision":"web_search","input":"","answer":""}"#,
    ] {
        let candidate = crate::app::inference_adapter::answer::GeneratedCandidate {
            response_language: ResponseLanguage::KoreanDefault,
            visible: visible.to_string(),
        };
        assert!(matches!(
            decide_generated_candidate(
                candidate,
                "2026년 월드컵 우승국가 어디냐",
                &[],
                true,
                true,
            )
            .unwrap(),
            RequestDecision::WebTool(
                crate::app::web_search_adapter::WebToolRoute::Search { .. }
            )
        ));
    }
}

#[test]
fn malformed_structured_turns_never_become_visible_answers() {
    for visible in [
        "자유형 답변",
        "설명\n{\"decision\":\"answer\",\"input\":\"\",\"answer\":\"답\"}",
        "```json\n{\"decision\":\"answer\",\"input\":\"\",\"answer\":\"답\"}\n```",
        "{\"decision\":\"answer\",\"input\":\"\",\"answer\":\"잘린 답",
    ] {
        let candidate = crate::app::inference_adapter::answer::GeneratedCandidate {
            response_language: ResponseLanguage::KoreanDefault,
            visible: visible.to_string(),
        };
        assert_eq!(
            decide_generated_candidate(candidate, "일반 질문", &[], false, true).unwrap(),
            RequestDecision::ContinueLocal,
            "{visible}"
        );
    }
}

#[test]
fn short_conversational_followups_cannot_become_web_queries() {
    for request in ["?", "뭔데", "하고있는거야?", "뭐 하는 중이야?"] {
        assert!(
            request_decision_from_agent_tool(
                structured_tool_call(AgentToolName::Search, request),
                request,
                &[],
            )
            .is_none(),
            "{request}"
        );
    }
}

#[test]
fn world_cup_followup_keeps_user_context_and_never_routes_to_repo_map() {
    let history = vec![
        TuiConversationTurn {
            role: TuiConversationRole::User,
            content: "월드컵 우승국가가 어디야".to_string(),
        },
        TuiConversationTurn {
            role: TuiConversationRole::Assistant,
            content: "2022년 우승국은 아르헨티나입니다.".to_string(),
        },
        TuiConversationTurn {
            role: TuiConversationRole::User,
            content: "2026년은?".to_string(),
        },
        TuiConversationTurn {
            role: TuiConversationRole::Assistant,
            content: "검색이 필요합니다.".to_string(),
        },
    ];
    let prior = recent_user_requests(&history);
    let decision = request_decision_from_agent_tool(
        structured_tool_call(AgentToolName::Search, "2026 월드컵 우승 국가"),
        "검색해봐 끝낫어",
        &prior,
    );

    let Some(RequestDecision::WebTool(crate::app::web_search_adapter::WebToolRoute::Search {
        query,
    })) = decision
    else {
        panic!("contextual follow-up did not route to web search");
    };
    assert_eq!(query, "2026 월드컵 우승 국가");
    assert!(is_conversational_request("아니 우승국가 찾아보라고"));
}

#[test]
fn private_tool_protocol_never_becomes_an_offline_direct_answer() {
    let candidate = crate::app::inference_adapter::answer::GeneratedCandidate {
        response_language: ResponseLanguage::KoreanDefault,
        visible: "WEB INPUT: 월드컵 우승 국가".to_string(),
    };

    assert!(matches!(
        decide_generated_candidate(candidate, "인터넷 없이 알려줘", &[], false, true).unwrap(),
        RequestDecision::ContinueLocal
    ));
}
