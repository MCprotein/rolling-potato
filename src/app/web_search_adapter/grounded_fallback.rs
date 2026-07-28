use super::WebGroundingEvidence;

const PASSAGE_CHARS: usize = 320;

pub(super) fn render(
    user_request: &str,
    conversation_context: &str,
    grounding: &[WebGroundingEvidence],
) -> Option<String> {
    let title = grounding
        .iter()
        .max_by_key(|evidence| evidence_score(user_request, &evidence.title, true))?;
    let passage = grounding
        .iter()
        .flat_map(|evidence| {
            evidence
                .excerpt
                .lines()
                .map(str::trim)
                .filter(|line| meaningful_chars(line) >= 8)
                .map(move |line| (evidence, line))
        })
        .max_by_key(|(_, line)| evidence_score(user_request, line, false));
    let title_text = bounded_chars(title.title.trim());
    let (passage_source, passage_text) = passage
        .map(|(evidence, text)| (evidence, bounded_chars(text.trim())))
        .unwrap_or((title, String::new()));
    let name = request_mentions_name(user_request)
        .then(|| remembered_name(conversation_context))
        .flatten()
        .map(|name| format!("{name}님, "))
        .unwrap_or_default();

    if passage_text.is_empty() || passage_text == title_text {
        return Some(format!(
            "{name}검색 근거에는 “{title_text}”라고 표기되어 있습니다. [{}]",
            title.source_id
        ));
    }
    Some(format!(
        "{name}검색 근거에는 “{title_text}”라고 표기되어 있고 [{}], 핵심 관련 원문에는 “{passage_text}”라고 적혀 있습니다. [{}]",
        title.source_id, passage_source.source_id
    ))
}

fn evidence_score(user_request: &str, candidate: &str, is_title: bool) -> usize {
    let request = user_request.to_lowercase();
    let candidate = candidate.to_lowercase();
    let mut score = request
        .split(|character: char| !character.is_alphanumeric())
        .filter_map(normalized_query_term)
        .filter(|term| candidate.contains(term))
        .count()
        * 3;
    if is_title {
        score += 4;
    }
    if asks_for_english_name(&request) && contains_multiword_english_name(&candidate) {
        score += 12;
    }
    if asks_for_purpose(&request)
        && ["목적", "목표", "위해", "위한", "purpose", "aim", "goal"]
            .iter()
            .any(|marker| candidate.contains(marker))
    {
        score += 10;
    }
    score
}

fn normalized_query_term(term: &str) -> Option<&str> {
    let term = term.trim();
    if term.chars().count() < 2 || is_query_stopword(term) {
        return None;
    }
    let normalized = [
        "에서", "으로", "라고", "이랑", "하고", "에게", "한테", "부터", "까지", "의", "은", "는",
        "이", "가", "을", "를", "와", "과", "도",
    ]
    .iter()
    .find_map(|suffix| {
        term.strip_suffix(suffix)
            .filter(|value| value.chars().count() >= 2)
    })
    .unwrap_or(term);
    (!is_query_stopword(normalized)).then_some(normalized)
}

fn is_query_stopword(term: &str) -> bool {
    matches!(
        term,
        "방금"
            | "검색한"
            | "검색"
            | "다시"
            | "말해줘"
            | "알려줘"
            | "그리고"
            | "함께"
            | "불러줘"
            | "문장"
            | "please"
            | "search"
            | "tell"
    )
}

fn asks_for_english_name(request: &str) -> bool {
    ["영문명", "영어명", "정식 명칭", "full name", "english name"]
        .iter()
        .any(|marker| request.contains(marker))
}

fn contains_multiword_english_name(candidate: &str) -> bool {
    candidate
        .split(|character: char| !character.is_ascii_alphabetic())
        .filter(|term| term.len() >= 3)
        .take(3)
        .count()
        >= 3
}

fn asks_for_purpose(request: &str) -> bool {
    ["목적", "목표", "왜", "purpose", "aim", "goal"]
        .iter()
        .any(|marker| request.contains(marker))
}

fn request_mentions_name(request: &str) -> bool {
    let request = request.to_lowercase();
    ["이름", "불러", "호칭", "name"]
        .iter()
        .any(|marker| request.contains(marker))
}

fn remembered_name(conversation_context: &str) -> Option<String> {
    ["내 이름은 ", "제 이름은 ", "이름은 "]
        .iter()
        .flat_map(|marker| {
            conversation_context
                .match_indices(marker)
                .map(move |(index, _)| (index, *marker))
        })
        .max_by_key(|(index, _)| *index)
        .and_then(|(index, marker)| {
            let value = &conversation_context[index + marker.len()..];
            let mut candidate = value
                .split(['.', '!', '?', '\n', '"', '\\'])
                .next()
                .unwrap_or_default()
                .trim()
                .to_string();
            for suffix in [
                "라고 기억해줘",
                "라고 해줘",
                "라고 합니다",
                "입니다",
                "이에요",
                "예요",
                "이야",
                "야",
            ] {
                if let Some(trimmed) = candidate.strip_suffix(suffix) {
                    candidate = trimmed.trim().to_string();
                    break;
                }
            }
            let length = candidate.chars().count();
            (1..=32)
                .contains(&length)
                .then_some(candidate)
                .filter(|value| !value.chars().any(char::is_control))
        })
}

fn meaningful_chars(value: &str) -> usize {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .count()
}

fn bounded_chars(value: &str) -> String {
    value.chars().take(PASSAGE_CHARS).collect()
}
