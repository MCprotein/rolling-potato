use crate::foundation::error::AppError;

pub(super) const MAX_SEARCH_CONTEXT_CHARS: usize = 6 * 1024;
pub(super) const MAX_SOURCES: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WebSearchEvidence {
    pub(crate) context: String,
    pub(crate) sources: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WebPageEvidence {
    pub(crate) requested_url: String,
    pub(crate) final_url: String,
    pub(crate) title: Option<String>,
    pub(crate) content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WebOpenResult {
    Opened(WebPageEvidence),
    Redirect {
        from_url: String,
        target_url: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SearchResult {
    pub(super) title: String,
    pub(super) url: String,
    pub(super) description: String,
}

pub(super) fn evidence_from_results(
    results: &[SearchResult],
) -> Result<WebSearchEvidence, AppError> {
    let mut context = String::new();
    let mut sources = Vec::new();
    for result in results {
        if sources.iter().any(|stored| stored == &result.url) {
            continue;
        }
        let section = format!(
            "Title: {}\nURL: {}\nDescription: {}",
            sanitize_context(&result.title),
            result.url,
            sanitize_context(&result.description)
        );
        let separator = if context.is_empty() {
            ""
        } else {
            "\n\n---\n\n"
        };
        let remaining = MAX_SEARCH_CONTEXT_CHARS.saturating_sub(context.chars().count());
        if remaining <= separator.chars().count() {
            break;
        }
        let bounded_section = section
            .chars()
            .take(remaining - separator.chars().count())
            .collect::<String>();
        if bounded_section.trim().is_empty() {
            break;
        }
        context.push_str(separator);
        context.push_str(&bounded_section);
        sources.push(result.url.clone());
        if context.chars().count() == MAX_SEARCH_CONTEXT_CHARS {
            break;
        }
    }
    if sources.is_empty() {
        return Err(AppError::blocked(
            "웹 검색 결과가 작은 모델용 context 한도 안에 들어오지 않았습니다.",
        ));
    }
    Ok(WebSearchEvidence { context, sources })
}

fn sanitize_context(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\t'))
        .collect::<String>()
        .trim()
        .to_string()
}
