use crate::foundation::error::AppError;

use super::error::blocked;
use super::parser;
use super::render::render_string;
use super::types::{CanonicalObject, CanonicalValue, Object, Value};

pub fn parse_canonical_object(
    input: &str,
    expected_keys: &[&str],
    context: &str,
) -> Result<CanonicalObject, AppError> {
    let value = parser::parse_value(input, context)?;
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
