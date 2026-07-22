//! Narrow non-mutating conversation path for greetings and basic TUI questions.

use crate::app::inference_adapter::backend;
use crate::foundation::error::AppError;

const CONVERSATION_MAX_TOKENS: u32 = 384;

pub(super) fn is_conversational_request(request: &str) -> bool {
    let trimmed = request.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 80 || has_agent_task_signal(trimmed) {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    let has_korean_signal = [
        "안녕",
        "반가워",
        "반갑습니다",
        "고마워",
        "감사합니다",
        "뭐 할 수 있어",
        "무엇을 할 수 있어",
        "사용법",
        "도움말",
    ]
    .iter()
    .any(|signal| lower.contains(signal));
    let has_english_word = lower
        .split(|ch: char| !ch.is_ascii_alphabetic())
        .any(|word| matches!(word, "hello" | "hi" | "hey" | "thanks"));

    is_model_identity_request(trimmed)
        || is_agent_identity_request(trimmed)
        || has_korean_signal
        || has_english_word
        || lower.contains("thank you")
        || lower.contains("what can you do")
}

pub(super) fn local_reply(request: &str, model: Option<&str>) -> Option<String> {
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
        .then(|| "저는 로컬에서 실행되는 코딩 에이전트 rpotato입니다.".to_string())
}

pub(super) fn reply(request: &str) -> Result<String, AppError> {
    let prompt = format!(
        "반드시 내부 추론 없이 첫 문장부터 짧고 자연스러운 한국어 최종 답변만 출력하세요. rpotato 로컬 코딩 에이전트로서 인사나 기본 사용 질문에만 답하세요. 사용자 입력: {request}"
    );
    backend::chat_once(&prompt, Some(CONVERSATION_MAX_TOKENS)).map(|run| run.response)
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
    ["넌 누구", "너는 누구", "누구야", "정체가 뭐", "who are you"]
        .iter()
        .any(|signal| lower.contains(signal))
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
    [
        "fix",
        "change",
        "edit",
        "implement",
        "refactor",
        "test",
        "review",
        "analyze",
        "search",
        "file",
        "code",
    ]
    .iter()
    .any(|signal| lower.contains(signal))
        || [
            "고쳐",
            "수정",
            "변경",
            "구현",
            "리팩터",
            "테스트",
            "리뷰",
            "분석",
            "찾아",
            "파일",
            "코드",
        ]
        .iter()
        .any(|signal| request.contains(signal))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greetings_and_basic_help_use_conversation_without_stealing_coding_tasks() {
        for request in [
            "안녕",
            "안녕하세요!",
            "고마워",
            "뭐 할 수 있어?",
            "hello",
            "넌 무슨모델이니",
            "너는 누구야?",
        ] {
            assert!(is_conversational_request(request), "{request}");
        }
        for request in [
            "안녕, 이 코드 고쳐줘",
            "src/main.rs 수정해줘",
            "이 오류를 분석해줘",
            "테스트를 실행해줘",
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
            local_reply("너는 누구야?", Some("ignored")),
            Some("저는 로컬에서 실행되는 코딩 에이전트 rpotato입니다.".to_string())
        );
        assert_eq!(
            local_reply("이 모델 코드를 수정해줘", Some("gemma-test")),
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
