//! Team runtime event family classification.

pub(crate) fn is_team_runtime_event(event_type: &str) -> bool {
    event_type.starts_with("team.admission.")
        || event_type.starts_with("team.dispatch.")
        || event_type.starts_with("team.continuation.")
        || event_type.starts_with("team.governor.")
        || event_type.starts_with("team.worker.")
        || event_type.starts_with("team.subagent.")
}
