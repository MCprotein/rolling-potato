use super::research::WebResearchStep;
use super::WebGroundingEvidence;
use crate::foundation::error::AppError;

pub(crate) fn route_tool_request(request: &str) -> Option<WebResearchStep> {
    let request = request.trim();
    if let Some(query) = request.strip_prefix("/search ") {
        return nonempty(query).map(|query| WebResearchStep::Search {
            query: query.to_string(),
        });
    }
    if let Some(url) = request.strip_prefix("/open ") {
        return nonempty(url).map(|url| WebResearchStep::Open {
            url: url.to_string(),
        });
    }
    if let Some(query) = request.strip_prefix("/find ") {
        return nonempty(query).map(|query| WebResearchStep::Find {
            query: query.to_string(),
        });
    }
    None
}

pub(crate) fn parse_agent_web_tool(response: &str) -> Option<WebResearchStep> {
    const MAX_AGENT_TOOL_INPUT_CHARS: usize = 512;

    let lines = response
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.is_empty() || lines.len() > 2 {
        return None;
    }
    let mut tool = None;
    let mut input = None;
    for line in lines {
        let (label, value) = line.split_once(':')?;
        match normalized_protocol_label(label).as_str() {
            "webtool" if tool.is_none() => tool = nonempty(value),
            "webinput" if input.is_none() => input = nonempty(value),
            _ => return None,
        }
    }
    let input = input?;
    if input.is_empty()
        || input.contains(['\r', '\n'])
        || input.chars().count() > MAX_AGENT_TOOL_INPUT_CHARS
    {
        return None;
    }
    match tool.unwrap_or("search").to_ascii_lowercase().as_str() {
        "search" => Some(WebResearchStep::Search {
            query: input.to_string(),
        }),
        "open" => Some(WebResearchStep::Open {
            url: input.to_string(),
        }),
        "find" => Some(WebResearchStep::Find {
            query: input.to_string(),
        }),
        _ => None,
    }
}

pub(crate) fn parse_agent_web_tool_for_request(
    response: &str,
    current_request: &str,
) -> Option<WebResearchStep> {
    if conversational_progress_followup(current_request) {
        return None;
    }
    let step = parse_agent_web_tool(response)?;
    literal_projection(step.input(), current_request).then_some(step)
}

pub(crate) fn validate_public_web_step(step: WebResearchStep) -> Result<WebResearchStep, AppError> {
    let candidate = match &step {
        WebResearchStep::Search { query } => Some(query.as_str()),
        WebResearchStep::Open { url } => Some(url.as_str()),
        WebResearchStep::Find { .. } => None,
    };
    if candidate.is_some_and(contains_credential_like_value) {
        return Err(AppError::blocked(
            "검색어나 URL에 인증정보로 보이는 값이 있어 외부 요청을 차단했습니다. 비밀값을 제거한 공개 검색어로 다시 요청하세요.",
        ));
    }
    Ok(step)
}

pub(crate) fn is_grounded_followup_request(request: &str) -> bool {
    has_explicit_prior_web_reference(request) || is_natural_regrounding_request(request)
}

pub(crate) fn can_reuse_prior_grounding(request: &str, grounding: &[WebGroundingEvidence]) -> bool {
    if grounding.is_empty() || !is_grounded_followup_request(request) {
        return false;
    }
    has_explicit_prior_web_reference(request)
        || request_topic_overlaps_grounding(request, grounding)
}

fn has_explicit_prior_web_reference(request: &str) -> bool {
    let lower = request.trim().to_ascii_lowercase();
    [
        "방금 검색",
        "아까 검색",
        "검색한 ",
        "검색 결과",
        "검색결과",
        "그 출처",
        "해당 출처",
        "출처에서",
        "방금 찾",
        "아까 찾",
        "웹에서 찾",
        "방금 연 ",
        "아까 연 ",
    ]
    .iter()
    .any(|signal| request.contains(signal))
        || [
            "the search result",
            "those search results",
            "the source",
            "those sources",
            "you just search",
            "you just searched",
            "you found earlier",
        ]
        .iter()
        .any(|signal| lower.contains(signal))
}

