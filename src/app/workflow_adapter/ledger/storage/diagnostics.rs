use std::path::Path;

use crate::foundation::error::AppError;

pub(super) fn ledger_corrupt(path: &Path, line: usize, reason: &str) -> AppError {
    let gap = crate::app::workflow_adapter::state::record_validation_gap(
        "corrupt-ledger",
        &format!("{}:{line}:{reason}", path.display()),
    );
    let suffix = gap
        .err()
        .map(|err| format!("\n- validation-gap 저장 실패: {}", err.message))
        .unwrap_or_default();
    AppError::blocked(format!(
        "runtime ledger 검증 차단\n- 이유: {reason}\n- path: {}\n- line: {line}{suffix}",
        path.display()
    ))
}
