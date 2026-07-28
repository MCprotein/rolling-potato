use crate::app::web_search_adapter::WebGroundingEvidence;
use crate::foundation::serialization::{self as strict_json, CanonicalObject, CanonicalValue};

const CONVERSATION_EVENT_SCHEMA_VERSION: u64 = 1;
pub(super) const MAX_WEB_GROUNDING_EXCERPT_CHARS: usize = 1_536;

pub(super) enum ConversationEvent {
    Reset,
    RuntimeError(String),
    WebGrounding(WebGroundingEvidence),
}

pub(super) fn render_reset_event() -> String {
    render_event(vec![string_entry("event_type", "reset")])
}

pub(super) fn render_runtime_error_event(content: &str) -> String {
    render_event(vec![
        string_entry("event_type", "runtime_error"),
        string_entry("content", content),
    ])
}

pub(super) fn render_web_grounding_event(evidence: &WebGroundingEvidence) -> String {
    render_event(vec![
        string_entry("event_type", "web_evidence"),
        string_entry("source_id", &evidence.source_id),
        string_entry("title", &evidence.title),
        string_entry("url", &evidence.url),
        string_entry(
            "excerpt",
            &evidence
                .excerpt
                .chars()
                .take(MAX_WEB_GROUNDING_EXCERPT_CHARS)
                .collect::<String>(),
        ),
    ])
}

fn render_event(mut entries: Vec<(String, CanonicalValue)>) -> String {
    entries.insert(
        0,
        (
            "schema_version".to_string(),
            CanonicalValue::Unsigned {
                raw: CONVERSATION_EVENT_SCHEMA_VERSION.to_string(),
            },
        ),
    );
    strict_json::render_canonical_object(&CanonicalObject { entries })
}

fn string_entry(key: &str, value: &str) -> (String, CanonicalValue) {
    (key.to_string(), CanonicalValue::String(value.to_string()))
}

pub(super) fn parse_conversation_event(content: &str) -> Option<ConversationEvent> {
    let object = strict_json::parse_object(
        content,
        &[
            "schema_version",
            "event_type",
            "content",
            "source_id",
            "title",
            "url",
            "excerpt",
        ],
        "conversation event",
    )
    .ok()?;
    if strict_json::number(&object, "schema_version", "conversation event").ok()?
        != CONVERSATION_EVENT_SCHEMA_VERSION
    {
        return None;
    }
    match strict_json::string(&object, "event_type", "conversation event")
        .ok()?
        .as_str()
    {
        "reset" => Some(ConversationEvent::Reset),
        "runtime_error" => Some(ConversationEvent::RuntimeError(
            strict_json::string(&object, "content", "conversation event").ok()?,
        )),
        "web_evidence" => Some(ConversationEvent::WebGrounding(WebGroundingEvidence {
            source_id: strict_json::string(&object, "source_id", "conversation event").ok()?,
            title: strict_json::string(&object, "title", "conversation event").ok()?,
            url: strict_json::string(&object, "url", "conversation event").ok()?,
            excerpt: strict_json::string(&object, "excerpt", "conversation event").ok()?,
        })),
        _ => None,
    }
}
