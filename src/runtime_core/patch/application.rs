//! Guarded patch apply and rollback decisions.

use std::path::PathBuf;

use crate::foundation::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ApplyResult {
    pub relative_path: String,
    pub original_sha256: String,
    pub applied_sha256: String,
    pub rollback_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RollbackResult {
    pub restored: bool,
    pub status: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ApplyAdmission {
    AlreadyApplied,
    Ready,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RollbackAdmission {
    AlreadyRestored(RollbackResult),
    Ready,
    Conflict(RollbackResult),
}

pub(crate) fn admit_apply(
    relative_path: &str,
    current_sha256: &str,
    original_sha256: &str,
    proposed_sha256: &str,
    rollback_sha256: Option<&str>,
) -> Result<ApplyAdmission, AppError> {
    if current_sha256 == proposed_sha256 {
        if let Some(rollback_sha256) = rollback_sha256 {
            if rollback_sha256 != original_sha256 {
                return Err(AppError::blocked(
                    "patch approve 차단\n- 이유: rollback record hash가 original hash와 일치하지 않습니다.",
                ));
            }
            return Ok(ApplyAdmission::AlreadyApplied);
        }
    }
    if current_sha256 != original_sha256 {
        return Err(AppError::blocked(format!(
            "patch approve 차단\n- 이유: 대상 파일이 preview 이후 변경되었습니다.\n- path: {}\n- expected original sha256: {}\n- current sha256: {}\n- 동작: patch preview를 다시 생성하세요.",
            relative_path, original_sha256, current_sha256
        )));
    }
    Ok(ApplyAdmission::Ready)
}

pub(crate) fn admit_rollback(
    current_sha256: &str,
    original_sha256: &str,
    proposed_sha256: &str,
) -> RollbackAdmission {
    if current_sha256 == original_sha256 {
        return RollbackAdmission::AlreadyRestored(RollbackResult {
            restored: true,
            status: format!("already-restored-and-verified sha256={original_sha256}"),
        });
    }
    if current_sha256 == proposed_sha256 {
        return RollbackAdmission::Ready;
    }
    RollbackAdmission::Conflict(RollbackResult {
        restored: false,
        status: format!("restore-conflict: target changed concurrently current={current_sha256}"),
    })
}

pub(crate) fn validate_rollback_record(
    actual_sha256: &str,
    expected_sha256: &str,
) -> Result<(), RollbackResult> {
    if actual_sha256 == expected_sha256 {
        Ok(())
    } else {
        Err(RollbackResult {
            restored: false,
            status: "restore-failed: rollback record hash mismatch".to_string(),
        })
    }
}

pub(crate) fn validate_applied_source(
    relative_path: &str,
    actual_sha256: &str,
    expected_sha256: &str,
) -> Result<(), AppError> {
    if actual_sha256 == expected_sha256 {
        Ok(())
    } else {
        Err(AppError::blocked(format!(
            "patch verification 차단\n- 이유: 적용된 source hash가 proposal과 일치하지 않습니다.\n- path: {}\n- expected proposed sha256: {}\n- current sha256: {}",
            relative_path, expected_sha256, actual_sha256
        )))
    }
}

pub(crate) fn validate_applied_rollback(
    actual_sha256: &str,
    expected_sha256: &str,
) -> Result<(), AppError> {
    if actual_sha256 == expected_sha256 {
        Ok(())
    } else {
        Err(AppError::blocked(
            "patch verification 차단\n- 이유: rollback record hash가 original hash와 일치하지 않습니다.",
        ))
    }
}

pub(crate) fn restored_result(actual_sha256: &str, expected_sha256: &str) -> RollbackResult {
    if actual_sha256 == expected_sha256 {
        RollbackResult {
            restored: true,
            status: format!("restored-and-verified sha256={expected_sha256}"),
        }
    } else {
        RollbackResult {
            restored: false,
            status: format!("restore-failed: restored hash mismatch actual={actual_sha256}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_admission_rejects_stale_source_and_bad_rollback() {
        let stale = admit_apply("src/lib.rs", "changed", "before", "after", None).unwrap_err();
        assert_eq!(stale.code, 3);
        assert!(stale.message.contains("preview 이후 변경"));

        let bad_rollback =
            admit_apply("src/lib.rs", "after", "before", "after", Some("tampered")).unwrap_err();
        assert_eq!(bad_rollback.code, 3);
        assert!(bad_rollback.message.contains("rollback record hash"));

        assert_eq!(
            admit_apply("src/lib.rs", "before", "before", "after", None).unwrap(),
            ApplyAdmission::Ready
        );
        assert_eq!(
            admit_apply("src/lib.rs", "after", "before", "after", Some("before")).unwrap(),
            ApplyAdmission::AlreadyApplied
        );
    }

    #[test]
    fn rollback_admission_preserves_concurrent_edits() {
        assert!(matches!(
            admit_rollback("before", "before", "after"),
            RollbackAdmission::AlreadyRestored(_)
        ));
        assert_eq!(
            admit_rollback("after", "before", "after"),
            RollbackAdmission::Ready
        );
        let RollbackAdmission::Conflict(conflict) = admit_rollback("user-edit", "before", "after")
        else {
            panic!("concurrent edit must conflict");
        };
        assert!(!conflict.restored);
        assert!(conflict.status.contains("user-edit"));
    }
}
