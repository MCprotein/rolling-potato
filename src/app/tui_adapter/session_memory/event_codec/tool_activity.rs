use crate::foundation::serialization::{self as strict_json, CanonicalValue};

use super::{render_event, string_entry};
use crate::app::tui_adapter::session_memory::{
    ConversationToolActivity, ConversationToolName, ConversationToolStatus,
};

pub(super) fn render(activity: &ConversationToolActivity) -> String {
    render_event(vec![
        string_entry("event_type", "tool_activity"),
        string_entry("execution_id", &activity.execution_id),
        string_entry("tool", activity.tool.as_str()),
        string_entry("input", &activity.input),
        string_entry("status", activity.status.as_str()),
        (
            "source_ids".to_string(),
            CanonicalValue::Array(
                activity
                    .source_ids
                    .iter()
                    .cloned()
                    .map(CanonicalValue::String)
                    .collect(),
            ),
        ),
    ])
}

pub(super) fn parse(object: &strict_json::Object) -> Option<ConversationToolActivity> {
    Some(ConversationToolActivity::bounded(
        strict_json::string(object, "execution_id", "conversation event").ok()?,
        ConversationToolName::parse(
            &strict_json::string(object, "tool", "conversation event").ok()?,
        )?,
        &strict_json::string(object, "input", "conversation event").ok()?,
        ConversationToolStatus::parse(
            &strict_json::string(object, "status", "conversation event").ok()?,
        )?,
        string_array(object, "source_ids")?,
    ))
}

fn string_array(object: &strict_json::Object, key: &str) -> Option<Vec<String>> {
    let strict_json::Value::Array(values) = object.get(key)? else {
        return None;
    };
    values
        .iter()
        .map(|value| match value {
            strict_json::Value::String(value) => Some(value.clone()),
            _ => None,
        })
        .collect()
}
