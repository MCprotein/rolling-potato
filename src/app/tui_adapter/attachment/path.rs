use super::*;

pub(super) fn normalized_source_path(value: &str) -> Result<PathBuf, AppError> {
    let value = value.trim();
    let value = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(value)
        .replace("\\ ", " ");
    if value.trim().is_empty() {
        return Err(AppError::usage("첨부할 파일 경로가 필요합니다."));
    }
    if let Some(suffix) = value.strip_prefix("~/") {
        let home = std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| {
            AppError::usage("HOME을 확인할 수 없어 ~/ 경로를 해석하지 못했습니다.")
        })?;
        return Ok(home.join(suffix));
    }
    Ok(PathBuf::from(
        value.strip_prefix("file://").unwrap_or(&value),
    ))
}

pub(super) fn safe_leaf(value: &str) -> String {
    let value = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                character
            } else {
                '_'
            }
        })
        .take(120)
        .collect::<String>();
    if value.is_empty() {
        "attachment".to_string()
    } else {
        value
    }
}
