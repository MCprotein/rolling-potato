const MAX_CONTEXT_TURNS: usize = 3;
const MAX_QUERY_CHARS: usize = 512;

pub(crate) fn contextualize_search_input(
    proposed_query: &str,
    current_request: &str,
    prior_user_requests: &[&str],
) -> Option<String> {
    if !projects_to_user_requests(proposed_query, current_request, prior_user_requests) {
        return None;
    }

    let proposed = clean_search_phrase(proposed_query);
    if is_self_contained(&proposed) || prior_user_requests.is_empty() {
        return nonempty_bounded(&proposed).or_else(|| nonempty_bounded(proposed_query));
    }

    let mut fragments = prior_user_requests
        .iter()
        .rev()
        .filter_map(|request| {
            let fragment = clean_search_phrase(request);
            has_topic(&fragment).then_some(fragment)
        })
        .take(MAX_CONTEXT_TURNS)
        .collect::<Vec<_>>();
    fragments.reverse();
    if has_topic(&proposed) && !fragments.iter().any(|fragment| fragment == &proposed) {
        fragments.push(proposed);
    }

    nonempty_bounded(&fragments.join(" ")).or_else(|| nonempty_bounded(proposed_query))
}

fn projects_to_user_requests(
    proposed_query: &str,
    current_request: &str,
    prior_user_requests: &[&str],
) -> bool {
    let proposed = proposed_query.trim().to_lowercase();
    if proposed.is_empty() {
        return false;
    }
    if current_request.to_lowercase().contains(&proposed) {
        return true;
    }

    let mut user_context = prior_user_requests.join("\n");
    user_context.push('\n');
    user_context.push_str(current_request);
    let compact_context = compact(&user_context.to_lowercase());
    let terms = semantic_terms(&proposed);
    !terms.is_empty()
        && terms
            .iter()
            .all(|term| compact_context.contains(&compact(term)))
}

fn clean_search_phrase(value: &str) -> String {
    let mut cleaned = value.trim().to_string();
    for directive in [
        "인터넷에서 검색해줘",
        "인터넷에서 찾아줘",
        "웹에서 검색해줘",
        "웹에서 찾아줘",
        "검색해서 알려줘",
        "검색하여 알려줘",
        "찾아보고 알려줘",
        "찾아보라고",
        "검색해봐",
        "검색해 줘",
        "검색해줘",
        "검색해서",
        "검색하여",
        "검색해",
        "찾아봐줘",
        "찾아봐",
        "찾아 줘",
        "찾아줘",
        "웹에서",
        "인터넷에서",
        "look it up",
        "search for",
        "browse for",
        "on the web",
    ] {
        cleaned = cleaned.replace(directive, " ");
    }
    let mut cleaned = cleaned.trim().to_string();
    loop {
        let before = cleaned.clone();
        for filler in ["아니", "그럼", "그러면", "그거", "그걸", "좀", "제발"] {
            if let Some(rest) = cleaned.strip_prefix(filler) {
                cleaned = rest.trim_start().to_string();
                break;
            }
        }
        if cleaned == before {
            break;
        }
    }
    cleaned
        .trim_matches(|character: char| {
            character.is_whitespace()
                || character.is_ascii_punctuation()
                || matches!(character, '？' | '。' | '！' | '…' | '·')
        })
        .split_whitespace()
        .filter(|term| {
            !matches!(
                *term,
                "끝낫어" | "끝났어" | "끝났냐" | "했어" | "했냐" | "됐어" | "됐냐"
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn semantic_terms(value: &str) -> Vec<&str> {
    value
        .split(|character: char| !character.is_alphanumeric())
        .filter(|term| term.chars().count() >= 2)
        .filter(|term| {
            !matches!(
                *term,
                "검색"
                    | "검색해"
                    | "찾아"
                    | "찾아봐"
                    | "알려줘"
                    | "아니"
                    | "그럼"
                    | "그러면"
                    | "그거"
                    | "그걸"
                    | "please"
                    | "search"
                    | "browse"
                    | "find"
            )
        })
        .collect()
}

fn is_self_contained(value: &str) -> bool {
    let terms = semantic_terms(value);
    terms.len() >= 2
        || terms.first().is_some_and(|term| {
            term.len() == term.chars().count()
                && term
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric())
                && term.len() >= 3
        })
}

fn has_topic(value: &str) -> bool {
    !semantic_terms(value).is_empty()
}

fn compact(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .collect()
}

fn nonempty_bounded(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.chars().take(MAX_QUERY_CHARS).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contextual_followup_query_uses_only_recent_user_requests() {
        let prior = ["월드컵 우승국가가 어디야", "2026년은?"];
        assert_eq!(
            contextualize_search_input("2026 월드컵 우승 국가", "검색해봐 끝낫어", &prior),
            Some("2026 월드컵 우승 국가".to_string())
        );
        assert!(
            contextualize_search_input("SECRET attachment value", "검색해봐 끝낫어", &prior)
                .is_none()
        );
    }

    #[test]
    fn raw_meta_search_request_is_rewritten_with_recent_topic() {
        let query = contextualize_search_input(
            "검색해봐 끝낫어",
            "검색해봐 끝낫어",
            &["월드컵 우승국가가 어디야", "2026년은?"],
        )
        .unwrap();

        assert!(query.contains("월드컵"));
        assert!(query.contains("2026"));
        assert!(!query.contains("끝낫어"));
        assert!(!query.contains("검색해봐"));
    }
}
