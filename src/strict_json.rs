use std::collections::BTreeMap;

use crate::app::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Object(BTreeMap<String, Value>),
    Array(Vec<Value>),
    String(String),
    Number(u64),
    Decimal(String),
    Bool(bool),
    Null,
}

pub fn parse_value(input: &str, context: &str) -> Result<Value, AppError> {
    let mut parser = Parser {
        bytes: input.as_bytes(),
        pos: 0,
    };
    let value = parser.value()?;
    parser.ws();
    if parser.pos != parser.bytes.len() {
        return Err(blocked(context, "trailing garbage"));
    }
    Ok(value)
}

pub fn parse_object(
    input: &str,
    allowed: &[&str],
    context: &str,
) -> Result<BTreeMap<String, Value>, AppError> {
    let value = parse_value(input, context)?;
    let Value::Object(object) = value else {
        return Err(blocked(context, "root must be an object"));
    };
    if let Some(key) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(blocked(context, &format!("unknown key: {key}")));
    }
    Ok(object)
}

pub fn string(
    object: &BTreeMap<String, Value>,
    key: &str,
    context: &str,
) -> Result<String, AppError> {
    match object.get(key) {
        Some(Value::String(value)) => Ok(value.clone()),
        Some(_) => Err(blocked(context, &format!("wrong type: {key}"))),
        None => Err(blocked(context, &format!("missing key: {key}"))),
    }
}

pub fn number(object: &BTreeMap<String, Value>, key: &str, context: &str) -> Result<u64, AppError> {
    match object.get(key) {
        Some(Value::Number(value)) => Ok(*value),
        Some(_) => Err(blocked(context, &format!("wrong type: {key}"))),
        None => Err(blocked(context, &format!("missing key: {key}"))),
    }
}

pub fn boolean(
    object: &BTreeMap<String, Value>,
    key: &str,
    context: &str,
) -> Result<bool, AppError> {
    match object.get(key) {
        Some(Value::Bool(value)) => Ok(*value),
        Some(_) => Err(blocked(context, &format!("wrong type: {key}"))),
        None => Err(blocked(context, &format!("missing key: {key}"))),
    }
}

fn blocked(context: &str, reason: &str) -> AppError {
    AppError::blocked(format!(
        "strict JSON 검증 차단\n- artifact: {context}\n- 이유: {reason}"
    ))
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn value(&mut self) -> Result<Value, AppError> {
        self.ws();
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
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

    fn object(&mut self) -> Result<Value, AppError> {
        self.take(b'{')?;
        let mut map = BTreeMap::new();
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
            if let Ok(value) = raw.parse::<u64>() {
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
}
