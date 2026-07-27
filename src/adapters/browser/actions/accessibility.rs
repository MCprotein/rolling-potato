use std::collections::BTreeSet;

use crate::foundation::error::AppError;
use crate::foundation::serialization::{Object, Value};
use crate::runtime_core::browser::{ElementRole, ObservedTargetSeed};

pub(super) fn interactive_targets(result: &Value) -> Result<Vec<ObservedTargetSeed>, AppError> {
    let nodes = object_array(result, "nodes", "Accessibility.getFullAXTree")?;
    let mut targets = Vec::new();
    for node in nodes {
        let Value::Object(node) = node else {
            continue;
        };
        if object_bool(node, "ignored").unwrap_or(false) {
            continue;
        }
        let Some(target_ref) = object_number(node, "backendDOMNodeId") else {
            continue;
        };
        let role_name = ax_value_string(node, "role").unwrap_or_default();
        let role = element_role(&role_name);
        if role == ElementRole::Other {
            continue;
        }
        let name = ax_value_string(node, "name").unwrap_or_default();
        targets.push(ObservedTargetSeed {
            target_ref,
            role,
            disabled: ax_disabled(node),
            sensitive: sensitive_target(&role_name, &name),
            name,
        });
    }
    Ok(targets)
}

pub(super) fn extract_accessibility_text(
    result: &Value,
    max_chars: usize,
) -> Result<String, AppError> {
    let nodes = object_array(result, "nodes", "Accessibility.getFullAXTree")?;
    let mut seen = BTreeSet::new();
    let mut output = String::new();
    for node in nodes {
        let Value::Object(node) = node else {
            continue;
        };
        let role = ax_value_string(node, "role").unwrap_or_default();
        if !matches!(
            role.to_ascii_lowercase().as_str(),
            "statictext" | "heading" | "paragraph" | "listitem" | "cell"
        ) {
            continue;
        }
        let Some(name) = ax_value_string(node, "name") else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() || !seen.insert(name.to_string()) {
            continue;
        }
        let separator = usize::from(!output.is_empty());
        let remaining = max_chars.saturating_sub(output.chars().count() + separator);
        if remaining == 0 {
            break;
        }
        if separator == 1 {
            output.push('\n');
        }
        output.extend(name.chars().take(remaining));
    }
    Ok(output)
}

fn element_role(role: &str) -> ElementRole {
    match role.to_ascii_lowercase().as_str() {
        "searchbox" => ElementRole::SearchBox,
        "textbox" | "textfield" => ElementRole::TextField,
        "button" => ElementRole::Button,
        "link" => ElementRole::Link,
        "checkbox" => ElementRole::Checkbox,
        "radio" | "radiobutton" => ElementRole::Radio,
        _ => ElementRole::Other,
    }
}

fn sensitive_target(role: &str, name: &str) -> bool {
    let value = format!("{} {}", role.to_ascii_lowercase(), name.to_lowercase());
    [
        "password",
        "sign in",
        "login",
        "payment",
        "purchase",
        "upload",
        "download",
        "comment",
        "로그인",
        "비밀번호",
        "결제",
        "구매",
        "업로드",
        "다운로드",
        "댓글",
        "게시",
    ]
    .iter()
    .any(|keyword| value.contains(keyword))
}

fn ax_disabled(node: &Object) -> bool {
    let Some(Value::Array(properties)) = node.get("properties") else {
        return false;
    };
    properties.iter().any(|property| {
        let Value::Object(property) = property else {
            return false;
        };
        matches!(property.get("name"), Some(Value::String(name)) if name == "disabled")
            && matches!(
                property.get("value"),
                Some(Value::Object(value))
                    if matches!(value.get("value"), Some(Value::Bool(true)))
            )
    })
}

fn ax_value_string(object: &Object, key: &str) -> Option<String> {
    let Value::Object(value) = object.get(key)? else {
        return None;
    };
    let Value::String(value) = value.get("value")? else {
        return None;
    };
    Some(value.clone())
}

fn object_array<'a>(object: &'a Value, key: &str, context: &str) -> Result<&'a [Value], AppError> {
    let Value::Object(object) = object else {
        return Err(AppError::runtime(format!(
            "{context} result가 object가 아닙니다."
        )));
    };
    let Some(Value::Array(value)) = object.get(key) else {
        return Err(AppError::runtime(format!(
            "{context} response에 {key} array가 없습니다."
        )));
    };
    Ok(value)
}

fn object_bool(object: &Object, key: &str) -> Option<bool> {
    let Value::Bool(value) = object.get(key)? else {
        return None;
    };
    Some(*value)
}

fn object_number(object: &Object, key: &str) -> Option<u64> {
    let Value::Number(value) = object.get(key)? else {
        return None;
    };
    u64::try_from(*value).ok()
}
