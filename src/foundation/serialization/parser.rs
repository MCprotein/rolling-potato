use crate::foundation::error::AppError;

use super::{blocked, Object, Value, MAX_JSON_NESTING_DEPTH};

pub(super) fn parse_value(input: &str, context: &str) -> Result<Value, AppError> {
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
