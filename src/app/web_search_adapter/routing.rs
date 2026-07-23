#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WebToolRoute {
    Open { url: String },
    Find { query: String },
}

pub(crate) fn route_tool_request(request: &str) -> Option<WebToolRoute> {
    let request = request.trim();
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

pub(crate) fn should_search(request: &str) -> bool {
    let request = request.trim();
    if request.is_empty() {
        return false;
    }
    let lower = request.to_ascii_lowercase();
    if ["검색하지마", "검색하지 마", "오프라인", "인터넷 쓰지마"]
        .iter()
        .any(|signal| request.contains(signal))
        || ["do not browse", "don't browse", "offline"]
            .iter()
            .any(|signal| contains_ascii_phrase(&lower, signal))
    {
        return false;
    }
    let explicit_web = ["인터넷", "웹에서", "웹 검색", "온라인"]
        .iter()
        .any(|signal| request.contains(signal))
        || [
            "web search",
            "search online",
            "look up online",
            "browse the web",
        ]
        .iter()
        .any(|signal| contains_ascii_phrase(&lower, signal));
    if explicit_web {
        return true;
    }
    let local_scope = [
        "저장소",
        "프로젝트",
        "코드",
        "파일",
        "디렉터리",
        "경로",
        "소스",
    ]
    .iter()
    .any(|signal| request.contains(signal))
        || ["repository", "repo", "codebase", "source file"]
            .iter()
            .any(|signal| contains_ascii_phrase(&lower, signal));
    if local_scope {
        return false;
    }
    if [
        "검색해",
        "찾아줘",
        "찾아 줘",
        "찾아봐",
        "찾아 봐",
        "알아봐",
        "알아 봐",
        "조회해",
        "확인해줘",
        "확인해 줘",
    ]
    .iter()
    .any(|signal| request.contains(signal))
    {
        return true;
    }
    let dynamic_result = ["결과", "우승", "스코어", "순위", "당선"]
        .iter()
        .any(|signal| request.contains(signal))
        && ["월드컵", "올림픽", "경기", "대회", "선거", "시상식", "리그"]
            .iter()
            .any(|signal| request.contains(signal));
    if dynamic_result {
        return true;
    }
    [
        "최신",
        "현재",
        "오늘",
        "지금",
        "뉴스",
        "날씨",
        "주가",
        "환율",
        "가격",
        "일정",
        "출시",
        "대통령",
        "총리",
        "대표이사",
    ]
    .iter()
    .any(|signal| lower.contains(signal))
        || [
            "latest", "current", "today", "news", "weather", "price", "schedule", "ceo",
        ]
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
