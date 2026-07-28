const MAX_QUERY_CHARS: usize = 512;

pub(super) fn clean_search_phrase(value: &str) -> String {
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

pub(super) fn semantic_terms(value: &str) -> Vec<&str> {
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

pub(super) fn is_self_contained(value: &str) -> bool {
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

pub(super) fn has_topic(value: &str) -> bool {
    !semantic_terms(value).is_empty()
}

pub(super) fn compact(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .collect()
}

pub(super) fn nonempty_bounded(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.chars().take(MAX_QUERY_CHARS).collect())
}
