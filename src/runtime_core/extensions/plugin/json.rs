use crate::foundation::error::AppError;

pub(crate) fn extract_json_string_field(text: &str, field: &str) -> Option<String> {
    let quoted_field = format!("\"{field}\"");
    let field_index = text.find(&quoted_field)?;
    let after_field = &text[field_index + quoted_field.len()..];
    let colon_index = after_field.find(':')?;
    let after_colon = after_field[colon_index + 1..].trim_start();
    let value = after_colon.strip_prefix('"')?;

    let mut escaped = false;
    let mut result = String::new();
    for ch in value.chars() {
        if escaped {
            result.push(ch);
            escaped = false;
            continue;
        }

        match ch {
            '\\' => escaped = true,
            '"' => return Some(result),
            _ => result.push(ch),
        }
    }

    None
}

pub(crate) fn extract_json_string_array(text: &str, field: &str) -> Vec<String> {
    let quoted_field = format!("\"{field}\"");
    let Some(field_index) = text.find(&quoted_field) else {
        return Vec::new();
    };
    let after_field = &text[field_index + quoted_field.len()..];
    let Some(colon_index) = after_field.find(':') else {
        return Vec::new();
    };
    let after_colon = after_field[colon_index + 1..].trim_start();
    let Some(array_body) = after_colon.strip_prefix('[') else {
        return Vec::new();
    };
    let Some(end) = array_body.find(']') else {
        return Vec::new();
    };

    array_body[..end]
        .split(',')
        .filter_map(|item| {
            let trimmed = item.trim();
            trimmed
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .map(|value| value.replace("\\\"", "\""))
        })
        .collect()
}

pub(crate) fn required_field(text: &str, field: &str) -> Result<String, AppError> {
    extract_json_string_field(text, field).ok_or_else(|| {
        AppError::usage(format!(
            "normalized plugin manifest에 필수 field가 없습니다: {field}"
        ))
    })
}

pub(crate) fn required_usize(text: &str, field: &str) -> Result<usize, AppError> {
    let quoted_field = format!("\"{field}\":");
    let start = text.find(&quoted_field).ok_or_else(|| {
        AppError::usage(format!(
            "normalized plugin manifest에 필수 number field가 없습니다: {field}"
        ))
    })? + quoted_field.len();
    let value = text[start..]
        .chars()
        .skip_while(|ch| ch.is_whitespace())
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    value.parse::<usize>().map_err(|_| {
        AppError::usage(format!(
            "normalized plugin manifest number field가 올바르지 않습니다: {field}"
        ))
    })
}
