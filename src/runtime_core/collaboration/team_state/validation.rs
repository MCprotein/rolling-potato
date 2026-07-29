use crate::foundation::error::AppError;

pub(crate) fn validate_id(value: &str, label: &str, max_bytes: usize) -> Result<(), AppError> {
    if value.is_empty()
        || value.len() > max_bytes
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
    {
        return Err(AppError::blocked(format!("{label} 형식 오류: {value}")));
    }
    Ok(())
}

pub(crate) fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
