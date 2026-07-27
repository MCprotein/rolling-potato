use std::collections::BTreeSet;

use crate::foundation::error::AppError;

use super::evidence::{stable_source_id, WebPageEvidence};

pub(super) const MAX_PAGE_CONTEXT_CHARS: usize = 24_000;

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
        "text/markdown" | "text/x-markdown" => scan_markdown(document),
        "application/rss+xml" | "application/atom+xml" => scan_feed(document)?,
        "application/xml" | "text/xml" if looks_like_feed(document) => scan_feed(document)?,
        "text/plain" | "application/json" => (None, collapse_text(document)),
        _ => {
            return Err(AppError::blocked(format!(
                "WebOpen은 HTML, plain text, JSON, Markdown, RSS, Atom 문서만 읽을 수 있습니다: {media_type}"
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
        source_id: stable_source_id(url),
        requested_url: url.to_string(),
        final_url: url.to_string(),
        title,
        content,
    })
}

fn scan_html(document: &str) -> (Option<String>, String) {
    let collected = scan_markup(document, MarkupMode::Html);
    let primary = collapse_unique_text(&decode_html_entities(&collected.primary));
    let fallback = collapse_unique_text(&decode_html_entities(&collected.fallback));
    let text = if primary.is_empty() {
        fallback
    } else {
        primary
    };
    let title = collapse_text(&decode_html_entities(&collected.title));
    ((!title.is_empty()).then_some(title), text)
}

fn scan_markdown(document: &str) -> (Option<String>, String) {
    let title = document.lines().find_map(|line| {
        let line = line.trim();
        line.strip_prefix("# ")
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_string)
    });
    (title, collapse_text(document))
}

fn scan_feed(document: &str) -> Result<(Option<String>, String), AppError> {
    if !looks_like_feed(document) {
        return Err(AppError::blocked(
            "WebOpen XML 문서가 RSS 또는 Atom feed 형식이 아닙니다.",
        ));
    }
    let collected = scan_markup(document, MarkupMode::Feed);
    let titles = collapse_text(&decode_html_entities(&collected.title));
    let mut title_lines = titles.lines();
    let title = title_lines.next().map(str::to_string);
    let entry_titles = title_lines.collect::<Vec<_>>().join("\n");
    let text = collapse_unique_text(&decode_html_entities(&format!(
        "{entry_titles}\n{}\n{}",
        collected.primary, collected.fallback
    )));
    Ok((title, text))
}

fn looks_like_feed(document: &str) -> bool {
    let prefix = document
        .chars()
        .take(4_096)
        .collect::<String>()
        .to_ascii_lowercase();
    prefix.contains("<rss")
        || prefix.contains("<feed")
        || prefix.contains("<rdf:rdf")
        || prefix.contains("<channel")
}

#[derive(Clone, Copy)]
enum MarkupMode {
    Html,
    Feed,
}

#[derive(Default)]
struct MarkupText {
    title: String,
    primary: String,
    fallback: String,
}

#[derive(Debug)]
struct HiddenScope {
    name: String,
    depth: usize,
}