fn is_natural_regrounding_request(request: &str) -> bool {
    let lower = request.to_ascii_lowercase();
    let asks_again = ["다시", "재설명", "재답변", "again"]
        .iter()
        .any(|signal| lower.contains(signal));
    let asks_for_evidence = ["근거", "출처", "evidence", "source"]
        .iter()
        .any(|signal| lower.contains(signal));
    asks_again && asks_for_evidence
}

fn request_topic_overlaps_grounding(request: &str, grounding: &[WebGroundingEvidence]) -> bool {
    let request = request.to_lowercase();
    request
        .split(|character: char| !character.is_alphanumeric())
        .filter_map(normalized_topic_term)
        .any(|term| {
            grounding.iter().any(|evidence| {
                evidence.title.to_lowercase().contains(term)
                    || evidence.excerpt.to_lowercase().contains(term)
            })
        })
}

fn normalized_topic_term(term: &str) -> Option<&str> {
    let term = term.trim();
    if term.chars().count() < 3 {
        return None;
    }
    let normalized = [
        "에서", "으로", "라고", "이랑", "하고", "에게", "한테", "부터", "까지", "의", "은", "는",
        "이", "가", "을", "를", "와", "과", "도", "에",
    ]
    .iter()
    .find_map(|suffix| {
        term.strip_suffix(suffix)
            .filter(|value| value.chars().count() >= 3)
    })
    .unwrap_or(term);
    (!matches!(
        normalized,
        "근거"
            | "출처"
            | "맞춰"
            | "다시"
            | "답해줘"
            | "설명해줘"
            | "정식"
            | "영문명"
            | "목적"
            | "이름"
            | "불러줘"
            | "evidence"
            | "source"
            | "again"
    ))
    .then_some(normalized)
}

pub(crate) fn web_disabled(request: &str) -> bool {
    let request = request.trim();
    let lower = request.to_ascii_lowercase();
    has_no_web_directive(&lower)
        || [
            "검색하지마",
            "검색하지 마",
            "검색하지 말",
            "검색 금지",
            "인터넷 쓰지마",
            "인터넷 쓰지 말",
            "인터넷 사용하지",
            "인터넷 없이",
            "웹 사용하지",
            "웹 없이",
            "외부 검색하지",
            "외부 네트워크에 연결하지",
            "외부 네트워크 연결하지",
            "네트워크에 연결하지",
            "네트워크 사용하지",
            "외부 연결하지",
            "오프라인",
        ]
        .iter()
        .any(|signal| request.contains(signal))
        || [
            "do not browse",
            "don't browse",
            "do not search",
            "don't search",
            "do not use the internet",
            "do not access the network",
            "don't access the network",
            "do not connect to the network",
            "don't connect to the network",
            "do not make network requests",
            "don't make network requests",
            "without browsing",
            "without internet",
            "without network access",
            "no web",
            "no browsing",
            "no network access",
            "offline",
        ]
        .iter()
        .any(|signal| contains_ascii_phrase(&lower, signal))
}

fn has_no_web_directive(request: &str) -> bool {
    request.strip_prefix("--no-web").is_some_and(|remaining| {
        remaining.is_empty() || remaining.chars().next().is_some_and(char::is_whitespace)
    })
}

