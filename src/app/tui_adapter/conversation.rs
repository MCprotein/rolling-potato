//! Non-mutating conversation path for general questions that do not need agent tools.

use crate::foundation::error::AppError;
use crate::runtime_core::inference::backend::{BackendChatInput, ResponseLanguage};

const CONVERSATION_MAX_TOKENS: u32 = 384;

pub(super) enum RequestDecision {
    Answer(String),
    ContinueLocal,
    WebTool(crate::app::web_search_adapter::WebToolRoute),
}

pub(super) fn is_conversational_request(request: &str) -> bool {
    let trimmed = request.trim();
    !trimmed.is_empty() && trimmed.chars().count() <= 2_000 && !has_agent_task_signal(trimmed)
}

pub(super) fn local_reply(request: &str, model: Option<&str>) -> Option<String> {
    if ResponseLanguage::from_user_request(request).allows_non_korean() {
        return None;
    }
    if is_model_identity_request(request) {
        return Some(
            match model.map(str::trim).filter(|value| !value.is_empty()) {
                Some(model) => format!("현재 사용 중인 모델은 {model}입니다."),
                None => {
                    "현재 선택된 모델이 없습니다. /model로 모델을 선택할 수 있습니다.".to_string()
                }
            },
        );
    }
    is_agent_identity_request(request)
        .then(|| "저는 로컬에서 실행되는 범용 AI·코딩 에이전트 rpotato입니다.".to_string())
}

pub(super) fn decide_request(
    user_request: &str,
    local_context: &str,
    allow_direct_answer: bool,
) -> Result<RequestDecision, AppError> {
    let response_language = ResponseLanguage::from_user_request(user_request);
    let language_instruction = language_instruction(response_language);
    let web_enabled = !crate::app::web_search_adapter::web_disabled(user_request);
    let web_instruction = if web_enabled {
        "답변에 현재 웹 정보나 외부 공개 근거가 실제로 필요하면 추측하거나 검색이 필요하다고 말하지 말고, 답변 대신 아래 두 줄만 출력해 WebSearch·WebOpen·WebFind 중 하나를 요청하라. WEB INPUT에는 첨부 파일 내용, 인증정보, 개인정보를 복사하지 말고 사용자 질문에서 필요한 최소 공개 검색어 또는 URL만 넣어라.\nWEB TOOL: search|open|find\nWEB INPUT: 최소 검색어 또는 HTTPS URL"
    } else {
        "사용자가 이 요청에서 인터넷 사용을 금지했다. 웹 도구를 요청하지 말고 현재 로컬 지식과 문맥만 사용하며 최신성이 불확실하면 그 한계를 밝혀라."
    };
    let completion_instruction = if allow_direct_answer {
        "웹 도구가 필요하지 않으면 사용자 질문에 바로 답하라."
    } else {
        "웹 도구가 필요하지 않으면 다른 설명 없이 `LOCAL TASK`만 출력하라."
    };
    let prompt = format!(
        "너는 rpotato라는 이름의 로컬 AI·코딩 에이전트다. 기반 모델의 개발사나 학습 출처를 자신의 정체성으로 소개하지 마라. {language_instruction} 기술 용어와 고유명사는 필요한 원문 표기를 유지한다. {web_instruction} {completion_instruction} 내부 추론, MODEL ACTION, 도구 설명, 메타데이터는 출력하지 마라.\n\n<USER_REQUEST>\n{user_request}\n</USER_REQUEST>\n\n<LOCAL_CONTEXT>\n{local_context}\n</LOCAL_CONTEXT>\n\n응답:"
    );
    let candidate = crate::app::inference_adapter::answer::generate_candidate_for_user(
        &prompt,
        user_request,
        CONVERSATION_MAX_TOKENS,
    )?;
    if web_enabled {
        if let Some(tool) = crate::app::web_search_adapter::parse_agent_web_tool(&candidate.visible)
        {
            return Ok(RequestDecision::WebTool(tool));
        }
    }
    if !allow_direct_answer {
        return Ok(RequestDecision::ContinueLocal);
    }
    crate::app::inference_adapter::answer::finish_candidate(candidate).map(RequestDecision::Answer)
}

pub(super) fn reply_with_images(input: &BackendChatInput) -> Result<String, AppError> {
    let mut input = input.clone();
    let language_instruction = language_instruction(input.response_language);
    input.text = format!(
        "너는 rpotato라는 이름의 로컬 범용 AI·코딩 에이전트다. 첨부 이미지를 직접 살펴본다. {language_instruction} 이미지에서 확인할 수 없는 내용은 추측하지 마라. 내부 추론, MODEL ACTION, 메타데이터는 출력하지 마라.\n\n사용자: {}\n답변:",
        input.text
    );
    crate::app::inference_adapter::answer::generate_input(&input, CONVERSATION_MAX_TOKENS)
}

fn language_instruction(language: ResponseLanguage) -> &'static str {
    if language.allows_non_korean() {
        "사용자가 명시한 출력 언어로 정확하게 답하라."
    } else {
        "사용자가 요청한 내용에만 정확하고 자연스러운 한국어로 답하라."
    }
}