fn scan_markup(document: &str, mode: MarkupMode) -> MarkupText {
    let mut collected = MarkupText::default();
    let mut cursor = 0;
    let mut in_title = false;
    let mut in_head = false;
    let mut primary_depth = 0_usize;
    let mut hidden = None::<HiddenScope>;

    while let Some(start_offset) = document[cursor..].find('<') {
        let start = cursor + start_offset;
        if hidden.is_none() {
            append_markup_text(
                &document[cursor..start],
                in_title,
                in_head,
                primary_depth,
                &mut collected,
            );
        }
        if document[start..].starts_with("<!--") {
            let Some(end_offset) = document[start + 4..].find("-->") else {
                break;
            };
            cursor = start + 4 + end_offset + 3;
            continue;
        }
        if document[start..].starts_with("<![CDATA[") {
            let Some(end_offset) = document[start + 9..].find("]]>") else {
                break;
            };
            if hidden.is_none() {
                let embedded = scan_markup(
                    &document[start + 9..start + 9 + end_offset],
                    MarkupMode::Html,
                );
                let embedded = if embedded.primary.trim().is_empty() {
                    embedded.fallback
                } else {
                    embedded.primary
                };
                append_markup_text(&embedded, in_title, in_head, primary_depth, &mut collected);
            }
            cursor = start + 9 + end_offset + 3;
            continue;
        }
        let Some(end_offset) = document[start..].find('>') else {
            break;
        };
        let end = start + end_offset + 1;
        let raw_tag = &document[start + 1..end - 1];
        if let Some(tag) = parse_tag(raw_tag) {
            if let Some(scope) = hidden.as_mut() {
                if scope.name.eq_ignore_ascii_case(tag.name) {
                    if tag.closing {
                        scope.depth = scope.depth.saturating_sub(1);
                        if scope.depth == 0 {
                            hidden = None;
                        }
                    } else if !tag.self_closing {
                        scope.depth += 1;
                    }
                }
            } else if !tag.closing && tag_starts_hidden_scope(tag.name, raw_tag, primary_depth) {
                if !tag.self_closing {
                    hidden = Some(HiddenScope {
                        name: tag.name.to_string(),
                        depth: 1,
                    });
                }
            } else {
                if tag.name.eq_ignore_ascii_case("title") {
                    if tag.closing {
                        in_title = false;
                    } else {
                        if !collected.title.is_empty() {
                            collected.title.push('\n');
                        }
                        in_title = true;
                    }
                }
                if matches!(mode, MarkupMode::Html) && tag.name.eq_ignore_ascii_case("head") {
                    in_head = !tag.closing;
                }
                if is_primary_tag(tag.name, mode) {
                    if tag.closing {
                        primary_depth = primary_depth.saturating_sub(1);
                    } else if !tag.self_closing {
                        primary_depth += 1;
                    }
                }
                if is_structural_tag(tag.name) {
                    append_line_break(primary_depth, &mut collected);
                }
            }
        }
        cursor = end;
    }
    if hidden.is_none() {
        append_markup_text(
            &document[cursor..],
            in_title,
            in_head,
            primary_depth,
            &mut collected,
        );
    }
    collected
}

fn append_markup_text(
    value: &str,
    in_title: bool,
    in_head: bool,
    primary_depth: usize,
    collected: &mut MarkupText,
) {
    if in_title {
        collected.title.push_str(value);
    } else if !in_head {
        if primary_depth > 0 {
            collected.primary.push_str(value);
        } else {
            collected.fallback.push_str(value);
        }
    }
}

fn append_line_break(primary_depth: usize, collected: &mut MarkupText) {
    if primary_depth > 0 {
        collected.primary.push('\n');
    } else {
        collected.fallback.push('\n');
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

fn tag_starts_hidden_scope(name: &str, raw_tag: &str, primary_depth: usize) -> bool {
    let hidden_element = [
        "script", "style", "noscript", "svg", "template", "canvas", "iframe", "nav", "aside",
        "footer", "form", "button", "select",
    ]
    .iter()
    .any(|candidate| name.eq_ignore_ascii_case(candidate))
        || (primary_depth == 0 && name.eq_ignore_ascii_case("header"));
    hidden_element || has_hidden_attribute(raw_tag)
}

fn has_hidden_attribute(raw_tag: &str) -> bool {
    let lower = raw_tag.to_ascii_lowercase();
    let normalized = lower.replace(['\'', '"'], "");
    normalized.split_whitespace().any(|part| part == "hidden")
        || normalized.contains("aria-hidden=true")
        || normalized.contains("display:none")
        || normalized.contains("display: none")
        || normalized.contains("visibility:hidden")
        || normalized.contains("visibility: hidden")
}

fn is_primary_tag(name: &str, mode: MarkupMode) -> bool {
    match mode {
        MarkupMode::Html => ["main", "article"]
            .iter()
            .any(|candidate| name.eq_ignore_ascii_case(candidate)),
        MarkupMode::Feed => ["item", "entry"]
            .iter()
            .any(|candidate| name.eq_ignore_ascii_case(candidate)),
    }
}

fn is_structural_tag(name: &str) -> bool {
    [
        "br",
        "p",
        "div",
        "main",
        "article",
        "section",
        "header",
        "footer",
        "nav",
        "aside",
        "li",
        "tr",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "title",
        "description",
        "summary",
        "content",
        "item",
        "entry",
    ]
    .iter()
    .any(|candidate| name.eq_ignore_ascii_case(candidate))
}

fn collapse_unique_text(value: &str) -> String {
    let collapsed = collapse_text(value);
    let mut seen = BTreeSet::new();
    collapsed
        .lines()
        .filter(|line| seen.insert((*line).to_string()))
        .collect::<Vec<_>>()
        .join("\n")
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
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
}
