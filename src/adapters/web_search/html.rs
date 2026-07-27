use crate::foundation::error::AppError;

use super::evidence::SearchResult;
use super::policy::canonicalize_source_url;

const HTML_RESULT_LINK_CLASS: &str = "result__a";
const HTML_RESULT_SNIPPET_CLASS: &str = "result__snippet";
const LITE_RESULT_LINK_CLASS: &str = "result-link";
const LITE_RESULT_SNIPPET_CLASS: &str = "result-snippet";
const MAX_PARSED_RESULTS: usize = 32;

pub(super) fn parse_html_search_results(document: &str) -> Result<Vec<SearchResult>, AppError> {
    parse_results(document, HTML_RESULT_LINK_CLASS, HTML_RESULT_SNIPPET_CLASS)
}

pub(super) fn parse_lite_search_results(document: &str) -> Result<Vec<SearchResult>, AppError> {
    parse_results(document, LITE_RESULT_LINK_CLASS, LITE_RESULT_SNIPPET_CLASS)
}

pub(super) fn normalize_result_url(raw: &str) -> Option<String> {
    let decoded = decode_html_entities(raw);
    if let Some(canonical) = canonicalize_source_url(&decoded) {
        return Some(canonical);
    }
    let query = decoded
        .strip_prefix("//duckduckgo.com/l/?")
        .or_else(|| decoded.strip_prefix("https://duckduckgo.com/l/?"))
        .or_else(|| decoded.strip_prefix("/l/?"))?;
    let encoded = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("uddg="))?;
    let target = percent_decode(encoded)?;
    canonicalize_source_url(&target)
}

fn parse_results(
    document: &str,
    link_class: &str,
    snippet_class: &str,
) -> Result<Vec<SearchResult>, AppError> {
    reject_challenge_document(document)?;
    let mut results = Vec::new();
    let mut cursor = 0;
    let mut markers = 0;
    while let Some(class_offset) = document[cursor..].find(link_class) {
        markers += 1;
        let class_index = cursor + class_offset;
        let Some(tag_start) = document[..class_index].rfind("<a") else {
            cursor = class_index + link_class.len();
            continue;
        };
        let Some(tag_end_offset) = document[class_index..].find('>') else {
            break;
        };
        let tag_end = class_index + tag_end_offset;
        let opening_tag = &document[tag_start..=tag_end];
        let Some(href) = attribute_value(opening_tag, "href") else {
            cursor = tag_end + 1;
            continue;
        };
        let Some(title_end_offset) = document[tag_end + 1..].find("</a>") else {
            break;
        };
        let title_end = tag_end + 1 + title_end_offset;
        let title = text_content(&document[tag_end + 1..title_end]);
        let next_cursor = title_end + "</a>".len();
        let next_result = document[next_cursor..]
            .find(link_class)
            .map_or(document.len(), |offset| next_cursor + offset);
        let description = extract_class_text(&document[next_cursor..next_result], snippet_class);
        cursor = next_cursor;

        let Some(url) = normalize_result_url(href) else {
            continue;
        };
        if title.is_empty() {
            continue;
        }
        results.push(SearchResult {
            title,
            url,
            description,
        });
        if results.len() == MAX_PARSED_RESULTS {
            break;
        }
    }

    if results.is_empty() {
        let reason = if markers == 0 {
            "검색 결과 marker가 없습니다."
        } else {
            "검색 결과 marker는 있지만 parser contract를 만족하는 항목이 없습니다."
        };
        return Err(AppError::blocked(format!(
            "직접 웹 검색 문서를 해석하지 못했습니다. {reason}"
        )));
    }
    Ok(results)
}

fn reject_challenge_document(document: &str) -> Result<(), AppError> {
    let lower = document.to_ascii_lowercase();
    if [
        "anomaly-modal",
        "challenge-form",
        "bots use duckduckgo",
        "unfortunately, bots",
    ]
    .iter()
    .any(|signal| lower.contains(signal))
    {
        return Err(AppError::runtime(
            "직접 웹 검색 endpoint가 자동 요청 확인 문서를 반환했습니다.",
        ));
    }
    Ok(())
}

fn extract_class_text(fragment: &str, class_name: &str) -> String {
    let Some(class_index) = fragment.find(class_name) else {
        return String::new();
    };
    let Some(tag_end_offset) = fragment[class_index..].find('>') else {
        return String::new();
    };
    let content_start = class_index + tag_end_offset + 1;
    let Some(content_end_offset) = fragment[content_start..].find("</") else {
        return String::new();
    };
    text_content(&fragment[content_start..content_start + content_end_offset])
}

fn attribute_value<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let marker = format!("{name}=");
    let start = tag.find(&marker)? + marker.len();
    let quote = tag[start..].chars().next()?;
    if !matches!(quote, '"' | '\'') {
        return None;
    }
    let value_start = start + quote.len_utf8();
    let value_end = tag[value_start..].find(quote)? + value_start;
    Some(&tag[value_start..value_end])
}

fn text_content(fragment: &str) -> String {
    let mut text = String::with_capacity(fragment.len());
    let mut inside_tag = false;
    for character in fragment.chars() {
        match character {
            '<' => inside_tag = true,
            '>' if inside_tag => {
                inside_tag = false;
                text.push(' ');
            }
            _ if !inside_tag => text.push(character),
            _ => {}
        }
    }
    collapse_whitespace(&decode_html_entities(&text))
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn decode_html_entities(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                let high = *bytes.get(index + 1)?;
                let low = *bytes.get(index + 2)?;
                decoded.push((hex_value(high)? << 4) | hex_value(low)?);
                index += 3;
            }
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).ok()
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}
