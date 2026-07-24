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
        "text/html" | "application/xhtml+xml" | "" => scan_html(document),
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

fn scan_html(document: &str) -> (Option<String>, String) {
    let mut output = String::with_capacity(document.len().min(MAX_PAGE_CONTEXT_CHARS * 2));
    let mut title = String::new();
    let mut cursor = 0;
    let mut in_title = false;
    let mut hidden = None::<(HiddenElement, usize)>;
    while let Some(start_offset) = document[cursor..].find('<') {
        let start = cursor + start_offset;
        if hidden.is_none() {
            append_visible_text(&document[cursor..start], in_title, &mut output, &mut title);
        }
        let Some(end_offset) = document[start..].find('>') else {
            break;
        };
        let end = start + end_offset + 1;
        if let Some(tag) = parse_tag(&document[start + 1..end - 1]) {
            if let Some((hidden_element, depth)) = hidden.as_mut() {
                if hidden_element.matches(tag.name) {
                    if tag.closing {
                        *depth -= 1;
                        if *depth == 0 {
                            hidden = None;
                        }
                    } else if !tag.self_closing {
                        *depth += 1;
                    }
                }
            } else if let Some(hidden_element) = HiddenElement::from_name(tag.name) {
                if !tag.closing && !tag.self_closing {
                    hidden = Some((hidden_element, 1));
                }
            } else {
                if tag.name.eq_ignore_ascii_case("title") {
                    in_title = !tag.closing;
                }
                if is_structural_tag(tag.name) {
                    output.push('\n');
                }
            }
        }
        cursor = end;
    }
    if hidden.is_none() {
        append_visible_text(&document[cursor..], in_title, &mut output, &mut title);
    }
    let title = collapse_text(&decode_html_entities(&title));
    let text = collapse_text(&decode_html_entities(&output));
    ((!title.is_empty()).then_some(title), text)
}

fn append_visible_text(value: &str, in_title: bool, output: &mut String, title: &mut String) {
    output.push_str(value);
    if in_title {
        title.push_str(value);
    }
}

#[derive(Clone, Copy)]
struct HtmlTag<'a> {
    name: &'a str,
    closing: bool,
    self_closing: bool,
}

fn parse_tag(value: &str) -> Option<HtmlTag<'_>> {
    let value = value.trim();
    if value.is_empty() || value.starts_with(['!', '?']) {
        return None;
    }
    let closing = value.starts_with('/');
    let value = value.strip_prefix('/').unwrap_or(value).trim_start();
    let name_end = value
        .find(|character: char| character.is_whitespace() || character == '/')
        .unwrap_or(value.len());
    let name = &value[..name_end];
    if name.is_empty() {
        return None;
    }
    Some(HtmlTag {
        name,
        closing,
        self_closing: value.trim_end().ends_with('/'),
    })
}

#[derive(Clone, Copy)]
enum HiddenElement {
    Script,
    Style,
    NoScript,
    Svg,
}

impl HiddenElement {
    fn from_name(name: &str) -> Option<Self> {
        if name.eq_ignore_ascii_case("script") {
            Some(Self::Script)
        } else if name.eq_ignore_ascii_case("style") {
            Some(Self::Style)
        } else if name.eq_ignore_ascii_case("noscript") {
            Some(Self::NoScript)
        } else if name.eq_ignore_ascii_case("svg") {
            Some(Self::Svg)
        } else {
            None
        }
    }

    fn matches(self, name: &str) -> bool {
        match self {
            Self::Script => name.eq_ignore_ascii_case("script"),
            Self::Style => name.eq_ignore_ascii_case("style"),
            Self::NoScript => name.eq_ignore_ascii_case("noscript"),
            Self::Svg => name.eq_ignore_ascii_case("svg"),
        }
    }
}

fn is_structural_tag(name: &str) -> bool {
    [
        "br", "p", "div", "main", "article", "section", "header", "footer", "nav", "li", "tr",
        "h1", "h2", "h3", "h4", "h5", "h6",
    ]
    .iter()
    .any(|candidate| name.eq_ignore_ascii_case(candidate))
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
