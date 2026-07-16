pub(crate) const TUI_MAX_ITEMS: usize = 120;
pub(crate) const TUI_MAX_CHARS: usize = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TuiReadBudget {
    pub(crate) max_items: usize,
    pub(crate) max_chars: usize,
}

impl TuiReadBudget {
    pub(crate) fn bounded(max_items: usize, max_chars: usize) -> Self {
        Self {
            max_items: max_items.clamp(1, TUI_MAX_ITEMS),
            max_chars: max_chars.clamp(1, TUI_MAX_CHARS),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TuiReadRequest {
    Overview {
        budget: TuiReadBudget,
    },
    Monitor {
        budget: TuiReadBudget,
    },
    Sessions {
        page: u64,
        budget: TuiReadBudget,
    },
    Transcript {
        session_id: String,
        page: u64,
        budget: TuiReadBudget,
    },
    ToolOutput {
        artifact_id: String,
        page: u64,
        budget: TuiReadBudget,
    },
    Approvals {
        page: u64,
        budget: TuiReadBudget,
    },
    Diff {
        proposal_id: String,
        page: u64,
        budget: TuiReadBudget,
    },
    Evidence {
        page: u64,
        budget: TuiReadBudget,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TuiReadPage {
    pub(crate) title: String,
    pub(crate) lines: Vec<String>,
    pub(crate) page: u64,
    pub(crate) has_previous: bool,
    pub(crate) has_next: bool,
    pub(crate) freshness: TuiFreshness,
    pub(crate) continuation: TuiReadContinuation,
    pub(crate) authority: TuiReadAuthority,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TuiReadContinuation {
    Complete,
    NextPage,
    Truncated,
    Unavailable,
    Redacted,
}

impl TuiReadContinuation {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::NextPage => "next-page",
            Self::Truncated => "truncated",
            Self::Unavailable => "unavailable",
            Self::Redacted => "redacted",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct TuiReadAuthority {
    pub(crate) current_revision: Option<u64>,
    pub(crate) current_hash: Option<String>,
    pub(crate) workflow_revision: Option<u64>,
    pub(crate) workflow_hash: Option<String>,
    pub(crate) ledger_sequence: Option<u64>,
    pub(crate) ledger_hash: Option<String>,
    pub(crate) projected_sequence: Option<u64>,
    pub(crate) content_hash: Option<String>,
    pub(crate) transcript_hash: Option<String>,
    pub(crate) validated_at_ms: Option<u128>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectionLease {
    pub(crate) project_id: String,
    pub(crate) session_id: String,
    pub(crate) selected_object_id: String,
    pub(crate) current_revision: u64,
    pub(crate) current_hash: String,
    pub(crate) active_session_id: String,
    pub(crate) active_workflow: Option<ObservedWorkflow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObservedWorkflow {
    pub(crate) workflow_id: String,
    pub(crate) revision: u64,
    pub(crate) hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TuiGateKind {
    PatchApply,
    VerificationCommand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TuiFreshness {
    Fresh,
    Stale,
    Unavailable,
    ProjectionLag,
}

impl TuiFreshness {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Fresh => "fresh",
            Self::Stale => "stale",
            Self::Unavailable => "unavailable",
            Self::ProjectionLag => "projection-lag",
        }
    }
}
