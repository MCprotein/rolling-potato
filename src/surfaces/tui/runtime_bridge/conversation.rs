#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TuiAttachmentKind {
    Image,
    Text,
}

impl TuiAttachmentKind {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Text => "file",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TuiAttachment {
    pub(crate) id: String,
    pub(crate) display_name: String,
    pub(crate) stored_path: String,
    pub(crate) size_bytes: u64,
    pub(crate) kind: TuiAttachmentKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TuiConversationRole {
    User,
    Assistant,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TuiConversationTurn {
    pub(crate) role: TuiConversationRole,
    pub(crate) content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TuiSessionOption {
    pub(crate) session_id: String,
    pub(crate) preview: String,
    pub(crate) current: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TuiSessionTransition {
    pub(crate) session_id: String,
    pub(crate) notice: String,
    pub(crate) turns: Vec<TuiConversationTurn>,
}
