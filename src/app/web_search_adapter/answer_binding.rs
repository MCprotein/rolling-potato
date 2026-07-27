use std::collections::{BTreeMap, BTreeSet};

use crate::adapters::web_search::WebSourceEvidence;

const WEB_ANSWER_FALLBACK: &str =
    "웹 검색은 완료했지만 로컬 모델이 요약을 완성하지 못했습니다. 아래 검증 가능한 출처를 확인하세요.";

pub(super) fn render_grounded_answer(
    answer: Option<String>,
    sources: &[WebSourceEvidence],
) -> String {
    let source_map = sources
        .iter()
        .map(|source| (source.source_id.as_str(), source))
        .collect::<BTreeMap<_, _>>();
    let sanitized = answer
        .map(|answer| sanitize_with_sources(&answer, &source_map))
        .filter(|answer| !answer.is_empty())
        .unwrap_or_else(|| WEB_ANSWER_FALLBACK.to_string());
    attach_verified_sources(&sanitized, sources)
}

pub(super) fn sanitize_model_summary(answer: &str) -> String {
    sanitize_with_sources(answer, &BTreeMap::new())
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
        let without_urls = strip_model_urls(&citations);
        let normalized = without_urls
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

fn attach_verified_sources(answer: &str, sources: &[WebSourceEvidence]) -> String {
    let mut rendered = String::new();
    let mut attached = BTreeSet::new();
    for paragraph in answer
        .split("\n\n")
        .filter(|paragraph| !paragraph.trim().is_empty())
    {
        if !rendered.is_empty() {
            rendered.push_str("\n\n");
        }
        rendered.push_str(paragraph);
        for source in sources
            .iter()
            .filter(|source| paragraph.contains(&format!("[{}]", source.source_id)))
        {
            rendered.push_str(&format!(
                "\n근거 · [{}] {} — {}",
                source.source_id, source.title, source.url
            ));
            attached.insert(source.source_id.as_str());
        }
    }
    if attached.is_empty() {
        rendered.push_str("\n\n검증된 출처");
        for source in sources {
            rendered.push_str(&format!(
                "\n- [{}] {} — {}",
                source.source_id, source.title, source.url
            ));
        }
    }
    rendered
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

#[cfg(test)]
mod tests {
    use super::*;

    fn source(id: &str, title: &str, url: &str) -> WebSourceEvidence {
        WebSourceEvidence {
            source_id: id.to_string(),
            title: title.to_string(),
            url: url.to_string(),
        }
    }

    #[test]
    fn invalid_citations_and_model_urls_cannot_replace_runtime_sources() {
        let answer = render_grounded_answer(
            Some(
                "확인된 주장 [source-good](https://evil.example/swap). 가짜 [source-bad]. 숫자 [1], 배열 [1, 2], a[1]."
                    .to_string(),
            ),
            &[source(
                "source-good",
                "Primary document",
                "https://example.com/verified",
            )],
        );

        assert!(answer.contains("[source-good]"));
        assert!(answer.contains("https://example.com/verified"));
        assert!(!answer.contains("source-bad"));
        assert!(!answer.contains("evil.example"));
        assert!(!answer.contains("숫자 [1]"));
        assert!(answer.contains("[1, 2]"));
        assert!(answer.contains("a[1]"));
    }

    #[test]
    fn verified_sources_are_attached_to_the_paragraph_that_cites_them() {
        let answer = render_grounded_answer(
            Some("첫 주장 [source-one]\n\n둘째 주장은 불확실합니다 [source-two]".to_string()),
            &[
                source("source-one", "One", "https://example.com/one"),
                source("source-two", "Two", "https://example.com/two"),
            ],
        );

        assert!(answer
            .contains("첫 주장 [source-one]\n근거 · [source-one] One — https://example.com/one"));
        assert!(answer.contains(
            "둘째 주장은 불확실합니다 [source-two]\n근거 · [source-two] Two — https://example.com/two"
        ));
        assert!(!answer.contains("\n\n검증된 출처"));
    }

    #[test]
    fn unusable_or_uncited_answers_keep_a_runtime_owned_source_fallback() {
        let source = source(
            "source-release",
            "Release notes",
            "https://example.com/releases/v1",
        );
        for answer in [
            None,
            Some("요약은 생성됐지만 marker가 없습니다.".to_string()),
        ] {
            let rendered = render_grounded_answer(answer, std::slice::from_ref(&source));

            assert!(rendered.contains("검증된 출처"));
            assert!(rendered
                .contains("- [source-release] Release notes — https://example.com/releases/v1"));
        }
    }
}
