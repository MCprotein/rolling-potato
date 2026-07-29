use super::super::*;

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
