use std::collections::BTreeMap;

use crate::foundation::error::AppError;

use super::types::FixtureJsonValue;

pub(super) fn parse_fixture_json_object(
    text: &str,
) -> Result<BTreeMap<String, FixtureJsonValue>, AppError> {
    let mut rest = skip_ws(text);
    rest = rest.strip_prefix('{').ok_or_else(fixture_json_error)?;
    let mut fields = BTreeMap::new();
    rest = skip_ws(rest);
    if let Some(after_object) = rest.strip_prefix('}') {
        if skip_ws(after_object).is_empty() {
            return Ok(fields);
        }
        return Err(fixture_json_error());
    }

    loop {
        let (key, after_key) = parse_json_string_value(rest).ok_or_else(fixture_json_error)?;
        rest = skip_ws(after_key);
        rest = rest.strip_prefix(':').ok_or_else(fixture_json_error)?;
        rest = skip_ws(rest);
        let (value, after_value) = parse_fixture_json_value(rest)?;
        if fields.insert(key.clone(), value).is_some() {
            return Err(AppError::usage(format!(
                "benchmark fixture field가 중복되었습니다: {key}"
            )));
        }

        rest = skip_ws(after_value);
        if let Some(after_comma) = rest.strip_prefix(',') {
            rest = skip_ws(after_comma);
            if rest.starts_with('}') {
                return Err(fixture_json_error());
            }
            continue;
        }
        if let Some(after_object) = rest.strip_prefix('}') {
            if skip_ws(after_object).is_empty() {
                return Ok(fields);
            }
            return Err(fixture_json_error());
        }
        return Err(fixture_json_error());
    }
}

fn parse_fixture_json_value(text: &str) -> Result<(FixtureJsonValue, &str), AppError> {
    if text.starts_with('"') {
        let (value, rest) = parse_json_string_value(text).ok_or_else(fixture_json_error)?;
        return Ok((FixtureJsonValue::String(value), rest));
    }
    if text.starts_with('[') {
        let (value, rest) = parse_json_string_array_value(text)?;
        return Ok((FixtureJsonValue::StringArray(value), rest));
    }
    if let Some(rest) = text.strip_prefix("true") {
        return Ok((FixtureJsonValue::Bool(true), rest));
    }
    if let Some(rest) = text.strip_prefix("false") {
        return Ok((FixtureJsonValue::Bool(false), rest));
    }
    if text.starts_with(|ch: char| ch.is_ascii_digit()) {
        let (value, rest) = parse_json_u32_value(text).ok_or_else(fixture_json_error)?;
        return Ok((FixtureJsonValue::U32(value), rest));
    }
    Err(fixture_json_error())
}

fn parse_json_string_array_value(text: &str) -> Result<(Vec<String>, &str), AppError> {
    let mut rest = text.strip_prefix('[').ok_or_else(fixture_json_error)?;
    let mut values = Vec::new();
    rest = skip_ws(rest);
    if let Some(after_array) = rest.strip_prefix(']') {
        return Ok((values, after_array));
    }

    loop {
        let (value, after_string) = parse_json_string_value(rest).ok_or_else(fixture_json_error)?;
        values.push(value);
        rest = skip_ws(after_string);
        if let Some(after_comma) = rest.strip_prefix(',') {
            rest = skip_ws(after_comma);
            if rest.starts_with(']') {
                return Err(fixture_json_error());
            }
            continue;
        }
        if let Some(after_array) = rest.strip_prefix(']') {
            return Ok((values, after_array));
        }
        return Err(fixture_json_error());
    }
}

fn parse_json_string_value(text: &str) -> Option<(String, &str)> {
    let mut index = 0;
    let quote = text[index..].chars().next()?;
    if quote != '"' {
        return None;
    }
    index += quote.len_utf8();
    let mut value = String::new();

    while index < text.len() {
        let ch = text[index..].chars().next()?;
        index += ch.len_utf8();
        match ch {
            '"' => return Some((value, &text[index..])),
            '\\' => {
                let escaped = text[index..].chars().next()?;
                index += escaped.len_utf8();
                match escaped {
                    '"' => value.push('"'),
                    '\\' => value.push('\\'),
                    '/' => value.push('/'),
                    'b' => value.push('\u{0008}'),
                    'f' => value.push('\u{000C}'),
                    'n' => value.push('\n'),
                    'r' => value.push('\r'),
                    't' => value.push('\t'),
                    'u' => {
                        let (decoded, next_index) = parse_json_unicode_escape(text, index)?;
                        value.push(decoded);
                        index = next_index;
                    }
                    _ => return None,
                }
            }
            ch if ch <= '\u{001F}' => return None,
            other => value.push(other),
        }
    }

    None
}

fn parse_json_unicode_escape(text: &str, index: usize) -> Option<(char, usize)> {
    let (high, mut next_index) = parse_hex_quad(text, index)?;
    if (0xD800..=0xDBFF).contains(&high) {
        let slash = text[next_index..].chars().next()?;
        next_index += slash.len_utf8();
        let u = text[next_index..].chars().next()?;
        next_index += u.len_utf8();
        if slash != '\\' || u != 'u' {
            return None;
        }
        let (low, after_low) = parse_hex_quad(text, next_index)?;
        if !(0xDC00..=0xDFFF).contains(&low) {
            return None;
        }
        let scalar = 0x10000 + (((high - 0xD800) << 10) | (low - 0xDC00));
        return char::from_u32(scalar).map(|ch| (ch, after_low));
    }
    if (0xDC00..=0xDFFF).contains(&high) {
        return None;
    }
    char::from_u32(high).map(|ch| (ch, next_index))
}

fn parse_hex_quad(text: &str, index: usize) -> Option<(u32, usize)> {
    let mut value = 0_u32;
    let mut next_index = index;
    for _ in 0..4 {
        let ch = text[next_index..].chars().next()?;
        let digit = ch.to_digit(16)?;
        value = (value << 4) | digit;
        next_index += ch.len_utf8();
    }
    Some((value, next_index))
}

fn parse_json_u32_value(text: &str) -> Option<(u32, &str)> {
    let digits = text
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if digits.len() > 1 && digits.starts_with('0') {
        return None;
    }
    let value = digits.parse().ok()?;
    Some((value, &text[digits.len()..]))
}

fn skip_ws(text: &str) -> &str {
    text.trim_start_matches(|ch: char| ch.is_ascii_whitespace())
}

fn fixture_json_error() -> AppError {
    AppError::usage("benchmark fixture JSON object가 schema parser를 통과하지 못했습니다.")
}
