//! Surface-neutral approval request records.

use crate::foundation::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRequest {
    pub request_id: String,
    pub source: String,
    pub status: String,
    pub reason: String,
    pub event_id: String,
    pub session_id: String,
    pub summary: String,
    pub items: Vec<String>,
}

pub(crate) fn render_request_record(
    request: &ApprovalRequest,
    redact: impl Fn(&str) -> String,
) -> String {
    let mut lines = vec![
        "record_version=1".to_string(),
        format!("request_id={}", record_value(&request.request_id, &redact)),
        format!("source={}", record_value(&request.source, &redact)),
        format!("status={}", record_value(&request.status, &redact)),
        format!("reason={}", record_value(&request.reason, &redact)),
        format!("event_id={}", record_value(&request.event_id, &redact)),
        format!("session_id={}", record_value(&request.session_id, &redact)),
        format!("summary={}", record_value(&request.summary, &redact)),
        format!("item_count={}", request.items.len()),
    ];
    for (index, item) in request.items.iter().enumerate() {
        lines.push(format!(
            "item_{}={}",
            index + 1,
            record_value(item, &redact)
        ));
    }
    lines.push(String::new());
    lines.join("\n")
}

pub(crate) fn validate_request_id(request_id: &str) -> Result<(), AppError> {
    if request_id.is_empty()
        || !request_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(AppError::runtime(format!(
            "approval request id가 안전하지 않습니다: {request_id}"
        )));
    }
    Ok(())
}

fn record_value(value: &str, redact: &impl Fn(&str) -> String) -> String {
    redact(value).replace(['\n', '\r'], " ")
}
