use super::super::*;
use crate::surfaces::tui::runtime_bridge::TuiVisionStatus;

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
fn model_identity_detection_uses_query_features_instead_of_known_product_names() {
    for request in [
        "너 지금 orion-7b-instruct 잖아",
        "현재 구동 중인 기반 모델 명칭이 뭐야?",
        "지금 어떤 추론 모델을 사용 중이야?",
        "what model is running right now?",
        "which model powers this session?",
    ] {
        assert_eq!(
            local_reply(
                request,
                Some("unknown-family-test"),
                TuiVisionStatus::OnDemand
            ),
            Some("현재 사용 중인 모델은 unknown-family-test입니다.".to_string()),
            "{request}"
        );
    }
}

#[test]
fn model_identity_detection_preserves_non_identity_intents() {
    for request in [
        "현재 모델 대신 뭘 추천해?",
        "현재 모델과 다른 모델 성능 비교해줘",
        "내가 전에 어떤 모델을 선호한다고 했지?",
        "이 모델로 다음 작업 계속해",
        "현재 모델 설정 코드를 수정해줘",
        "Please answer in English: what model is running right now?",
    ] {
        assert_eq!(
            local_reply(
                request,
                Some("unknown-family-test"),
                TuiVisionStatus::OnDemand
            ),
            None,
            "{request}는 현재 모델 사실 응답으로 가로채면 안 됩니다."
        );
    }
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
fn local_access_status_uses_verified_runtime_capability_without_stealing_tasks() {
    for request in [
        "너 로컬 파일시스템 접근 가능해?",
        "현재 프로젝트 파일을 직접 읽을 수 있어?",
    ] {
        let reply = local_reply(request, Some("gemma-test"), TuiVisionStatus::OnDemand)
            .expect("local access capability is a runtime fact");
        assert!(reply.starts_with("가능합니다."), "{request}");
        assert!(reply.contains("현재 프로젝트 범위"), "{request}");
        assert!(reply.contains("읽기 전용"), "{request}");
    }

    assert_eq!(
        local_reply(
            "src/main.rs 파일을 읽어서 구조를 분석해줘",
            Some("gemma-test"),
            TuiVisionStatus::OnDemand,
        ),
        None,
    );
}
