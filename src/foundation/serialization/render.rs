use super::types::Value;

pub fn render_compact(value: &Value) -> String {
    let mut out = String::new();
    render_value(value, &mut out);
    out
}

pub(crate) fn escape_string_content(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch if ch.is_control() => {
                use std::fmt::Write as _;
                write!(escaped, "\\u{:04x}", ch as u32).expect("String 쓰기는 실패하지 않습니다.");
            }
            ch => escaped.push(ch),
        }
    }
    escaped
}

fn render_value(value: &Value, out: &mut String) {
    match value {
        Value::Object(object) => {
            out.push('{');
            for (index, (key, value)) in object.0.iter().enumerate() {
                if index != 0 {
                    out.push(',');
                }
                render_string(key, out);
                out.push(':');
                render_value(value, out);
            }
            out.push('}');
        }
        Value::Array(values) => {
            out.push('[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    out.push(',');
                }
                render_value(value, out);
            }
            out.push(']');
        }
        Value::String(value) => render_string(value, out),
        Value::Number(value) => out.push_str(&value.to_string()),
        Value::Decimal(value) => out.push_str(value),
        Value::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
        Value::Null => out.push_str("null"),
    }
}

pub(super) fn render_string(value: &str, out: &mut String) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch <= '\u{001f}' => {
                use std::fmt::Write as _;
                write!(out, "\\u{:04x}", ch as u32).expect("String 쓰기는 실패하지 않습니다.");
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
}
