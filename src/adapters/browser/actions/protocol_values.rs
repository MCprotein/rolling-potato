use crate::foundation::error::AppError;
use crate::foundation::serialization::Value;

pub(super) fn box_center(result: &Value) -> Result<(String, String), AppError> {
    let Value::Object(result) = result else {
        return Err(AppError::runtime(
            "DOM.getBoxModel result가 object가 아닙니다.",
        ));
    };
    let Value::Object(model) = result
        .get("model")
        .ok_or_else(|| AppError::runtime("DOM.getBoxModel response에 model이 없습니다."))?
    else {
        return Err(AppError::runtime(
            "DOM.getBoxModel model 형식이 올바르지 않습니다.",
        ));
    };
    let Value::Array(content) = model
        .get("content")
        .ok_or_else(|| AppError::runtime("DOM.getBoxModel response에 content quad가 없습니다."))?
    else {
        return Err(AppError::runtime(
            "DOM.getBoxModel content quad 형식이 올바르지 않습니다.",
        ));
    };
    if content.len() != 8 {
        return Err(AppError::runtime(
            "DOM.getBoxModel content quad 길이가 올바르지 않습니다.",
        ));
    }
    let numbers = content
        .iter()
        .map(json_number)
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| AppError::runtime("DOM.getBoxModel 좌표가 숫자가 아닙니다."))?;
    let x = (numbers[0] + numbers[2] + numbers[4] + numbers[6]) / 4.0;
    let y = (numbers[1] + numbers[3] + numbers[5] + numbers[7]) / 4.0;
    if !x.is_finite() || !y.is_finite() {
        return Err(AppError::runtime(
            "DOM.getBoxModel 좌표를 처리할 수 없습니다.",
        ));
    }
    Ok((format!("{x:.2}"), format!("{y:.2}")))
}

pub(super) fn object_string(object: &Value, key: &str, context: &str) -> Result<String, AppError> {
    let Value::Object(object) = object else {
        return Err(AppError::runtime(format!(
            "{context} result가 object가 아닙니다."
        )));
    };
    object_string_value(&Value::Object(object.clone()), key, context)
}

pub(super) fn object_string_value(
    object: &Value,
    key: &str,
    context: &str,
) -> Result<String, AppError> {
    let Value::Object(object) = object else {
        return Err(AppError::runtime(format!(
            "{context} result가 object가 아닙니다."
        )));
    };
    let Some(Value::String(value)) = object.get(key) else {
        return Err(AppError::runtime(format!(
            "{context} response에 {key} string이 없습니다."
        )));
    };
    Ok(value.clone())
}

fn json_number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(value) => value.to_string().parse().ok(),
        Value::Decimal(value) => value.parse().ok(),
        _ => None,
    }
}

pub(super) fn json_string(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 2);
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\u{08}' => output.push_str("\\b"),
            '\u{0c}' => output.push_str("\\f"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character.is_control() => {
                output.push_str(&format!("\\u{:04x}", u32::from(character)));
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}
