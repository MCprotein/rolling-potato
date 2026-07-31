const MAX_TOOL_INPUT_CHARS: usize = 512;
const MAX_TOOL_SOURCE_IDS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::app::tui_adapter) enum ConversationToolName {
    Search,
    Open,
    Find,
}

impl ConversationToolName {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Search => "web_search",
            Self::Open => "web_open",
            Self::Find => "web_find",
        }
    }

    pub(super) fn parse(value: &str) -> Option<Self> {
        match value {
            "web_search" => Some(Self::Search),
            "web_open" => Some(Self::Open),
            "web_find" => Some(Self::Find),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::app::tui_adapter) enum ConversationToolStatus {
    Succeeded,
    Failed,
    Blocked,
    Cancelled,
}

impl ConversationToolStatus {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Blocked => "blocked",
            Self::Cancelled => "cancelled",
        }
    }

    pub(super) fn parse(value: &str) -> Option<Self> {
        match value {
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            "blocked" => Some(Self::Blocked),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::app::tui_adapter) struct ConversationToolActivity {
    pub(in crate::app::tui_adapter) execution_id: String,
    pub(in crate::app::tui_adapter) tool: ConversationToolName,
    pub(in crate::app::tui_adapter) input: String,
    pub(in crate::app::tui_adapter) status: ConversationToolStatus,
    pub(in crate::app::tui_adapter) source_ids: Vec<String>,
}

impl ConversationToolActivity {
    pub(in crate::app::tui_adapter) fn bounded(
        execution_id: impl Into<String>,
        tool: ConversationToolName,
        input: &str,
        status: ConversationToolStatus,
        source_ids: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            execution_id: bounded_text(&execution_id.into(), 96),
            tool,
            input: bounded_text(input, MAX_TOOL_INPUT_CHARS),
            status,
            source_ids: source_ids
                .into_iter()
                .filter_map(|source_id| {
                    let source_id = bounded_text(&source_id, 128);
                    (!source_id.is_empty()).then_some(source_id)
                })
                .take(MAX_TOOL_SOURCE_IDS)
                .collect(),
        }
    }
}

fn bounded_text(value: &str, limit: usize) -> String {
    value
        .chars()
        .filter(|character| !character.is_control() || character.is_whitespace())
        .take(limit)
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_activity_bounds_untrusted_fields_and_source_count() {
        let activity = ConversationToolActivity::bounded(
            "execution\u{0000}-1",
            ConversationToolName::Search,
            &"검색어".repeat(300),
            ConversationToolStatus::Succeeded,
            (0..20).map(|index| format!("source-{index}")),
        );

        assert_eq!(activity.execution_id, "execution-1");
        assert_eq!(activity.input.chars().count(), MAX_TOOL_INPUT_CHARS);
        assert_eq!(activity.source_ids.len(), MAX_TOOL_SOURCE_IDS);
    }
}
