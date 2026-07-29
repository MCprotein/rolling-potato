use std::collections::BTreeMap;

use crate::adapters::web_search::WebSourceEvidence;

pub(super) fn grounded_candidate(
    answer: &str,
    sources: &BTreeMap<&str, &WebSourceEvidence>,
) -> Option<String> {
    let sanitized = sanitize_with_sources(answer, sources);
    (!sanitized.is_empty() && contains_verified_citation(&sanitized, sources)).then_some(sanitized)
}

fn sanitize_with_sources(answer: &str, sources: &BTreeMap<&str, &WebSourceEvidence>) -> String {
    let mut lines = Vec::new();
    for line in answer.lines() {
        let trimmed = line.trim();
        if is_source_heading(trimmed) {
            break;
        }
        if is_numeric_reference_definition(trimmed) {
            continue;
        }
        let citations = sanitize_citation_markers(line, sources);
        let normalized = strip_model_urls(&citations)
            .replace("( )", "")
            .replace(" .", ".")
            .replace(" ,", ",");
        if normalized
            .chars()
            .any(|character| character.is_alphanumeric())
        {
            lines.push(normalized.trim_end().to_string());
        } else if trimmed.is_empty() && lines.last().is_some_and(|line| !line.is_empty()) {
            lines.push(String::new());
        }
    }
    lines.join("\n").trim().to_string()
}

fn contains_verified_citation(answer: &str, sources: &BTreeMap<&str, &WebSourceEvidence>) -> bool {
    sources
        .keys()
        .any(|source_id| answer.contains(&format!("[{source_id}]")))
}

fn is_source_heading(line: &str) -> bool {
    matches!(
        line.trim_end_matches(':')
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "출처" | "참고 링크" | "source" | "sources" | "references"
    )
}

fn is_numeric_reference_definition(line: &str) -> bool {
    let Some(candidate) = line.strip_prefix('[') else {
        return false;
    };
    let Some((marker, rest)) = candidate.split_once(']') else {
        return false;
    };
    is_citation_number(marker) && rest.trim_start().starts_with(':')
}

fn sanitize_citation_markers(answer: &str, sources: &BTreeMap<&str, &WebSourceEvidence>) -> String {
    let mut cleaned = String::with_capacity(answer.len());
    let mut remaining = answer;
    while let Some(start) = remaining.find('[') {
        cleaned.push_str(&remaining[..start]);
        let candidate = &remaining[start + 1..];
        let Some(end) = candidate.find(']') else {
            cleaned.push_str(&remaining[start..]);
            return cleaned;
        };
        let marker = &candidate[..end];
        let after_marker = &candidate[end + 1..];
        let boundary_before = cleaned
            .chars()
            .last()
            .is_none_or(|character| !character.is_ascii_alphanumeric() && character != '_');
        let boundary_after = after_marker
            .chars()
            .next()
            .is_none_or(|character| !character.is_ascii_alphanumeric() && character != '_');
        let numeric_citation = boundary_before && boundary_after && is_citation_number(marker);
        let source_citation = marker.starts_with("source-");
        if numeric_citation || source_citation {
            if source_citation && sources.contains_key(marker) {
                cleaned.push('[');
                cleaned.push_str(marker);
                cleaned.push(']');
            }
            remaining = strip_markdown_link(after_marker);
            continue;
        }
        cleaned.push('[');
        cleaned.push_str(marker);
        cleaned.push(']');
        remaining = after_marker;
    }
    cleaned.push_str(remaining);
    cleaned
}

fn strip_markdown_link(value: &str) -> &str {
    let Some(link) = value.strip_prefix('(') else {
        return value;
    };
    let Some(close) = link.find(')') else {
        return value;
    };
    let target = &link[..close];
    if target.starts_with("https://") || target.starts_with("http://") {
        &link[close + 1..]
    } else {
        value
    }
}

fn is_citation_number(marker: &str) -> bool {
    !marker.is_empty()
        && marker.len() <= 2
        && marker.chars().all(|character| character.is_ascii_digit())
}

fn strip_model_urls(text: &str) -> String {
    let mut cleaned = String::with_capacity(text.len());
    let mut remaining = text;
    loop {
        let http = remaining.find("http://");
        let https = remaining.find("https://");
        let start = match (http, https) {
            (Some(http), Some(https)) => http.min(https),
            (Some(http), None) => http,
            (None, Some(https)) => https,
            (None, None) => {
                cleaned.push_str(remaining);
                break;
            }
        };
        cleaned.push_str(&remaining[..start]);
        if matches!(cleaned.chars().last(), Some('(' | '<')) {
            cleaned.pop();
        }
        let url = &remaining[start..];
        let end = url
            .char_indices()
            .find_map(|(index, character)| character.is_whitespace().then_some(index))
            .unwrap_or(url.len());
        remaining = &url[end..];
    }
    cleaned
}
