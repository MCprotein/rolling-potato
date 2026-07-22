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

    has_korean_signal
        || has_english_word
        || lower.contains("thank you")
        || lower.contains("what can you do")
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

    report.trim().to_string()
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
        for request in ["안녕", "안녕하세요!", "고마워", "뭐 할 수 있어?", "hello"] {
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
    }
}
