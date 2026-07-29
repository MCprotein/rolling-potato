use crate::foundation::error::AppError;

use super::error::blocked;
use super::parser;
use super::types::{Object, Value};

pub fn parse_object(input: &str, allowed: &[&str], context: &str) -> Result<Object, AppError> {
    let value = parser::parse_value(input, context)?;
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
