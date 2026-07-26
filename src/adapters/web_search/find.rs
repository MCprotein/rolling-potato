use crate::foundation::error::AppError;

use super::evidence::WebPageEvidence;

const MAX_FIND_QUERY_CHARS: usize = 160;
const MAX_FIND_MATCHES: usize = 20;
const MAX_FIND_CONTEXT_CHARS: usize = 480;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WebFindMatch {
    pub(crate) line_number: usize,
    pub(crate) context: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WebFindEvidence {
    pub(crate) source_id: String,
    pub(crate) page_url: String,
    pub(crate) query: String,
    pub(crate) matches: Vec<WebFindMatch>,
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
    let lines = page.content.lines().map(str::trim).collect::<Vec<_>>();
    let folded_query = query.to_lowercase();
    let matches = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| !line.is_empty() && line.to_lowercase().contains(&folded_query))
        .map(|(index, _)| WebFindMatch {
            line_number: index + 1,
            context: contextual_snippet(&lines, index),
        })
        .take(MAX_FIND_MATCHES)
        .collect();
    Ok(WebFindEvidence {
        source_id: page.source_id.clone(),
        page_url: page.final_url.clone(),
        query: query.to_string(),
        matches,
    })
}

fn contextual_snippet(lines: &[&str], matched_index: usize) -> String {
    let start = matched_index.saturating_sub(1);
    let end = (matched_index + 2).min(lines.len());
    let context = lines[start..end]
        .iter()
        .enumerate()
        .filter(|(_, line)| !line.is_empty())
        .map(|(offset, line)| format!("{}: {}", start + offset + 1, line))
        .collect::<Vec<_>>()
        .join("\n");
    bounded_context(&context)
}

fn bounded_context(context: &str) -> String {
    let mut snippet = context
        .chars()
        .take(MAX_FIND_CONTEXT_CHARS)
        .collect::<String>();
    if context.chars().count() > MAX_FIND_CONTEXT_CHARS {
        snippet.push('…');
    }
    snippet
}
