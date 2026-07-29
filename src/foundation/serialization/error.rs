use crate::foundation::error::AppError;

pub(super) fn blocked(context: &str, reason: &str) -> AppError {
    AppError::blocked(format!(
        "strict JSON 검증 차단\n- artifact: {context}\n- 이유: {reason}"
    ))
}
