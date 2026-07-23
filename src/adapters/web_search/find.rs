use crate::foundation::error::AppError;

use super::evidence::WebPageEvidence;

const MAX_FIND_QUERY_CHARS: usize = 160;
const MAX_FIND_MATCHES: usize = 20;
const MAX_FIND_SNIPPET_CHARS: usize = 320;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WebFindEvidence {
    pub(crate) page_url: String,
    pub(crate) query: String,
    pub(crate) matches: Vec<String>,
}

pub(crate) fn find_in_page(
    page: &WebPageEvidence,
    query: &str,
) -> Result<WebFindEvidence, AppError> {
    let query = query.trim();
    if query.is_empty() {
        return Err(AppError::usage("페이지에서 찾을 텍스트가 필요합니다."));
    }
    if query.chars().count() > MAX_FIND_QUERY_CHARS
        || query.chars().any(|character| character.is_control())
    {
        return Err(AppError::usage(format!(
            "페이지 찾기 텍스트는 제어 문자 없이 최대 {MAX_FIND_QUERY_CHARS}자까지 허용합니다."
        )));
    }
    let folded_query = query.to_lowercase();
    let matches = page
        .content
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            (!line.is_empty() && line.to_lowercase().contains(&folded_query))
                .then(|| bounded_snippet(line))
        })
        .take(MAX_FIND_MATCHES)
        .collect();
    Ok(WebFindEvidence {
        page_url: page.final_url.clone(),
        query: query.to_string(),
        matches,
    })
}

fn bounded_snippet(line: &str) -> String {
    let mut snippet = line
        .chars()
        .take(MAX_FIND_SNIPPET_CHARS)
        .collect::<String>();
    if line.chars().count() > MAX_FIND_SNIPPET_CHARS {
        snippet.push('…');
    }
    snippet
}
