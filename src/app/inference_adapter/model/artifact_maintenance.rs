use std::path::PathBuf;

use crate::adapters::filesystem::model_artifact;
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::foundation::integrity as checksum;
use crate::runtime_core::inference::model::manifest::find_candidate;

pub fn verify_file_report(path: &str, expected_sha256: &str) -> Result<String, AppError> {
    if !checksum::is_valid_sha256(expected_sha256) {
        return Err(AppError::usage(
            "expected SHA-256은 64자리 hex string이어야 합니다.",
        ));
    }

    let path = PathBuf::from(path);
    let actual_sha256 = model_artifact::sha256_for_file(&path)?;
    let matched = actual_sha256.eq_ignore_ascii_case(expected_sha256);
    let event_type = if matched {
        "model.sha256.verified"
    } else {
        "model.sha256.rejected"
    };
    let summary = if matched {
        "model artifact SHA-256 검증 성공"
    } else {
        "model artifact SHA-256 검증 실패"
    };
    let event_id = state::record_event(
        event_type,
        summary,
        &format!(
            "path={} expected_sha256={} actual_sha256={}",
            path.display(),
            expected_sha256,
            actual_sha256
        ),
    )?;

    if !matched {
        return Err(AppError::blocked(format!(
            "SHA-256 검증 실패\n- path: {}\n- expected: {}\n- actual: {}\n- ledger event: {}\n- 동작: registry 등록을 차단해야 하며, 실패 artifact 정리는 별도 cleanup phase에서 처리합니다.",
            path.display(),
            expected_sha256,
            actual_sha256,
            event_id
        )));
    }

    Ok(format!(
        "SHA-256 검증 성공\n- path: {}\n- expected: {}\n- actual: {}\n- ledger event: {}",
        path.display(),
        expected_sha256,
        actual_sha256,
        event_id
    ))
}

pub fn cleanup_failed_report(id: &str, dry_run: bool) -> Result<String, AppError> {
    let candidate = find_candidate(id)?;
    let cleanup = model_artifact::cleanup_failed_artifacts(candidate, dry_run)?;

    let event_id = state::record_event(
        if dry_run {
            "model.failed_artifact.cleanup.planned"
        } else {
            "model.failed_artifact.cleanup.completed"
        },
        if dry_run {
            "failed model artifact cleanup dry-run"
        } else {
            "failed model artifact cleanup 완료"
        },
        &format!(
            "model_id={} dry_run={} removed={} missing={}",
            candidate.id, dry_run, cleanup.removed, cleanup.missing
        ),
    )?;

    Ok(format!(
        "failed artifact cleanup {}\n- id: {}\n- removed: {}\n- missing: {}\n- ledger event: {}\n{}\n- boundary: app data downloads/models 아래의 failed/partial artifact만 대상으로 합니다.",
        if dry_run { "dry-run" } else { "결과" },
        candidate.id,
        cleanup.removed,
        cleanup.missing,
        event_id,
        cleanup.rows.join("\n")
    ))
}
