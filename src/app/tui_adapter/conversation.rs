//! Non-mutating conversation path for general questions that do not need agent tools.

mod decision;
mod local_facts;
mod presentation;
mod reply;

pub(super) use decision::{decide_request, RequestDecision};
pub(super) use local_facts::{is_conversational_request, local_reply};
pub(super) use presentation::{ensure_public_answer, present_agent_report};
pub(super) use reply::{render_web_conversation_context, reply_with_context, reply_with_images};

#[cfg(test)]
use decision::{
    decide_generated_candidate, recent_user_requests, request_decision_from_agent_tool,
    structured_tool_call,
};
#[cfg(test)]
use presentation::contains_private_tool_protocol;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_core::inference::backend::ResponseLanguage;
    use crate::surfaces::tui::runtime_bridge::{
        TuiConversationRole, TuiConversationTurn, TuiVisionStatus,
    };

    #[test]
    fn general_questions_use_conversation_without_stealing_agent_tasks() {
        for request in [
            "안녕",
            "안녕하세요!",
            "고마워",
            "뭐 할 수 있어?",
            "hello",
            "넌 무슨모델이니",
            "넌누구니?",
            "대한민국의 수도는?",
            "5 * 3은?",
            "Rust ownership을 쉽게 설명해줘",
            "What was the Manhattan Project?",
            "What is a profile?",
            "What is research?",
            "월드컵 우승국가 찾아봐",
            "아니 우승국가 찾아보라고",
            "경제 전망을 분석해줘",
        ] {
            assert!(is_conversational_request(request), "{request}");
        }
        for request in [
            "안녕, 이 코드 고쳐줘",
            "src/main.rs 수정해줘",
            "이 오류를 분석해줘",
            "테스트를 실행해줘",
            "이 저장소 구조를 알려줘",
            "이 저장소에서 함수를 찾아줘",
            "this crashes on startup",
            "they need help with startup",
        ] {
            assert!(!is_conversational_request(request), "{request}");
        }
    }

    #[test]
    fn model_and_agent_identity_questions_return_local_facts_without_a_workflow() {
        for request in [
            "넌 무슨모델이니",
            "모델 뭐쓰냐",
            "무슨 모델인지도 몰라?",
            "너 지금 qwen 이잖아",
            "지금 어떤 모델 쓰고 있어?",
        ] {
            assert_eq!(
                local_reply(request, Some("gemma-test"), TuiVisionStatus::OnDemand),
                Some("현재 사용 중인 모델은 gemma-test입니다.".to_string()),
                "{request}"
            );
        }
        assert_eq!(
            local_reply(
                "모델 뭐 추천해?",
                Some("gemma-test"),
                TuiVisionStatus::OnDemand
            ),
            None
        );
        assert_eq!(
            local_reply("넌누구니?", Some("ignored"), TuiVisionStatus::OnDemand),
            Some("저는 로컬에서 실행되는 범용 AI·코딩 에이전트 rpotato입니다.".to_string())
        );
        for contextual_followup in ["내 이름이 뭐였지?", "이름이뭔데", "그 사람 누구야?"]
        {
            assert_eq!(
                local_reply(
                    contextual_followup,
                    Some("ignored"),
                    TuiVisionStatus::OnDemand
                ),
                None,
                "{contextual_followup}는 대화 문맥을 모델에 전달해야 합니다."
            );
        }
        for contextual_second_person in [
            "너 이름 전에 감자라고 정했는데 기억해?",
            "아까 네 이름이 뭐라고 했지?",
        ] {
            assert_eq!(
                local_reply(
                    contextual_second_person,
                    Some("ignored"),
                    TuiVisionStatus::OnDemand
                ),
                None,
                "{contextual_second_person}는 직접 정체성 질문이 아닙니다."
            );
        }
        assert_eq!(
            local_reply(
                "이 모델 코드를 수정해줘",
                Some("gemma-test"),
                TuiVisionStatus::OnDemand
            ),
            None
        );
        assert_eq!(
            local_reply(
                "내가 전에 어떤 모델을 좋아한다고 했지?",
                Some("gemma-test"),
                TuiVisionStatus::OnDemand
            ),
            None
        );
        assert_eq!(
            local_reply(
                "Please answer in English: which model are you using?",
                Some("gemma-test"),
                TuiVisionStatus::OnDemand
            ),
            None
        );
    }

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

        let error = render_web_conversation_context(&history, "방금 오류가 무슨 뜻이야?", 1_024)
            .unwrap_err();

        assert!(error
            .message
            .contains("context length가 prompt를 조립하기에 너무 작습니다"));
    }

    #[test]
    fn vision_status_questions_use_runtime_facts_instead_of_model_guessing() {
        let reply = local_reply(
            "비전 왜 text-only임?",
            Some("qwen3.5-4b"),
            TuiVisionStatus::OnDemand,
        )
        .unwrap();

        assert!(reply.contains("이미지 입력을 지원합니다"));
        assert!(reply.contains("미지원이 아니라"));
        assert!(reply.contains("projector"));
        assert!(!reply.contains("비전 모드를 지원하지"));
        assert!(local_reply(
            "현재 모델은 비전 지원돼?",
            Some("qwen3.5-4b"),
            TuiVisionStatus::OnDemand,
        )
        .unwrap()
        .contains("이미지 입력을 지원합니다"));
    }

    #[test]
    fn vision_status_reply_does_not_intercept_agent_tasks() {
        for request in [
            "이미지 지원 코드를 수정해줘",
            "비전 사용이 가능하도록 구현해줘",
            "비전 버그를 분석해줘",
            "이미지 입력 테스트를 실행해줘",
        ] {
            assert_eq!(
                local_reply(request, Some("qwen3.5-4b"), TuiVisionStatus::OnDemand),
                None,
                "{request}"
            );
        }
    }

    #[test]
    fn agent_reports_collapse_to_visible_answer_or_reviewable_patch_summary() {
        let answer = present_agent_report(
            "run 결과\n- 상태: 완료\n- workflow id: workflow-read\n- 답변:\n원인은 설정 누락입니다.",
        );
        assert_eq!(answer, "원인은 설정 누락입니다.");

        let proposal = present_agent_report(
            "run agent loop\n- status: pending-approval\n- workflow id: workflow-one\n- proposal id: proposal-one\n- approval command: rpotato patch approve proposal-one --token secret\n- diff:\n--- a/src/main.rs\n+++ b/src/main.rs",
        );
        assert!(proposal.starts_with("변경 제안을 준비했습니다."));
        assert!(proposal.contains("workflow: workflow-one"));
        assert!(proposal.contains("--- a/src/main.rs"));
        assert!(proposal.contains("select workflow-one → approve proposal-one"));
        assert!(!proposal.contains("resource governor"));

        let failure = present_agent_report(
            "패치 제안 실패\n- workflow id: workflow-secret\n- 이유: backend-call-failed\n- 성공 보고: 차단",
        );
        assert!(failure.starts_with("모델 응답을 받지 못했습니다."));
        assert!(!failure.contains("workflow-secret"));
        assert!(!failure.contains("backend-call-failed"));
    }

    #[test]
    fn history_only_secret_cannot_become_network_tool_input() {
        use crate::runtime_core::agent::AgentToolName;

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
    fn required_external_grounding_overrides_a_small_model_direct_answer() {
        for request in [
            "2026년 월드컵 우승국가 어디냐",
            "gemma vs qwen 성능 비교해봐",
            "현재 Rust stable 버전이 뭐야?",
        ] {
            let candidate = crate::app::inference_adapter::answer::GeneratedCandidate {
                response_language: ResponseLanguage::KoreanDefault,
                visible: r#"{"decision":"answer","input":"","answer":"근거 없는 답변"}"#
                    .to_string(),
            };
            assert!(
                matches!(
                    decide_generated_candidate(candidate, request, &[], true, true).unwrap(),
                    RequestDecision::WebTool(
                        crate::app::web_search_adapter::WebToolRoute::Search { .. }
                    )
                ),
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
        use crate::runtime_core::agent::AgentToolName;

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
        use crate::runtime_core::agent::AgentToolName;

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
    fn malformed_private_tool_protocol_is_never_presented_as_an_answer() {
        for candidate in [
            "WEB INPUT: 월드컵 우승 국가",
            "WEBTool: search\nWEBINPUT: 월드컵 우승 국가",
            "browser url: https://example.com",
        ] {
            assert!(contains_private_tool_protocol(candidate), "{candidate}");
        }
        assert!(!contains_private_tool_protocol(
            "웹 검색 결과를 바탕으로 답변합니다."
        ));
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

    #[test]
    fn repeated_private_tool_protocol_is_rejected_at_the_presentation_boundary() {
        let error = ensure_public_answer("WEBTool: search\nWEBINPUT: 월드컵 우승 국가".to_string())
            .unwrap_err();

        assert!(error.message.contains("내부 도구 요청을 반복"));
        assert!(!error.message.contains("월드컵 우승 국가"));
        assert_eq!(
            ensure_public_answer("대한민국의 수도는 서울입니다.".to_string()).unwrap(),
            "대한민국의 수도는 서울입니다."
        );
    }
}
