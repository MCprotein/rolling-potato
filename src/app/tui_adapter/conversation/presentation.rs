use crate::foundation::error::AppError;

pub(in crate::app::tui_adapter) fn ensure_public_answer(
    answer: String,
) -> Result<String, AppError> {
    if contains_private_tool_protocol(&answer) {
        return Err(AppError::blocked(
            "모델이 내부 도구 요청을 반복해 안전한 최종 답변을 만들지 못했습니다. 요청을 다시 표현하거나 /doctor로 모델 상태를 확인하세요.",
        ));
    }
    Ok(answer)
}

pub(in crate::app::tui_adapter) fn present_agent_report(report: &str) -> String {
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

pub(super) fn contains_private_tool_protocol(candidate: &str) -> bool {
    candidate.lines().any(|line| {
        let Some((label, _)) = line.trim().split_once(':') else {
            return false;
        };
        matches!(
            label
                .chars()
                .filter(|character| character.is_ascii_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect::<String>()
                .as_str(),
            "webtool" | "webinput" | "browsertool" | "browserurl" | "browserinput"
        )
    })
}

fn report_field<'a>(report: &'a str, field: &str) -> Option<&'a str> {
    let prefix = format!("- {field}: ");
    report
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}
