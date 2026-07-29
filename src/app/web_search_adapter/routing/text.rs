//! Small, domain-neutral lexical helpers used by routing and evidence selection.

pub(in crate::app::web_search_adapter) fn overlap_score(reference: &str, candidate: &str) -> usize {
    let candidate = candidate.to_lowercase();
    content_terms(reference)
        .iter()
        .filter(|term| candidate.contains(term.as_str()))
        .count()
}

pub(in crate::app::web_search_adapter) fn best_query_term(query: &str) -> Option<String> {
    content_terms(query)
        .into_iter()
        .filter(|term| !is_query_hint(term))
        .max_by_key(|term| term.chars().count())
}

fn content_terms(value: &str) -> Vec<String> {
    value
        .to_lowercase()
        .split(|character: char| !character.is_alphanumeric() && character != '-')
        .filter_map(normalize_term)
        .map(str::to_string)
        .collect()
}

fn normalize_term(term: &str) -> Option<&str> {
    let term = term.trim();
    if term.chars().count() < 2 || is_stopword(term) {
        return None;
    }
    let normalized = [
        "에서", "으로", "라고", "이랑", "하고", "에게", "한테", "부터", "까지", "의", "은", "는",
        "이", "가", "을", "를", "와", "과", "도", "에",
    ]
    .iter()
    .find_map(|suffix| {
        term.strip_suffix(suffix)
            .filter(|value| value.chars().count() >= 2)
    })
    .unwrap_or(term);
    (!is_stopword(normalized)).then_some(normalized)
}

fn is_stopword(term: &str) -> bool {
    matches!(
        term,
        "방금"
            | "검색한"
            | "검색"
            | "검색해"
            | "검색해서"
            | "다시"
            | "페이지"
            | "현재"
            | "열린"
            | "찾아줘"
            | "찾아봐"
            | "찾아보"
            | "말해줘"
            | "알려줘"
            | "그리고"
            | "함께"
            | "불러줘"
            | "문장"
            | "근거"
            | "출처"
            | "맞춰"
            | "답해줘"
            | "설명해줘"
            | "정식"
            | "영문명"
            | "목적"
            | "이름"
            | "please"
            | "search"
            | "find"
            | "locate"
            | "this"
            | "current"
            | "opened"
            | "page"
            | "within"
            | "tell"
            | "evidence"
            | "source"
            | "again"
    )
}

fn is_query_hint(term: &str) -> bool {
    matches!(
        term,
        "공식" | "official" | "result" | "benchmark" | "methodology"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexical_helpers_are_domain_neutral_and_bounded_to_content_terms() {
        assert_eq!(
            best_query_term("Rust stable release 2026 공식 official"),
            Some("release".to_string())
        );
        assert!(
            overlap_score(
                "alpha-model beta-model 성능 비교",
                "alpha-model benchmark results"
            ) > 0
        );
        assert_eq!(best_query_term("a 1 공식 official"), None);
    }
}