pub(super) fn present_agent_report(report: &str) -> String {
    if let Some((_, answer)) = report.split_once("- 답변:\n") {
        let answer = answer.trim();
        if !answer.is_empty() {
            return answer.to_string();
        }
    }

    if report.contains("- status: pending-approval") {
        let workflow = report_field(report, "workflow id").unwrap_or("unknown");
        let proposal = report_field(report, "proposal id").unwrap_or("unknown");
        let approval = report_field(report, "approval command");
        let diff = report
            .split_once("- diff:\n")
            .map(|(_, value)| value.trim())
            .filter(|value| !value.is_empty());
        let mut visible = vec![
            "변경 제안을 준비했습니다.".to_string(),
            format!("workflow: {workflow}"),
            format!("proposal: {proposal}"),
        ];
        if let Some(diff) = diff {
            visible.push(String::new());
            visible.push(diff.to_string());
        }
        visible.push(String::new());
        visible.push(format!(
            "검토 후 적용: select {workflow} → approve {proposal}"
        ));
        if let Some(approval) = approval {
            visible.push(format!("one-time 승인 정보: {approval}"));
        }
        return visible.join("\n");
    }

    if report.contains("backend-call-failed") {
        return "모델 응답을 받지 못했습니다. 잠시 후 다시 시도하거나 /doctor로 backend 상태를 확인하세요."
            .to_string();
    }

    report.trim().to_string()
}

fn is_model_identity_request(request: &str) -> bool {
    let lower = request.trim().to_ascii_lowercase();
    if !lower.contains("모델") && !lower.contains("model") {
        return false;
    }
    [
        "무슨",
        "어떤",
        "뭐",
        "이름",
        "현재",
        "사용 중",
        "사용중",
        "쓰고",
    ]
    .iter()
    .any(|signal| lower.contains(signal))
        || [
            "what model",
            "which model",
            "model are you",
            "current model",
        ]
        .iter()
        .any(|signal| lower.contains(signal))
}

fn is_agent_identity_request(request: &str) -> bool {
    let lower = request.trim().to_ascii_lowercase();
    let compact = lower
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    [
        "넌누구",
        "너는누구",
        "누구야",
        "정체가뭐",
        "이름이뭐",
        "이름이뭔",
        "네이름",
        "너이름",
    ]
    .iter()
    .any(|signal| compact.contains(signal))
        || lower.contains("who are you")
        || lower.contains("what is your name")
}

fn report_field<'a>(report: &'a str, field: &str) -> Option<&'a str> {
    let prefix = format!("- {field}: ");
    report
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn has_agent_task_signal(request: &str) -> bool {
    let lower = request.to_ascii_lowercase();
    let words = ascii_words(&lower);
    let english_mutation = ["fix", "change", "edit", "implement", "refactor"]
        .iter()
        .any(|signal| words.contains(signal));
    let english_failure = ["error", "crash", "crashes", "startup"]
        .iter()
        .any(|signal| words.contains(signal));
    let english_local_scope = ["file", "code", "repo", "repository", "codebase", "project"]
        .iter()
        .any(|signal| words.contains(signal));
    let english_action = is_english_action_request(&words);
    let korean_action = [
        "고쳐",
        "수정",
        "변경",
        "구현",
        "리팩터",
        "테스트",
        "리뷰",
        "분석",
        "찾아",
    ]
    .iter()
    .any(|signal| request.contains(signal));
    let korean_local_scope = [
        "파일",
        "코드",
        "저장소",
        "프로젝트",
        "디렉터리",
        "경로",
        "소스",
    ]
    .iter()
    .any(|signal| request.contains(signal));
    let korean_local_action = ["알려", "보여", "열어", "확인", "구조", "내용", "어디"]
        .iter()
        .any(|signal| request.contains(signal));

    english_mutation
        || english_failure
        || (english_local_scope && english_action)
        || korean_action
        || (korean_local_scope && korean_local_action)
}

fn ascii_words(text: &str) -> Vec<&str> {
    text.split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect()
}

fn is_english_action_request(words: &[&str]) -> bool {
    const ACTIONS: &[&str] = &[
        "test", "review", "analyze", "search", "show", "open", "read", "find", "explain",
    ];
    words.first().is_some_and(|word| ACTIONS.contains(word))
        || words
            .windows(2)
            .any(|window| window[0] == "please" && ACTIONS.contains(&window[1]))
        || words.windows(3).any(|window| {
            matches!(window[0], "can" | "could" | "would")
                && window[1] == "you"
                && ACTIONS.contains(&window[2])
        })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        ] {
            assert!(is_conversational_request(request), "{request}");
        }
        for request in [
            "안녕, 이 코드 고쳐줘",
            "src/main.rs 수정해줘",
            "이 오류를 분석해줘",
            "테스트를 실행해줘",
            "이 저장소 구조를 알려줘",
            "this crashes on startup",
            "they need help with startup",
        ] {
            assert!(!is_conversational_request(request), "{request}");
        }
    }

    #[test]
    fn model_and_agent_identity_questions_return_local_facts_without_a_workflow() {
        assert_eq!(
            local_reply("넌 무슨모델이니", Some("gemma-test")),
            Some("현재 사용 중인 모델은 gemma-test입니다.".to_string())
        );
        assert_eq!(
            local_reply("넌누구니?", Some("ignored")),
            Some("저는 로컬에서 실행되는 범용 AI·코딩 에이전트 rpotato입니다.".to_string())
        );
        assert_eq!(
            local_reply("이름이뭔데", Some("ignored")),
            Some("저는 로컬에서 실행되는 범용 AI·코딩 에이전트 rpotato입니다.".to_string())
        );
        assert_eq!(
            local_reply("이 모델 코드를 수정해줘", Some("gemma-test")),
            None
        );
        assert_eq!(
            local_reply(
                "Please answer in English: which model are you using?",
                Some("gemma-test")
            ),
            None
        );
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
}
