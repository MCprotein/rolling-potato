use crate::app::AppError;

const MAX_JSON_NESTING_DEPTH: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Object(Vec<(String, Value)>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalObject {
    pub entries: Vec<(String, CanonicalValue)>,
}

impl CanonicalObject {
    pub fn get(&self, key: &str) -> Option<&CanonicalValue> {
        self.entries
            .iter()
            .find_map(|(stored, value)| (stored == key).then_some(value))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalValue {
    Object(CanonicalObject),
    Array(Vec<CanonicalValue>),
    String(String),
    Unsigned { raw: String },
    Bool(bool),
    Null,
}

impl Object {
    fn new() -> Self {
        Self(Vec::new())
    }

    fn insert(&mut self, key: String, value: Value) -> Option<Value> {
        if let Some((_, stored)) = self.0.iter_mut().find(|(stored, _)| stored == &key) {
            return Some(std::mem::replace(stored, value));
        }
        self.0.push((key, value));
        None
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0
            .iter()
            .find_map(|(stored, value)| (stored == key).then_some(value))
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.0.iter().map(|(key, _)| key)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.0.iter().any(|(stored, _)| stored == key)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Object(Object),
    Array(Vec<Value>),
    String(String),
    Number(u128),
    Decimal(String),
    Bool(bool),
    Null,
}

pub fn parse_value(input: &str, context: &str) -> Result<Value, AppError> {
    let mut parser = Parser {
        bytes: input.as_bytes(),
        pos: 0,
        depth: 0,
    };
    let value = parser.value()?;
    parser.ws();
    if parser.pos != parser.bytes.len() {
        return Err(blocked(context, "trailing garbage"));
    }
    Ok(value)
}

pub fn parse_object(input: &str, allowed: &[&str], context: &str) -> Result<Object, AppError> {
    let value = parse_value(input, context)?;
    let Value::Object(object) = value else {
        return Err(blocked(context, "root must be an object"));
    };
    if let Some(key) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(blocked(context, &format!("unknown key: {key}")));
    }
    Ok(object)
}

pub fn parse_object_exact_order(
    input: &str,
    keys: &[&str],
    context: &str,
) -> Result<Object, AppError> {
    let object = parse_object(input, keys, context)?;
    let actual = object.keys().map(String::as_str).collect::<Vec<_>>();
    if actual != keys {
        return Err(blocked(context, "object key order mismatch"));
    }
    Ok(object)
}

pub fn parse_canonical_object(
    input: &str,
    expected_keys: &[&str],
    context: &str,
) -> Result<CanonicalObject, AppError> {
    let value = parse_value(input, context)?;
    let Value::Object(object) = value else {
        return Err(blocked(context, "root must be an object"));
    };
    let actual = object.keys().map(String::as_str).collect::<Vec<_>>();
    if actual != expected_keys {
        return Err(blocked(context, "object key order mismatch"));
    }
    let canonical = canonical_object_from_legacy(object, context)?;
    if render_canonical_object(&canonical) != input {
        return Err(blocked(context, "input is not canonical JSON"));
    }
    Ok(canonical)
}

pub fn canonical_u128(
    object: &CanonicalObject,
    key: &str,
    context: &str,
) -> Result<u128, AppError> {
    let Some(CanonicalValue::Unsigned { raw }) = object.get(key) else {
        return Err(blocked(context, &format!("missing/wrong type: {key}")));
    };
    parse_canonical_u128(raw, context, key)
}

pub fn canonical_u64(object: &CanonicalObject, key: &str, context: &str) -> Result<u64, AppError> {
    u64::try_from(canonical_u128(object, key, context)?)
        .map_err(|_| blocked(context, &format!("out of range: {key}")))
}

pub fn render_canonical_object(object: &CanonicalObject) -> String {
    let mut out = String::new();
    render_canonical_value(&CanonicalValue::Object(object.clone()), &mut out);
    out
}

fn canonical_object_from_legacy(
    object: Object,
    context: &str,
) -> Result<CanonicalObject, AppError> {
    object
        .0
        .into_iter()
        .map(|(key, value)| Ok((key, canonical_value_from_legacy(value, context)?)))
        .collect::<Result<Vec<_>, AppError>>()
        .map(|entries| CanonicalObject { entries })
}

fn canonical_value_from_legacy(value: Value, context: &str) -> Result<CanonicalValue, AppError> {
    match value {
        Value::Object(object) => {
            canonical_object_from_legacy(object, context).map(CanonicalValue::Object)
        }
        Value::Array(values) => values
            .into_iter()
            .map(|value| canonical_value_from_legacy(value, context))
            .collect::<Result<Vec<_>, _>>()
            .map(CanonicalValue::Array),
        Value::String(value) => Ok(CanonicalValue::String(value)),
        Value::Number(value) => Ok(CanonicalValue::Unsigned {
            raw: value.to_string(),
        }),
        Value::Decimal(_) => Err(blocked(
            context,
            "canonical number must be unsigned integer",
        )),
        Value::Bool(value) => Ok(CanonicalValue::Bool(value)),
        Value::Null => Ok(CanonicalValue::Null),
    }
}

fn parse_canonical_u128(raw: &str, context: &str, key: &str) -> Result<u128, AppError> {
    if raw.is_empty() || (raw.len() > 1 && raw.starts_with('0')) {
        return Err(blocked(
            context,
            &format!("invalid unsigned integer: {key}"),
        ));
    }
    raw.bytes().try_fold(0_u128, |value, byte| {
        let digit = byte
            .checked_sub(b'0')
            .filter(|digit| *digit <= 9)
            .ok_or_else(|| blocked(context, &format!("invalid unsigned integer: {key}")))?;
        value
            .checked_mul(10)
            .and_then(|value| value.checked_add(u128::from(digit)))
            .ok_or_else(|| blocked(context, &format!("out of range: {key}")))
    })
}

fn render_canonical_value(value: &CanonicalValue, out: &mut String) {
    match value {
        CanonicalValue::Object(object) => {
            out.push('{');
            for (index, (key, value)) in object.entries.iter().enumerate() {
                if index != 0 {
                    out.push(',');
                }
                render_string(key, out);
                out.push(':');
                render_canonical_value(value, out);
            }
            out.push('}');
        }
        CanonicalValue::Array(values) => {
            out.push('[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    out.push(',');
                }
                render_canonical_value(value, out);
            }
            out.push(']');
        }
        CanonicalValue::String(value) => render_string(value, out),
        CanonicalValue::Unsigned { raw } => out.push_str(raw),
        CanonicalValue::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
        CanonicalValue::Null => out.push_str("null"),
    }
}

pub fn string(object: &Object, key: &str, context: &str) -> Result<String, AppError> {
    match object.get(key) {
        Some(Value::String(value)) => Ok(value.clone()),
        Some(_) => Err(blocked(context, &format!("wrong type: {key}"))),
        None => Err(blocked(context, &format!("missing key: {key}"))),
    }
}

pub fn number(object: &Object, key: &str, context: &str) -> Result<u64, AppError> {
    match object.get(key) {
        Some(Value::Number(value)) => {
            u64::try_from(*value).map_err(|_| blocked(context, &format!("out of range: {key}")))
        }
        Some(_) => Err(blocked(context, &format!("wrong type: {key}"))),
        None => Err(blocked(context, &format!("missing key: {key}"))),
    }
}

pub fn number_u128(object: &Object, key: &str, context: &str) -> Result<u128, AppError> {
    match object.get(key) {
        Some(Value::Number(value)) => Ok(*value),
        Some(_) => Err(blocked(context, &format!("wrong type: {key}"))),
        None => Err(blocked(context, &format!("missing key: {key}"))),
    }
}

pub fn boolean(object: &Object, key: &str, context: &str) -> Result<bool, AppError> {
    match object.get(key) {
        Some(Value::Bool(value)) => Ok(*value),
        Some(_) => Err(blocked(context, &format!("wrong type: {key}"))),
        None => Err(blocked(context, &format!("missing key: {key}"))),
    }
}

pub fn render_compact(value: &Value) -> String {
    let mut out = String::new();
    render_value(value, &mut out);
    out
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

fn render_string(value: &str, out: &mut String) {
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

fn blocked(context: &str, reason: &str) -> AppError {
    AppError::blocked(format!(
        "strict JSON 검증 차단\n- artifact: {context}\n- 이유: {reason}"
    ))
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    depth: usize,
}

impl Parser<'_> {
    fn value(&mut self) -> Result<Value, AppError> {
        self.ws();
        match self.peek() {
            Some(b'{') => self.container(Self::object),
            Some(b'[') => self.container(Self::array),
            Some(b'"') => self.string_value().map(Value::String),
            Some(b't') => {
                self.literal(b"true")?;
                Ok(Value::Bool(true))
            }
            Some(b'f') => {
                self.literal(b"false")?;
                Ok(Value::Bool(false))
            }
            Some(b'n') => {
                self.literal(b"null")?;
                Ok(Value::Null)
            }
            Some(b'-' | b'0'..=b'9') => self.number_value(),
            _ => Err(blocked("JSON", "invalid value")),
        }
    }

    fn container(
        &mut self,
        parse: fn(&mut Self) -> Result<Value, AppError>,
    ) -> Result<Value, AppError> {
        if self.depth >= MAX_JSON_NESTING_DEPTH {
            return Err(blocked("JSON", "nesting depth budget exceeded"));
        }
        self.depth += 1;
        let result = parse(self);
        self.depth -= 1;
        result
    }

    fn object(&mut self) -> Result<Value, AppError> {
        self.take(b'{')?;
        let mut map = Object::new();
        self.ws();
        if self.consume(b'}') {
            return Ok(Value::Object(map));
        }
        loop {
            self.ws();
            let key = self.string_value()?;
            self.ws();
            self.take(b':')?;
            let value = self.value()?;
            if map.insert(key.clone(), value).is_some() {
                return Err(blocked("JSON", &format!("duplicate key: {key}")));
            }
            self.ws();
            if self.consume(b'}') {
                break;
            }
            self.take(b',')?;
        }
        Ok(Value::Object(map))
    }

    fn array(&mut self) -> Result<Value, AppError> {
        self.take(b'[')?;
        let mut values = Vec::new();
        self.ws();
        if self.consume(b']') {
            return Ok(Value::Array(values));
        }
        loop {
            values.push(self.value()?);
            self.ws();
            if self.consume(b']') {
                break;
            }
            self.take(b',')?;
        }
        Ok(Value::Array(values))
    }

    fn string_value(&mut self) -> Result<String, AppError> {
        self.take(b'"')?;
        let mut out = String::new();
        loop {
            let byte = self
                .next()
                .ok_or_else(|| blocked("JSON", "unterminated string"))?;
            match byte {
                b'"' => return Ok(out),
                b'\\' => match self
                    .next()
                    .ok_or_else(|| blocked("JSON", "invalid escape"))?
                {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'b' => out.push('\u{0008}'),
                    b'f' => out.push('\u{000c}'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'u' => {
                        let mut code = self.hex4()?;
                        if (0xD800..=0xDBFF).contains(&code) {
                            self.take(b'\\')?;
                            self.take(b'u')?;
                            let low = self.hex4()?;
                            if !(0xDC00..=0xDFFF).contains(&low) {
                                return Err(blocked("JSON", "invalid surrogate pair"));
                            }
                            code = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00);
                        } else if (0xDC00..=0xDFFF).contains(&code) {
                            return Err(blocked("JSON", "unpaired low surrogate"));
                        }
                        let ch = char::from_u32(code)
                            .ok_or_else(|| blocked("JSON", "invalid unicode escape"))?;
                        out.push(ch);
                    }
                    _ => return Err(blocked("JSON", "invalid escape")),
                },
                0..=31 => return Err(blocked("JSON", "control character in string")),
                first @ 0x80..=0xff => {
                    self.pos -= 1;
                    let width = match first {
                        0xC2..=0xDF => 2,
                        0xE0..=0xEF => 3,
                        0xF0..=0xF4 => 4,
                        _ => return Err(blocked("JSON", "invalid UTF-8")),
                    };
                    let end = self
                        .pos
                        .checked_add(width)
                        .ok_or_else(|| blocked("JSON", "invalid UTF-8"))?;
                    let encoded = self
                        .bytes
                        .get(self.pos..end)
                        .ok_or_else(|| blocked("JSON", "invalid UTF-8"))?;
                    let ch = std::str::from_utf8(encoded)
                        .map_err(|_| blocked("JSON", "invalid UTF-8"))?;
                    let ch = ch
                        .chars()
                        .next()
                        .ok_or_else(|| blocked("JSON", "invalid UTF-8"))?;
                    out.push(ch);
                    self.pos = end;
                }
                other => out.push(other as char),
            }
        }
    }

    fn number_value(&mut self) -> Result<Value, AppError> {
        let start = self.pos;
        self.consume(b'-');
        match self.peek() {
            Some(b'0') => {
                self.pos += 1;
                if self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                    return Err(blocked("JSON", "leading-zero number"));
                }
            }
            Some(b'1'..=b'9') => {
                while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                    self.pos += 1;
                }
            }
            _ => return Err(blocked("JSON", "invalid number")),
        }
        if self.consume(b'.') {
            let fraction_start = self.pos;
            while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                self.pos += 1;
            }
            if self.pos == fraction_start {
                return Err(blocked("JSON", "invalid fraction"));
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            let exponent_start = self.pos;
            while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                self.pos += 1;
            }
            if self.pos == exponent_start {
                return Err(blocked("JSON", "invalid exponent"));
            }
        }
        let raw = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| blocked("JSON", "invalid number"))?;
        if !raw.starts_with('-') && !raw.contains(['.', 'e', 'E']) {
            if let Ok(value) = raw.parse::<u128>() {
                return Ok(Value::Number(value));
            }
        }
        Ok(Value::Decimal(raw.to_string()))
    }

    fn hex4(&mut self) -> Result<u32, AppError> {
        let mut value = 0;
        for _ in 0..4 {
            let byte = self
                .next()
                .ok_or_else(|| blocked("JSON", "short unicode escape"))?;
            value = value * 16
                + (byte as char)
                    .to_digit(16)
                    .ok_or_else(|| blocked("JSON", "invalid unicode escape"))?;
        }
        Ok(value)
    }
    fn literal(&mut self, expected: &[u8]) -> Result<(), AppError> {
        if self.bytes.get(self.pos..self.pos + expected.len()) == Some(expected) {
            self.pos += expected.len();
            Ok(())
        } else {
            Err(blocked("JSON", "invalid literal"))
        }
    }
    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.pos += 1;
        }
    }
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }
    fn next(&mut self) -> Option<u8> {
        let value = self.peek()?;
        self.pos += 1;
        Some(value)
    }
    fn consume(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn take(&mut self, byte: u8) -> Result<(), AppError> {
        if self.consume(byte) {
            Ok(())
        } else {
            Err(blocked("JSON", "unexpected token"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_duplicate_unknown_escape_type_and_trailing() {
        for input in [
            r#"{"a":"x","a":"y"}"#,
            r#"{"b":"x"}"#,
            r#"{"a":"\q"}"#,
            r#"{"a":1}"#,
            r#"{"a":"x"} garbage"#,
        ] {
            let parsed = parse_object(input, &["a"], "fixture");
            if let Ok(object) = parsed {
                assert!(string(&object, "a", "fixture").is_err());
            }
        }
    }

    #[test]
    fn rejects_leading_zero_number() {
        assert!(parse_object("{\"schema\":01}", &["schema"], "fixture").is_err());
    }

    #[test]
    fn generic_parser_accepts_standard_numbers_and_surrogate_pairs() {
        let parsed = parse_value(
            r#"{"negative":-1,"fraction":1.25,"exponent":2e3,"emoji":"\uD83D\uDE00"}"#,
            "fixture",
        )
        .unwrap();
        let Value::Object(object) = parsed else {
            panic!("object가 필요합니다.");
        };

        assert_eq!(object.get("negative"), Some(&Value::Decimal("-1".into())));
        assert_eq!(object.get("fraction"), Some(&Value::Decimal("1.25".into())));
        assert_eq!(object.get("exponent"), Some(&Value::Decimal("2e3".into())));
        assert_eq!(object.get("emoji"), Some(&Value::String("😀".into())));
    }

    #[test]
    fn ordered_object_round_trips_compact_bytes_and_checks_exact_order() {
        let input = r#"{"z":340282366920938463463374607431768211455,"a":[true,"한글\n",null]}"#;
        let value = parse_value(input, "ordered fixture").unwrap();

        assert_eq!(render_compact(&value), input);
        assert!(parse_object_exact_order(input, &["z", "a"], "ordered fixture").is_ok());
        assert!(parse_object_exact_order(input, &["a", "z"], "ordered fixture").is_err());
    }

    #[test]
    fn checked_u128_and_u64_reject_narrowing_overflow() {
        let input = r#"{"small":18446744073709551615,"large":18446744073709551616}"#;
        let object =
            parse_object_exact_order(input, &["small", "large"], "number fixture").unwrap();

        assert_eq!(
            number(&object, "small", "number fixture").unwrap(),
            u64::MAX
        );
        assert_eq!(
            number_u128(&object, "large", "number fixture").unwrap(),
            u64::MAX as u128 + 1
        );
        assert!(number(&object, "large", "number fixture").is_err());
    }

    #[test]
    fn canonical_object_rejects_noncanonical_bytes_and_numeric_spellings() {
        for input in [
            "{\"n\": 1}",
            " {\"n\":1}",
            "{\"n\":1}\n",
            "{\"n\":-1}",
            "{\"n\":1.0}",
            "{\"n\":1e0}",
            "{\"n\":\"\\u0061\"}",
            "{\"n\":1,\"extra\":2}",
        ] {
            assert!(parse_canonical_object(input, &["n"], "canonical fixture").is_err());
        }
    }

    #[test]
    fn canonical_unsigned_boundaries_round_trip_byte_exactly() {
        let input = r#"{"zero":0,"u64":18446744073709551615,"u128":340282366920938463463374607431768211455,"nested":{"value":7},"array":[0,true,null,"한글"]}"#;
        let object = parse_canonical_object(
            input,
            &["zero", "u64", "u128", "nested", "array"],
            "canonical fixture",
        )
        .unwrap();

        assert_eq!(
            canonical_u64(&object, "zero", "canonical fixture").unwrap(),
            0
        );
        assert_eq!(
            canonical_u64(&object, "u64", "canonical fixture").unwrap(),
            u64::MAX
        );
        assert_eq!(
            canonical_u128(&object, "u128", "canonical fixture").unwrap(),
            u128::MAX
        );
        assert_eq!(render_canonical_object(&object), input);
        assert!(parse_canonical_object(
            r#"{"n":340282366920938463463374607431768211456}"#,
            &["n"],
            "overflow fixture"
        )
        .is_err());
    }

    #[test]
    fn nesting_depth_is_bounded_before_recursive_descent() {
        let at_limit = format!(
            "{}0{}",
            "[".repeat(MAX_JSON_NESTING_DEPTH),
            "]".repeat(MAX_JSON_NESTING_DEPTH)
        );
        let beyond_limit = format!(
            "{}0{}",
            "[".repeat(MAX_JSON_NESTING_DEPTH + 1),
            "]".repeat(MAX_JSON_NESTING_DEPTH + 1)
        );

        assert!(parse_value(&at_limit, "depth fixture").is_ok());
        let error = parse_value(&beyond_limit, "depth fixture").unwrap_err();
        assert!(error.message.contains("nesting depth budget exceeded"));
    }
}
