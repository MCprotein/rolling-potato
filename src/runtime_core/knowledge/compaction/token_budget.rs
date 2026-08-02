//! Conservative token estimation and UTF-8-safe bounded truncation.

pub(crate) fn estimate_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    let chars = text.chars().count().div_ceil(3);
    let bytes = text.len().div_ceil(4);
    chars.max(bytes).max(1)
}

pub(crate) fn truncate_head_and_tail_to_tokens(text: &str, max_tokens: usize) -> String {
    if max_tokens == 0 {
        return String::new();
    }
    if estimate_tokens(text) <= max_tokens {
        return text.to_string();
    }
    const MARKER: &str = "\n[compacted]\n";
    if estimate_tokens(MARKER) >= max_tokens {
        return bounded_chars_and_bytes(MARKER, max_tokens, TokenTruncation::Head);
    }
    let count = text.chars().count();
    let marker_chars = MARKER.chars().count();
    let marker_bytes = MARKER.len();
    let available_chars = max_tokens.saturating_mul(3).saturating_sub(marker_chars);
    let available_bytes = max_tokens.saturating_mul(4).saturating_sub(marker_bytes);
    let head_chars = available_chars.div_ceil(2);
    let tail_chars = available_chars - head_chars;
    let head_bytes = available_bytes.div_ceil(2);
    let tail_bytes = available_bytes - head_bytes;
    let head = bounded_chars_and_bytes_raw(text, head_chars, head_bytes, TokenTruncation::Head);
    let tail = bounded_chars_and_bytes_raw(text, tail_chars, tail_bytes, TokenTruncation::Tail);
    debug_assert!(head.chars().count() + tail.chars().count() < count);
    format!("{head}{MARKER}{tail}")
}

pub(crate) fn truncate_head_to_tokens(text: &str, max_tokens: usize) -> String {
    truncate_to_token_budget(text, max_tokens, TokenTruncation::Head)
}

pub(crate) fn truncate_tail_to_estimated_tokens(text: &str, max_tokens: usize) -> String {
    truncate_to_token_budget(text, max_tokens, TokenTruncation::Tail)
}

#[derive(Debug, Clone, Copy)]
enum TokenTruncation {
    Head,
    Tail,
}

fn truncate_to_token_budget(text: &str, max_tokens: usize, mode: TokenTruncation) -> String {
    if max_tokens == 0 {
        return String::new();
    }
    if estimate_tokens(text) <= max_tokens {
        return text.to_string();
    }
    const MARKER: &str = "\n[compacted]\n";
    if estimate_tokens(MARKER) >= max_tokens {
        return bounded_chars_and_bytes(MARKER, max_tokens, TokenTruncation::Head);
    }
    let marker_chars = MARKER.chars().count();
    let marker_bytes = MARKER.len();
    let max_chars = max_tokens.saturating_mul(3).saturating_sub(marker_chars);
    let max_bytes = max_tokens.saturating_mul(4).saturating_sub(marker_bytes);
    let bounded = bounded_chars_and_bytes_raw(text, max_chars, max_bytes, mode);
    match mode {
        TokenTruncation::Head => format!("{bounded}{MARKER}"),
        TokenTruncation::Tail => format!("{MARKER}{bounded}"),
    }
}

fn bounded_chars_and_bytes(text: &str, max_tokens: usize, mode: TokenTruncation) -> String {
    bounded_chars_and_bytes_raw(
        text,
        max_tokens.saturating_mul(3),
        max_tokens.saturating_mul(4),
        mode,
    )
}

fn bounded_chars_and_bytes_raw(
    text: &str,
    max_chars: usize,
    max_bytes: usize,
    mode: TokenTruncation,
) -> String {
    match mode {
        TokenTruncation::Head => {
            let end = text
                .char_indices()
                .take(max_chars)
                .take_while(|(index, ch)| index.saturating_add(ch.len_utf8()) <= max_bytes)
                .map(|(index, ch)| index + ch.len_utf8())
                .last()
                .unwrap_or(0);
            text[..end].to_string()
        }
        TokenTruncation::Tail => {
            let mut bytes = 0usize;
            let mut start = text.len();
            for (chars, (index, ch)) in text.char_indices().rev().enumerate() {
                if chars == max_chars || bytes.saturating_add(ch.len_utf8()) > max_bytes {
                    break;
                }
                bytes += ch.len_utf8();
                start = index;
            }
            text[start..].to_string()
        }
    }
}
