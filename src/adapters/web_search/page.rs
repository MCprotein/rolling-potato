use crate::foundation::error::AppError;

use super::evidence::WebPageEvidence;

const MAX_PAGE_CONTEXT_CHARS: usize = 24_000;

pub(super) fn parse_page_document(
    requested_url: &str,
    final_url: &str,
    document: &str,
    content_type: &str,
) -> Result<WebPageEvidence, AppError> {
    let mut page = normalize_page_text(final_url, document, content_type)?;
    page.requested_url = requested_url.to_string();
    Ok(page)
}

pub(super) fn normalize_page_text(
    url: &str,
    document: &str,
    content_type: &str,
) -> Result<WebPageEvidence, AppError> {
    let media_type = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let (title, text) = match media_type.as_str() {
        "text/html" | "application/xhtml+xml" | "" => {
            let title = extract_title(document);
            (title, html_to_text(document))
        }
        "text/plain" | "application/json" => (None, collapse_text(document)),
        _ => {
            return Err(AppError::blocked(format!(
                "WebOpen은 HTML, plain text, JSON 문서만 읽을 수 있습니다: {media_type}"
            )))
        }
    };
    let content = text
        .chars()
        .take(MAX_PAGE_CONTEXT_CHARS)
        .collect::<String>();
    if content.trim().is_empty() {
        return Err(AppError::blocked(
            "WebOpen 문서에서 읽을 수 있는 텍스트를 찾지 못했습니다.",
        ));
    }
    Ok(WebPageEvidence {
        requested_url: url.to_string(),
        final_url: url.to_string(),
        title,
        content,
    })
}

fn extract_title(document: &str) -> Option<String> {
    let lower = document.to_ascii_lowercase();
    let start = lower.find("<title")?;
    let open_end = lower[start..].find('>')? + start + 1;
    let close = lower[open_end..].find("</title>")? + open_end;
    let title = collapse_text(&strip_tags(&document[open_end..close]));
    (!title.is_empty()).then_some(title)
}

fn html_to_text(document: &str) -> String {
    let without_active = remove_elements(document, &["script", "style", "noscript", "svg"]);
    let with_breaks = add_structural_breaks(&without_active);
    collapse_text(&strip_tags(&with_breaks))
}

fn remove_elements(document: &str, names: &[&str]) -> String {
    let mut output = document.to_string();
    for name in names {
        loop {
            let lower = output.to_ascii_lowercase();
            let Some(start) = lower.find(&format!("<{name}")) else {
                break;
            };
            let Some(open_end) = lower[start..].find('>').map(|offset| start + offset + 1) else {
                output.truncate(start);
                break;
            };
            let close_marker = format!("</{name}>");
            let end = lower[open_end..]
                .find(&close_marker)
                .map_or(open_end, |offset| open_end + offset + close_marker.len());
            output.replace_range(start..end, " ");
        }
    }
    output
}

fn add_structural_breaks(document: &str) -> String {
    let mut output = String::with_capacity(document.len());
    let mut cursor = 0;
    while let Some(start_offset) = document[cursor..].find('<') {
        let start = cursor + start_offset;
        output.push_str(&document[cursor..start]);
        let Some(end_offset) = document[start..].find('>') else {
            break;
        };
        let end = start + end_offset + 1;
        let tag = document[start + 1..end - 1]
            .trim_start_matches('/')
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .trim_end_matches('/')
            .to_ascii_lowercase();
        if matches!(
            tag.as_str(),
            "br" | "p"
                | "div"
                | "main"
                | "article"
                | "section"
                | "header"
                | "footer"
                | "nav"
                | "li"
                | "tr"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
        ) {
            output.push('\n');
        }
        output.push_str(&document[start..end]);
        cursor = end;
    }
    output.push_str(&document[cursor..]);
    output
}

fn strip_tags(document: &str) -> String {
    let mut output = String::with_capacity(document.len());
    let mut inside_tag = false;
    for character in document.chars() {
        match character {
            '<' => inside_tag = true,
            '>' if inside_tag => inside_tag = false,
            _ if !inside_tag => output.push(character),
            _ => {}
        }
    }
    decode_html_entities(&output)
}

fn collapse_text(value: &str) -> String {
    let mut output = String::new();
    for line in value.lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            continue;
        }
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&line);
    }
    output
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
