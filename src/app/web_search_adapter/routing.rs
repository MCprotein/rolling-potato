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
    if let Some(query) =
        korean_page_find_query(request).or_else(|| english_page_find_query(request))
    {
        return Some(WebToolRoute::Find { query });
    }
    let url = first_web_url(request)?;
    let without_url = request.replace(url, "");
    let lower = without_url.to_ascii_lowercase();
    let open_signal = without_url.trim().is_empty()
        || ["열어", "읽어", "요약", "내용", "확인"]
            .iter()
            .any(|signal| without_url.contains(signal))
        || ["open", "read", "summarize", "fetch"]
            .iter()
            .any(|signal| contains_ascii_phrase(&lower, signal));
    open_signal.then(|| WebToolRoute::Open {
        url: url.to_string(),
    })
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
    ["검색하지마", "검색하지 마", "오프라인", "인터넷 쓰지마"]
        .iter()
        .any(|signal| request.contains(signal))
        || ["do not browse", "don't browse", "offline"]
            .iter()
            .any(|signal| contains_ascii_phrase(&lower, signal))
}

fn nonempty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn first_web_url(request: &str) -> Option<&str> {
    let start = request
        .find("https://")
        .or_else(|| request.find("http://"))?;
    let candidate = request[start..]
        .split_whitespace()
        .next()?
        .trim_end_matches(['.', ',', ';', '!', '?', ')', ']', '}', '>', '"', '\'']);
    (!candidate.is_empty()).then_some(candidate)
}

fn korean_page_find_query(request: &str) -> Option<String> {
    for marker in ["이 페이지에서", "현재 페이지에서", "페이지에서", "문서에서"]
    {
        let Some((_, tail)) = request.split_once(marker) else {
            continue;
        };
        let tail = tail.trim();
        let Some(end) = tail.find("찾").or_else(|| tail.find("검색")) else {
            continue;
        };
        let query = tail[..end].trim().trim_matches(['"', '\'', '`']);
        if !query.is_empty() {
            return Some(query.to_string());
        }
    }
    None
}

fn english_page_find_query(request: &str) -> Option<String> {
    let lower = request.to_ascii_lowercase();
    let tail = lower.strip_prefix("find ")?;
    let end = tail
        .find(" in this page")
        .or_else(|| tail.find(" on this page"))
        .or_else(|| tail.find(" in the page"))?;
    let query = request[request.len() - tail.len()..][..end]
        .trim()
        .trim_matches(['"', '\'', '`']);
    (!query.is_empty()).then(|| query.to_string())
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
