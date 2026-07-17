//! Filesystem adapter for approval request records.

use std::fs;
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;
#[cfg(test)]
use std::time::SystemTime;

use crate::foundation::error::AppError;
pub use crate::runtime_core::policy::approval::ApprovalRequest;
use crate::{adapters::filesystem::layout as paths, ledger};

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(test)]
pub struct ApprovalRequestSummary {
    pub request_id: String,
    pub source: String,
    pub status: String,
    pub reason: String,
    pub item_count: String,
    pub request_path: PathBuf,
}

pub fn write_request(request: &ApprovalRequest) -> Result<PathBuf, AppError> {
    crate::runtime_core::policy::approval::validate_request_id(&request.request_id)?;
    let dir = paths::project_approval_requests_dir();
    fs::create_dir_all(&dir).map_err(|err| {
        AppError::runtime(format!(
            "approval request directory를 만들지 못했습니다: {} ({err})",
            dir.display()
        ))
    })?;

    let path = dir.join(format!("{}.txt", request.request_id));
    fs::write(
        &path,
        crate::runtime_core::policy::approval::render_request_record(request, ledger::redact_text),
    )
    .map_err(|err| {
        AppError::runtime(format!(
            "approval request record를 쓰지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    Ok(path)
}

#[cfg(test)]
pub fn request_summaries(limit: usize) -> Result<Vec<ApprovalRequestSummary>, AppError> {
    request_summaries_bounded(limit, usize::MAX)
}

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
fn required_record_value(record: &str, key: &str, path: &Path) -> Result<String, AppError> {
    record_value_for(record, key).ok_or_else(|| {
        AppError::runtime(format!(
            "approval request record에 {} 값이 없습니다: {}",
            key,
            path.display()
        ))
    })
}

#[cfg(test)]
fn record_value_for(record: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    record
        .lines()
        .find_map(|line| line.strip_prefix(&prefix).map(|value| value.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_request_record_bytes_are_stable() {
        let rendered = crate::runtime_core::policy::approval::render_request_record(
            &ApprovalRequest {
                request_id: "approval-fixture".to_string(),
                source: "team-admission".to_string(),
                status: "pending-approval".to_string(),
                reason: "policy-blocked".to_string(),
                event_id: "event-fixture".to_string(),
                session_id: "session-fixture".to_string(),
                summary: "policy approval required".to_string(),
                items: vec!["write: README.md -> ask".to_string()],
            },
            ledger::redact_text,
        );

        assert_eq!(
            rendered,
            "record_version=1\nrequest_id=approval-fixture\nsource=team-admission\nstatus=pending-approval\nreason=policy-blocked\nevent_id=event-fixture\nsession_id=session-fixture\nsummary=policy approval required\nitem_count=1\nitem_1=write: README.md -> ask\n"
        );
    }

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
