use std::collections::BTreeMap;

use crate::app::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Object(BTreeMap<String, Value>),
    Array(Vec<Value>),
    String(String),
    Number(u64),
    Bool(bool),
    Null,
}

pub fn parse_object(
    input: &str,
    allowed: &[&str],
    context: &str,
) -> Result<BTreeMap<String, Value>, AppError> {
    let mut parser = Parser {
        bytes: input.as_bytes(),
        pos: 0,
    };
    let value = parser.value()?;
    parser.ws();
    if parser.pos != parser.bytes.len() {
        return Err(blocked(context, "trailing garbage"));
    }
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
            Some(b'0'..=b'9') => self.number_value().map(Value::Number),
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
                        let code = self.hex4()?;
                        let ch = char::from_u32(code)
                            .ok_or_else(|| blocked("JSON", "invalid unicode escape"))?;
                        if (0xD800..=0xDFFF).contains(&code) {
                            return Err(blocked("JSON", "surrogate escape unsupported"));
                        }
                        out.push(ch);
                    }
                    _ => return Err(blocked("JSON", "invalid escape")),
                },
                0..=31 => return Err(blocked("JSON", "control character in string")),
                0x80..=0xff => {
                    self.pos -= 1;
                    let rest = std::str::from_utf8(&self.bytes[self.pos..])
                        .map_err(|_| blocked("JSON", "invalid UTF-8"))?;
                    let ch = rest
                        .chars()
                        .next()
                        .ok_or_else(|| blocked("JSON", "invalid UTF-8"))?;
                    out.push(ch);
                    self.pos += ch.len_utf8();
                }
                other => out.push(other as char),
            }
        }
    }

    fn number_value(&mut self) -> Result<u64, AppError> {
        if self.peek() == Some(b'0') && self.bytes.get(self.pos + 1).is_some_and(u8::is_ascii_digit)
        {
            return Err(blocked("JSON", "leading-zero number"));
        }
        let start = self.pos;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        if self.peek() == Some(b'.') || matches!(self.peek(), Some(b'e' | b'E' | b'-' | b'+')) {
            return Err(blocked("JSON", "only unsigned integer numbers are allowed"));
        }
        std::str::from_utf8(&self.bytes[start..self.pos])
            .ok()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| blocked("JSON", "invalid number"))
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
}
