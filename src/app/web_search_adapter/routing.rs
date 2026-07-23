#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WebToolRoute {
    Search { query: String },
    Open { url: String },
    Find { query: String },
}

pub(crate) fn route_tool_request(request: &str) -> Option<WebToolRoute> {
    let request = request.trim();
    if let Some(query) = request.strip_prefix("/search ") {
        return nonempty(query).map(|query| WebToolRoute::Search {
            query: query.to_string(),
        });
    }
    if let Some(url) = request.strip_prefix("/open ") {
        return nonempty(url).map(|url| WebToolRoute::Open {
            url: url.to_string(),
        });
    }
    if let Some(query) = request.strip_prefix("/find ") {
        return nonempty(query).map(|query| WebToolRoute::Find {
            query: query.to_string(),
        });
    }
    None
}

pub(crate) fn parse_agent_web_tool(response: &str) -> Option<WebToolRoute> {
    const MAX_AGENT_TOOL_INPUT_CHARS: usize = 512;

    let lines = response
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let [tool_line, input_line] = lines.as_slice() else {
        return None;
    };
    let tool = tool_line.strip_prefix("WEB TOOL:")?.trim();
    let input = input_line.strip_prefix("WEB INPUT:")?.trim();
    if input.is_empty()
        || input.contains(['\r', '\n'])
        || input.chars().count() > MAX_AGENT_TOOL_INPUT_CHARS
    {
        return None;
    }
    match tool {
        "search" => Some(WebToolRoute::Search {
            query: input.to_string(),
        }),
        "open" => Some(WebToolRoute::Open {
            url: input.to_string(),
        }),
        "find" => Some(WebToolRoute::Find {
            query: input.to_string(),
        }),
        _ => None,
    }
}

pub(crate) fn web_disabled(request: &str) -> bool {
    let request = request.trim();
    let lower = request.to_ascii_lowercase();
    has_no_web_directive(&lower)
        || [
            "검색하지마",
            "검색하지 마",
            "검색 금지",
            "인터넷 쓰지마",
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
