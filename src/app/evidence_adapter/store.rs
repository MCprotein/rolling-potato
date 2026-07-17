use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use crate::adapters::filesystem::layout as paths;
use crate::foundation::error::AppError;
use crate::foundation::serialization as strict_json;
use crate::runtime_core::knowledge::evidence::{stale_policy_summary, EvidenceStoreStatus};

pub fn store_status() -> Result<EvidenceStoreStatus, AppError> {
    let runtime_evidence_file = paths::runtime_evidence_file();
    let project_evidence_dir = paths::project_evidence_dir();

    Ok(EvidenceStoreStatus {
        runtime_evidence_records: count_jsonl_records(&runtime_evidence_file)?,
        project_artifacts: count_files(&project_evidence_dir)?,
        runtime_evidence_file,
        project_evidence_dir,
        stale_policy: stale_policy_summary(),
        truncated: false,
    })
}

pub(crate) fn store_status_bounded(
    scan_limit: usize,
    max_bytes: u64,
) -> Result<EvidenceStoreStatus, AppError> {
    if scan_limit == 0 || max_bytes == 0 {
        return Err(AppError::blocked(
            "evidence read-only budget은 0보다 커야 합니다.",
        ));
    }
    let runtime_evidence_file = paths::runtime_evidence_file();
    let project_evidence_dir = paths::project_evidence_dir();
    let (runtime_evidence_records, runtime_truncated) =
        count_jsonl_records_bounded(&runtime_evidence_file, scan_limit, max_bytes)?;
    let (project_artifacts, project_truncated) =
        count_top_level_files_bounded(&project_evidence_dir, scan_limit)?;
    Ok(EvidenceStoreStatus {
        runtime_evidence_file,
        runtime_evidence_records,
        project_evidence_dir,
        project_artifacts,
        stale_policy: stale_policy_summary(),
        truncated: runtime_truncated || project_truncated,
    })
}

fn count_jsonl_records(path: &Path) -> Result<usize, AppError> {
    if !path.exists() {
        return Ok(0);
    }

    let body = fs::read_to_string(path).map_err(|err| {
        AppError::runtime(format!(
            "runtime evidence store를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    Ok(body.lines().filter(|line| !line.trim().is_empty()).count())
}

fn count_jsonl_records_bounded(
    path: &Path,
    scan_limit: usize,
    max_bytes: u64,
) -> Result<(usize, bool), AppError> {
    if !path.exists() {
        return Ok((0, false));
    }
    let metadata = fs::symlink_metadata(path).map_err(|err| {
        AppError::blocked(format!(
            "runtime evidence metadata를 읽지 못했습니다: {err}"
        ))
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AppError::blocked(
            "runtime evidence regular-file boundary 불일치",
        ));
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|err| AppError::blocked(format!("runtime evidence open 실패: {err}")))?
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|err| AppError::blocked(format!("runtime evidence bounded read 실패: {err}")))?;
    let byte_truncated = bytes.len() as u64 > max_bytes;
    if byte_truncated {
        bytes.truncate(max_bytes as usize);
    }
    let body = std::str::from_utf8(&bytes)
        .map_err(|_| AppError::blocked("runtime evidence UTF-8 불일치"))?;
    let mut count = 0_usize;
    let mut record_truncated = false;
    for line in body.lines().filter(|line| !line.trim().is_empty()) {
        if count == scan_limit {
            record_truncated = true;
            break;
        }
        count = count.saturating_add(1);
        strict_json::parse_value(line, "runtime evidence bounded record")?;
    }
    Ok((count, byte_truncated || record_truncated))
}

fn count_top_level_files_bounded(
    path: &Path,
    scan_limit: usize,
) -> Result<(usize, bool), AppError> {
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok((0, false)),
        Err(err) => {
            return Err(AppError::blocked(format!(
                "project evidence directory 읽기 실패: {err}"
            )))
        }
    };
    let mut files = 0_usize;
    let mut scanned = 0_usize;
    for entry in entries {
        if scanned == scan_limit {
            return Ok((files, true));
        }
        scanned = scanned.saturating_add(1);
        let entry = entry
            .map_err(|err| AppError::blocked(format!("project evidence entry 실패: {err}")))?;
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|err| AppError::blocked(format!("project evidence metadata 실패: {err}")))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(AppError::blocked(
                "project evidence view는 top-level regular file만 허용합니다.",
            ));
        }
        files = files.saturating_add(1);
    }
    Ok((files, false))
}

fn count_files(path: &Path) -> Result<usize, AppError> {
    if !path.exists() {
        return Ok(0);
    }

    let mut count = 0;
    for entry in fs::read_dir(path).map_err(|err| {
        AppError::runtime(format!(
            "project evidence 디렉터리를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })? {
        let entry = entry.map_err(|err| {
            AppError::runtime(format!(
                "project evidence 항목을 읽지 못했습니다: {} ({err})",
                path.display()
            ))
        })?;
        let file_type = entry.file_type().map_err(|err| {
            AppError::runtime(format!(
                "project evidence 항목 타입을 읽지 못했습니다: {} ({err})",
                entry.path().display()
            ))
        })?;
        if file_type.is_file() {
            count += 1;
        } else if file_type.is_dir() {
            count += count_files(&entry.path())?;
        }
    }
    Ok(count)
}