fn nonempty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn normalized_protocol_label(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn conversational_progress_followup(request: &str) -> bool {
    let compact = request
        .trim()
        .to_ascii_lowercase()
        .chars()
        .filter(|character| !character.is_whitespace() && !character.is_ascii_punctuation())
        .collect::<String>();
    if compact.is_empty() {
        return true;
    }
    if compact.chars().count() > 16 || has_explicit_web_intent(&compact) {
        return false;
    }
    ["왜", "뭐", "뭔", "무슨", "그래서", "어떻게", "어디까지"]
        .iter()
        .any(|prefix| compact.starts_with(prefix))
        || ["하고있", "하는중", "검색중", "되고있", "진행중"]
            .iter()
            .any(|signal| compact.contains(signal))
}

fn has_explicit_web_intent(request: &str) -> bool {
    ["검색", "찾아", "웹", "인터넷", "search", "browse", "web"]
        .iter()
        .any(|signal| request.contains(signal))
}

fn literal_projection(input: &str, current_request: &str) -> bool {
    let input = input.trim().to_lowercase();
    let current_request = current_request.trim().to_lowercase();
    !input.is_empty() && current_request.contains(&input)
}

fn contains_credential_like_value(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    if ["authorization:", "bearer ", "basic "]
        .iter()
        .any(|signal| lower.contains(signal))
    {
        return true;
    }
    if [
        "api_key",
        "apikey",
        "api-key",
        "access_token",
        "access-token",
        "password",
        "passwd",
        "client_secret",
        "client-secret",
        "credential",
    ]
    .iter()
    .any(|name| {
        ["=", ":", "%3d"]
            .iter()
            .any(|separator| lower.contains(&format!("{name}{separator}")))
    }) {
        return true;
    }
    lower
        .split(|character: char| {
            character.is_whitespace()
                || matches!(character, '"' | '\'' | '&' | '?' | ',' | ';' | '(' | ')')
        })
        .any(|token| {
            let token = token.trim_matches(|character: char| {
                !character.is_ascii_alphanumeric() && !matches!(character, '-' | '_')
            });
            (token.starts_with("sk-") && token.len() >= 12)
                || (token.starts_with("ghp_") && token.len() >= 12)
                || (token.starts_with("github_pat_") && token.len() >= 20)
                || (token.starts_with("xoxb-") && token.len() >= 12)
                || (token.starts_with("akia") && token.len() >= 16)
        })
}

fn contains_ascii_phrase(text: &str, phrase: &str) -> bool {
    let words = ascii_words(text);
    let phrase = ascii_words(phrase);
    !phrase.is_empty() && words.windows(phrase.len()).any(|window| window == phrase)
}

fn ascii_words(text: &str) -> Vec<&str> {
    text.split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_web_steps_reject_credential_like_current_input() {
        for step in [
            WebResearchStep::Search {
                query: "latest release api_key=SECRET-123".to_string(),
            },
            WebResearchStep::Search {
                query: "Authorization: Bearer eyJhbGciOiJIUzI1NiJ9".to_string(),
            },
            WebResearchStep::Search {
                query: "search sk-1234567890abcdef".to_string(),
            },
            WebResearchStep::Open {
                url: "https://example.com/?access_token=SECRET".to_string(),
            },
        ] {
            let error = validate_public_web_step(step).unwrap_err();
            assert!(error.message.contains("외부 요청을 차단"));
            assert!(!error.message.contains("SECRET"));
        }
    }

    #[test]
    fn ordinary_public_search_terms_and_page_find_are_allowed() {
        assert!(validate_public_web_step(WebResearchStep::Search {
            query: "OAuth access token 보안 모범 사례".to_string(),
        })
        .is_ok());
        assert!(validate_public_web_step(WebResearchStep::Find {
            query: "password policy".to_string(),
        })
        .is_ok());
    }

    #[test]
    fn only_explicitly_referential_questions_use_prior_web_grounding() {
        for request in [
            "방금 검색한 ESPR의 정식 명칭은?",
            "그 출처에서 핵심 목적을 다시 설명해줘",
            "ESPR 정식 영문명과 목적을 근거에 맞춰 다시 답해줘",
            "What did you just search?",
        ] {
            assert!(is_grounded_followup_request(request), "{request}");
        }
        for request in ["오늘 날씨 검색해줘", "내 이름이 뭐였지?", "Rust를 설명해줘"]
        {
            assert!(!is_grounded_followup_request(request), "{request}");
        }
    }

    #[test]
    fn natural_regrounding_requires_topic_overlap_with_cached_evidence() {
        let grounding = vec![WebGroundingEvidence {
            source_id: "source-espr".to_string(),
            title: "Ecodesign for Sustainable Products Regulation (ESPR)".to_string(),
            url: "https://example.com/espr".to_string(),
            excerpt: "ESPR은 지속가능한 제품을 위한 EU 규정입니다.".to_string(),
        }];

        assert!(can_reuse_prior_grounding(
            "ESPR 정식 영문명과 목적을 근거에 맞춰 다시 답해줘",
            &grounding
        ));
        assert!(!can_reuse_prior_grounding(
            "Rust 소유권을 근거에 맞춰 다시 설명해줘",
            &grounding
        ));
        assert!(can_reuse_prior_grounding(
            "그 출처에서 목적을 다시 설명해줘",
            &grounding
        ));
    }
}
