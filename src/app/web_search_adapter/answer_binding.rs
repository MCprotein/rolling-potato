use std::collections::{BTreeMap, BTreeSet};

use crate::adapters::web_search::WebSourceEvidence;

const WEB_ANSWER_FALLBACK: &str =
    "웹 검색은 완료했지만 로컬 모델이 요약을 완성하지 못했습니다. 아래 검증 가능한 출처를 확인하세요.";

pub(super) fn render_grounded_answer(
    user_request: &str,
    generated: Option<String>,
    fallback: Option<String>,
    sources: &[WebSourceEvidence],
) -> String {
    let source_map = sources
        .iter()
        .map(|source| (source.source_id.as_str(), source))
        .collect::<BTreeMap<_, _>>();
    let sanitized = generated
        .filter(|answer| candidate_answers_request(user_request, answer))
        .and_then(|answer| sanitize_grounded_candidate(&answer, &source_map))
        .or_else(|| {
            fallback
                .filter(|answer| candidate_answers_request(user_request, answer))
                .and_then(|answer| sanitize_grounded_candidate(&answer, &source_map))
        })
        .unwrap_or_else(|| WEB_ANSWER_FALLBACK.to_string());
    attach_verified_sources(&sanitized, sources)
}

pub(super) fn asks_for_winner(request: &str) -> bool {
    let lower = request.to_lowercase();
    [
        "우승국",
        "우승 국가",
        "우승팀",
        "우승자",
        "winner",
        "who won",
    ]
    .iter()
    .any(|signal| lower.contains(signal))
}

pub(super) fn contains_concrete_winner_claim(answer: &str) -> bool {
    let lower = answer.to_lowercase();
    [
        "우승국은",
        "우승 국가는",
        "우승팀은",
        "우승자는",
        "우승했",
        "우승을 차지",
        "winner is",
        "champion is",
        "champions",
        "won the",
    ]
    .iter()
    .any(|signal| lower.contains(signal))
}

fn candidate_answers_request(request: &str, answer: &str) -> bool {
    let answer_lower = answer.to_lowercase();
    if asks_for_winner(request) {
        return contains_concrete_winner_claim(answer) || contains_uncertainty(&answer_lower);
    }
    if super::routing::is_empirical_comparison_request(request) {
        let request_lower = request.to_lowercase();
        let adds_unrequested_license = !["라이선스", "license"]
            .iter()
            .any(|signal| request_lower.contains(signal))
            && ["라이선스", "license"]
                .iter()
                .any(|signal| answer_lower.contains(signal));
        let answer_without_citation = answer_without_trailing_source_citation(answer);
        let unfinished = ["다음과 같습니다:", "아래와 같습니다:", "as follows:"]
            .iter()
            .any(|suffix| answer_without_citation.to_lowercase().ends_with(suffix));
        return !adds_unrequested_license
            && !unfinished
            && (contains_uncertainty(&answer_lower)
                || [
                    "벤치마크",
                    "점수",
                    "파라미터",
                    "모델 크기",
                    "양자화",
                    "속도",
                    "지연",
                    "정확도",
                    "평가 작업",
                    "benchmark",
                    "parameter",
                    "quantization",
                    "latency",
                    "accuracy",
                ]
                .iter()
                .any(|signal| answer_lower.contains(signal)));
    }
    true
}

fn answer_without_trailing_source_citation(answer: &str) -> &str {
    let answer = answer.trim_end();
    answer
        .rfind("[source-")
        .filter(|start| answer[*start..].ends_with(']'))
        .map(|start| answer[..start].trim_end())
        .unwrap_or(answer)
}

fn contains_uncertainty(answer_lower: &str) -> bool {
    [
        "확인할 수 없",
        "확인되지 않",
        "확인하지 못",
        "근거가 없",
        "알 수 없",
        "단정할 수 없",
        "unable to verify",
        "not verified",
        "cannot verify",
    ]
    .iter()
    .any(|signal| answer_lower.contains(signal))
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

fn sanitize_grounded_candidate(
    answer: &str,
    sources: &BTreeMap<&str, &WebSourceEvidence>,
) -> Option<String> {
    let sanitized = sanitize_with_sources(answer, sources);
    (!sanitized.is_empty() && contains_verified_citation(&sanitized, sources)).then_some(sanitized)
}

fn contains_verified_citation(answer: &str, sources: &BTreeMap<&str, &WebSourceEvidence>) -> bool {
    sources
        .keys()
        .any(|source_id| answer.contains(&format!("[{source_id}]")))
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
#[path = "answer_binding/tests.rs"]
mod tests;
