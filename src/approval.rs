use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::app::AppError;
use crate::{ledger, paths};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRequestSummary {
    pub request_id: String,
    pub source: String,
    pub status: String,
    pub reason: String,
    pub item_count: String,
    pub request_path: PathBuf,
}

pub fn write_request(request: &ApprovalRequest) -> Result<PathBuf, AppError> {
    validate_request_id(&request.request_id)?;
    let dir = paths::project_approval_requests_dir();
    fs::create_dir_all(&dir).map_err(|err| {
        AppError::runtime(format!(
            "approval request directory를 만들지 못했습니다: {} ({err})",
            dir.display()
        ))
    })?;

    let path = dir.join(format!("{}.txt", request.request_id));
    fs::write(&path, render_request_record(request)).map_err(|err| {
        AppError::runtime(format!(
            "approval request record를 쓰지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    Ok(path)
}

pub fn request_summaries(limit: usize) -> Result<Vec<ApprovalRequestSummary>, AppError> {
    request_summaries_bounded(limit, usize::MAX)
}

pub fn request_summaries_bounded(
    limit: usize,
    scan_limit: usize,
) -> Result<Vec<ApprovalRequestSummary>, AppError> {
    let dir = paths::project_approval_requests_dir();
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => {
            return Err(AppError::runtime(format!(
                "approval request directory를 읽지 못했습니다: {} ({err})",
                dir.display()
            )));
        }
    };

    let mut rows = Vec::new();
    for (index, entry) in entries.enumerate() {
        if index >= scan_limit {
            return Err(AppError::blocked(
                "approval request view directory scan budget 초과",
            ));
        }
        let entry = entry.map_err(|err| {
            AppError::runtime(format!("approval request entry를 읽지 못했습니다: {err}"))
        })?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("txt") {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        rows.push((modified, summary_from_path(&path)?));
    }

    rows.sort_by_key(|row| std::cmp::Reverse(row.0));
    Ok(rows
        .into_iter()
        .take(limit)
        .map(|(_, summary)| summary)
        .collect())
}

fn render_request_record(request: &ApprovalRequest) -> String {
    let mut lines = vec![
        "record_version=1".to_string(),
        format!("request_id={}", record_value(&request.request_id)),
        format!("source={}", record_value(&request.source)),
        format!("status={}", record_value(&request.status)),
        format!("reason={}", record_value(&request.reason)),
        format!("event_id={}", record_value(&request.event_id)),
        format!("session_id={}", record_value(&request.session_id)),
        format!("summary={}", record_value(&request.summary)),
        format!("item_count={}", request.items.len()),
    ];
    for (index, item) in request.items.iter().enumerate() {
        lines.push(format!("item_{}={}", index + 1, record_value(item)));
    }
    lines.push(String::new());
    lines.join("\n")
}

fn summary_from_path(path: &Path) -> Result<ApprovalRequestSummary, AppError> {
    let metadata = fs::symlink_metadata(path).map_err(|err| {
        AppError::blocked(format!(
            "approval request record metadata를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > 64 * 1024 {
        return Err(AppError::blocked(
            "approval request summary regular-file/byte budget 불일치",
        ));
    }
    let contents = fs::read_to_string(path).map_err(|err| {
        AppError::runtime(format!(
            "approval request record를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    Ok(ApprovalRequestSummary {
        request_id: required_record_value(&contents, "request_id", path)?,
        source: required_record_value(&contents, "source", path)?,
        status: required_record_value(&contents, "status", path)?,
        reason: required_record_value(&contents, "reason", path)?,
        item_count: record_value_for(&contents, "item_count").unwrap_or_else(|| "0".to_string()),
        request_path: path.to_path_buf(),
    })
}

fn required_record_value(record: &str, key: &str, path: &Path) -> Result<String, AppError> {
    record_value_for(record, key).ok_or_else(|| {
        AppError::runtime(format!(
            "approval request record에 {} 값이 없습니다: {}",
            key,
            path.display()
        ))
    })
}

fn record_value_for(record: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    record
        .lines()
        .find_map(|line| line.strip_prefix(&prefix).map(|value| value.to_string()))
}

fn record_value(value: &str) -> String {
    ledger::redact_text(value).replace(['\n', '\r'], " ")
}

fn validate_request_id(request_id: &str) -> Result<(), AppError> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_summarizes_approval_request() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-approval-request-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let path = write_request(&ApprovalRequest {
            request_id: "approval-test".to_string(),
            source: "team-admission".to_string(),
            status: "pending-approval".to_string(),
            reason: "policy-blocked".to_string(),
            event_id: "event-test".to_string(),
            session_id: "session-test".to_string(),
            summary: "policy approval required".to_string(),
            items: vec!["write: README.md -> ask".to_string()],
        })
        .unwrap();
        let summaries = request_summaries(5).unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);

        assert!(path.ends_with("approval-test.txt"));
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].source, "team-admission");
        assert_eq!(summaries[0].status, "pending-approval");
        assert_eq!(summaries[0].reason, "policy-blocked");
        assert_eq!(summaries[0].item_count, "1");
    }
}
