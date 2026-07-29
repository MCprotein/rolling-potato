use std::fmt::Write as _;

use sha2::{Digest, Sha256};

pub(crate) fn encode_hex_text(value: &str) -> String {
    let mut output = String::with_capacity(value.len() * 2);
    for byte in value.as_bytes() {
        write!(output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

pub(crate) fn decode_hex_text(value: &str) -> Result<String, String> {
    if !value.len().is_multiple_of(2) {
        return Err("hex length must be even".to_string());
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    let chars = value.as_bytes();
    let mut index = 0usize;
    while index < chars.len() {
        let high = hex_value(chars[index]).ok_or_else(|| "invalid high nibble".to_string())?;
        let low = hex_value(chars[index + 1]).ok_or_else(|| "invalid low nibble".to_string())?;
        bytes.push((high << 4) | low);
        index += 2;
    }
    String::from_utf8(bytes).map_err(|err| err.to_string())
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub(crate) fn sha256_text(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let bytes = hasher.finalize();
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

pub(super) fn sha256_bytes(value: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value);
    let mut output = String::with_capacity(64);
    for byte in hasher.finalize() {
        write!(output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}
