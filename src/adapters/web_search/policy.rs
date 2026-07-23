use crate::foundation::error::AppError;

pub(super) const MAX_QUERY_CHARS: usize = 400;
pub(super) const MAX_QUERY_WORDS: usize = 50;
const MAX_SOURCE_URL_BYTES: usize = 2_048;

pub(super) fn validate_query(query: &str) -> Result<&str, AppError> {
    let query = query.trim();
    if query.is_empty() {
        return Err(AppError::usage("웹 검색어가 필요합니다."));
    }
    if query.chars().count() > MAX_QUERY_CHARS || query.split_whitespace().count() > MAX_QUERY_WORDS
    {
        return Err(AppError::usage(format!(
            "웹 검색어는 최대 {MAX_QUERY_CHARS}자, {MAX_QUERY_WORDS}단어까지 허용합니다."
        )));
    }
    if query
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\t' | '\n'))
    {
        return Err(AppError::usage(
            "웹 검색어에는 제어 문자를 사용할 수 없습니다.",
        ));
    }
    Ok(query)
}

pub(super) fn is_valid_https_source_url(url: &str) -> bool {
    if url.len() > MAX_SOURCE_URL_BYTES
        || url
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return false;
    }
    let Ok(uri) = url.parse::<ureq::http::Uri>() else {
        return false;
    };
    uri.scheme_str() == Some("https")
        && uri.authority().is_some_and(|authority| {
            !authority.host().is_empty() && !authority.as_str().contains('@')
        })
}
