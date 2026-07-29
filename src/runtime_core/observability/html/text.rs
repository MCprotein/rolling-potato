use crate::runtime_core::policy::redaction;

pub(super) fn optional_f64(value: Option<f64>, suffix: &str) -> String {
    value
        .filter(|value| value.is_finite())
        .map(|value| format!("{value:.1}{suffix}"))
        .unwrap_or_else(|| "미기록".to_owned())
}

pub(super) fn optional_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "미기록".to_owned())
}

pub(super) fn pressure_class(value: &str) -> &'static str {
    match value {
        "normal" => "healthy",
        "degraded" => "warning",
        "critical" => "failed",
        _ => "muted",
    }
}

pub(super) fn policy_class(value: &str) -> &'static str {
    match value {
        "recommend" => "healthy",
        "constrained" | "insufficient-evidence" => "warning",
        "blocked" => "failed",
        _ => "muted",
    }
}

pub(super) fn safe_html_text(value: &str) -> String {
    let redacted = redaction::redact_text(value);
    let path_redacted = redacted
        .split_whitespace()
        .map(|part| {
            if contains_absolute_path(part) {
                "[REDACTED_PATH]"
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    escape_html(&path_redacted)
}

fn contains_absolute_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    for index in 0..bytes.len() {
        let boundary = index == 0
            || matches!(
                bytes[index - 1],
                b'=' | b':' | b'(' | b'[' | b'{' | b'"' | b'\'' | b',' | b';'
            );
        if bytes[index] == b'/' && boundary {
            return true;
        }
        if bytes[index] == b'~'
            && boundary
            && bytes.get(index + 1).is_some_and(|next| *next == b'/')
        {
            return true;
        }
        if bytes[index].is_ascii_alphabetic()
            && bytes.get(index + 1).is_some_and(|next| *next == b':')
            && bytes
                .get(index + 2)
                .is_some_and(|next| matches!(*next, b'/' | b'\\'))
        {
            return true;
        }
        if bytes[index] == b'\\'
            && boundary
            && bytes.get(index + 1).is_some_and(|next| *next == b'\\')
        {
            return true;
        }
    }
    false
}

fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}
