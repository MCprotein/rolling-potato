use std::collections::{BTreeMap, BTreeSet};

use sha2::{Digest, Sha256};

use crate::foundation::error::AppError;

use super::policy::canonicalize_source_url;

pub(super) const MAX_SEARCH_CONTEXT_CHARS: usize = 6 * 1024;
pub(super) const MAX_SOURCES: usize = 8;
const MAX_SOURCES_PER_DOMAIN: usize = 2;
const SOURCE_ID_HEX_CHARS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WebSourceEvidence {
    pub(crate) source_id: String,
    pub(crate) url: String,
    pub(crate) title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WebSearchEvidence {
    pub(crate) context: String,
    pub(crate) sources: Vec<WebSourceEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WebPageEvidence {
    pub(crate) source_id: String,
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
    query: &str,
    results: Vec<SearchResult>,
) -> Result<WebSearchEvidence, AppError> {
    let ranked = rank_and_deduplicate(query, results);
    let mut context = String::new();
    let mut sources = Vec::new();
    for result in ranked {
        let source = source_evidence(&result);
        let section = format!(
            "Source ID: {}\nTitle: {}\nURL: {}\nDescription: {}",
            source.source_id,
            sanitize_context(&source.title),
            source.url,
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
        sources.push(source);
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

pub(super) fn stable_source_id(url: &str) -> String {
    let canonical = canonicalize_source_url(url).unwrap_or_else(|| url.trim().to_string());
    let digest = Sha256::digest(canonical.as_bytes());
    let mut hex = String::with_capacity(SOURCE_ID_HEX_CHARS);
    for byte in digest.iter().take(SOURCE_ID_HEX_CHARS / 2) {
        hex.push_str(&format!("{byte:02x}"));
    }
    format!("source-{hex}")
}

fn source_evidence(result: &SearchResult) -> WebSourceEvidence {
    WebSourceEvidence {
        source_id: stable_source_id(&result.url),
        url: result.url.clone(),
        title: sanitize_context(&result.title),
    }
}

fn rank_and_deduplicate(query: &str, results: Vec<SearchResult>) -> Vec<SearchResult> {
    let terms = normalized_terms(query);
    let mut seen_urls = BTreeSet::new();
    let mut candidates = results
        .into_iter()
        .enumerate()
        .filter_map(|(index, mut result)| {
            let canonical = canonicalize_source_url(&result.url)?;
            if !seen_urls.insert(canonical.clone()) {
                return None;
            }
            result.url = canonical;
            let score = relevance_score(&result, &terms);
            Some((score, index, result))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));

    let mut domain_counts = BTreeMap::<String, usize>::new();
    let mut ranked = Vec::new();
    for (_, _, result) in candidates {
        let Some(domain) = source_domain(&result.url) else {
            continue;
        };
        let count = domain_counts.entry(domain).or_default();
        if *count >= MAX_SOURCES_PER_DOMAIN {
            continue;
        }
        *count += 1;
        ranked.push(result);
        if ranked.len() == MAX_SOURCES {
            break;
        }
    }
    ranked
}

fn relevance_score(result: &SearchResult, terms: &[String]) -> i16 {
    let title = result.title.to_lowercase();
    let description = result.description.to_lowercase();
    let url = result.url.to_ascii_lowercase();
    let domain = source_domain(&result.url);
    let mut score = 0_i16;
    for term in terms {
        if title.contains(term) {
            score = score.saturating_add(4);
        }
        if url.contains(term) {
            score = score.saturating_add(2);
        }
        if description.contains(term) {
            score = score.saturating_add(1);
        }
    }
    if [
        "official",
        "공식",
        "documentation",
        "docs",
        "release notes",
        "릴리스 노트",
    ]
    .iter()
    .any(|signal| title.contains(signal))
    {
        score = score.saturating_add(8);
    }
    if [
        "/docs",
        "/documentation",
        "/releases",
        "/release-notes",
        "/news",
    ]
    .iter()
    .any(|signal| url.contains(signal))
    {
        score = score.saturating_add(4);
    }
    if domain.as_deref().is_some_and(|domain| {
        domain.ends_with(".gov")
            || domain.ends_with(".edu")
            || domain.starts_with("docs.")
            || domain.starts_with("developer.")
    }) {
        score = score.saturating_add(4);
    }
    if domain.as_deref().is_some_and(|domain| {
        domain.split('.').any(|label| {
            terms
                .iter()
                .any(|term| is_brand_domain_term(term) && label == term)
        })
    }) {
        score = score.saturating_add(12);
    }
    if domain.as_deref().is_some_and(|domain| {
        [
            "blog.naver.com",
            "tistory.com",
            "medium.com",
            "reddit.com",
            "quora.com",
        ]
        .iter()
        .any(|candidate| domain == *candidate || domain.ends_with(&format!(".{candidate}")))
    }) {
        score = score.saturating_sub(12);
    }
    if ["개인 블로그", "블로그", "예상", "전망", "prediction"]
        .iter()
        .any(|signal| title.contains(signal) || description.contains(signal))
    {
        score = score.saturating_sub(6);
    }
    score
}

fn is_brand_domain_term(term: &str) -> bool {
    term.len() >= 3
        && term.is_ascii()
        && term.chars().all(|character| character.is_alphanumeric())
        && !matches!(
            term,
            "official"
                | "result"
                | "benchmark"
                | "release"
                | "version"
                | "docs"
                | "documentation"
                | "world"
                | "cup"
        )
}

fn normalized_terms(query: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split(|character: char| !character.is_alphanumeric())
        .filter(|term| term.chars().count() > 1)
        .map(str::to_string)
        .collect()
}

fn source_domain(url: &str) -> Option<String> {
    url.parse::<ureq::http::Uri>()
        .ok()?
        .authority()
        .map(|authority| authority.host().to_ascii_lowercase())
}

fn sanitize_context(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\t'))
        .collect::<String>()
        .trim()
        .to_string()
}
