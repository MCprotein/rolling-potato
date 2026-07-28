use crate::app::web_search_adapter::web_disabled;

const MAX_BROWSER_QUERY_CHARS: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BrowserSearchRequest {
    pub(crate) url: String,
    pub(crate) query: String,
}

pub(crate) fn deterministic_browser_fallback(request: &str) -> Option<BrowserSearchRequest> {
    if web_disabled(request) {
        return None;
    }
    let lower = request.to_lowercase();
    let url = if lower.contains("네이버") || lower.contains("naver") {
        "https://www.naver.com/"
    } else if lower.contains("구글") || lower.contains("google") {
        "https://www.google.com/"
    } else {
        return None;
    };
    let asks_to_open = ["열고", "들어가", "접속", "open ", "go to ", "visit "]
        .iter()
        .any(|signal| lower.contains(signal));
    let asks_to_interact = ["검색창", "검색란", "입력", "써줘", "search for ", "type "]
        .iter()
        .any(|signal| lower.contains(signal));
    if !asks_to_open || !asks_to_interact {
        return None;
    }
    let query = quoted_query(request)
        .or_else(|| korean_search_field_query(request))
        .or_else(|| english_search_query(request))?;
    bounded_request(url, &query)
}

fn bounded_request(url: &str, query: &str) -> Option<BrowserSearchRequest> {
    let url = url.trim();
    let query = trim_query(query);
    if url.is_empty()
        || url.len() > 2_048
        || query.is_empty()
        || query.chars().count() > MAX_BROWSER_QUERY_CHARS
        || url.chars().any(char::is_control)
        || query.chars().any(char::is_control)
    {
        return None;
    }
    Some(BrowserSearchRequest {
        url: url.to_string(),
        query,
    })
}

fn quoted_query(request: &str) -> Option<String> {
    for (open, close) in [('"', '"'), ('\'', '\''), ('“', '”'), ('‘', '’')] {
        let Some(start) = request.find(open).map(|index| index + open.len_utf8()) else {
            continue;
        };
        let Some(rest) = request.get(start..) else {
            continue;
        };
        let Some(end) = rest.find(close) else {
            continue;
        };
        let value = trim_query(rest.get(..end)?);
        if !value.is_empty() {
            return Some(value);
        }
    }
    None
}

fn korean_search_field_query(request: &str) -> Option<String> {
    for marker in ["검색창에", "검색란에"] {
        let Some((_, rest)) = request.split_once(marker) else {
            continue;
        };
        let end = ["입력", "써", "검색"]
            .iter()
            .filter_map(|signal| rest.find(signal))
            .min()
            .unwrap_or(rest.len());
        let query = trim_query(rest.get(..end)?);
        if !query.is_empty() {
            return Some(query);
        }
    }
    None
}

fn english_search_query(request: &str) -> Option<String> {
    let lower = request.to_ascii_lowercase();
    for marker in ["search for ", "type "] {
        let Some(start) = lower.find(marker).map(|index| index + marker.len()) else {
            continue;
        };
        let Some(rest) = request.get(start..) else {
            continue;
        };
        let Some(lower_rest) = lower.get(start..) else {
            continue;
        };
        let end = [
            " in the search",
            " into the search",
            " and press",
            " and hit",
        ]
        .iter()
        .filter_map(|signal| lower_rest.find(signal))
        .min()
        .unwrap_or(rest.len());
        let query = trim_query(rest.get(..end)?);
        if !query.is_empty() {
            return Some(query);
        }
    }
    None
}

fn trim_query(value: &str) -> String {
    let value = value
        .trim()
        .trim_matches(|character: char| {
            character.is_whitespace()
                || matches!(
                    character,
                    '"' | '\'' | '“' | '”' | '‘' | '’' | ',' | '.' | '?' | '!'
                )
        })
        .trim();
    let value = ["이라고", "라고", "을", "를"]
        .iter()
        .find_map(|suffix| value.strip_suffix(suffix))
        .unwrap_or(value);
    value.trim().to_string()
}
