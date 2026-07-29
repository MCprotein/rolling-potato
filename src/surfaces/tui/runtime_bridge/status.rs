#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TuiStatusSnapshot {
    pub(crate) model: String,
    pub(crate) context_tokens_used: Option<u32>,
    pub(crate) context_limit_tokens: Option<u32>,
    pub(crate) has_compaction_checkpoint: bool,
    pub(crate) backend: TuiBackendStatus,
    pub(crate) vision: TuiVisionStatus,
    pub(crate) session_id: String,
}

impl TuiStatusSnapshot {
    pub(crate) fn unavailable() -> Self {
        Self {
            model: "미선택".to_string(),
            context_tokens_used: None,
            context_limit_tokens: None,
            has_compaction_checkpoint: false,
            backend: TuiBackendStatus::Unavailable,
            vision: TuiVisionStatus::Unavailable,
            session_id: "미초기화".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TuiVisionStatus {
    Ready,
    OnDemand,
    Unsupported,
    Unavailable,
}

impl TuiVisionStatus {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::OnDemand => "on-demand",
            Self::Unsupported => "unsupported",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TuiBackendStatus {
    Ready,
    Stopped,
    Stale,
    Unavailable,
}

impl TuiBackendStatus {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Stopped => "stopped",
            Self::Stale => "stale",
            Self::Unavailable => "unavailable",
        }
    }
}
