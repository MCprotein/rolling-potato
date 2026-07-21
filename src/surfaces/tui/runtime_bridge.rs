use crate::foundation::error::AppError;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) const TUI_MAX_ITEMS: usize = 120;
pub(crate) const TUI_MAX_CHARS: usize = 65_536;

static TUI_INTENT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(crate) fn new_tui_intent_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = TUI_INTENT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("intent-tui-{:x}-{:x}-{sequence:x}", std::process::id(), now)
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TuiStatusSnapshot {
    pub(crate) model: String,
    pub(crate) context_tokens_used: Option<u32>,
    pub(crate) context_limit_tokens: Option<u32>,
    pub(crate) backend: TuiBackendStatus,
    pub(crate) session_id: String,
}

impl TuiStatusSnapshot {
    pub(crate) fn unavailable() -> Self {
        Self {
            model: "미선택".to_string(),
            context_tokens_used: None,
            context_limit_tokens: None,
            backend: TuiBackendStatus::Unavailable,
            session_id: "미초기화".to_string(),
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TuiModelOption {
    pub(crate) id: String,
    pub(crate) display_name: String,
    pub(crate) quantization: String,
    pub(crate) download_bytes: u64,
    pub(crate) context_length: Option<u32>,
    pub(crate) ram: String,
    pub(crate) license: String,
    pub(crate) note: String,
    pub(crate) recommended: bool,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectionObservation {
    pub(crate) project_id: String,
    pub(crate) session_id: String,
    pub(crate) current_revision: u64,
    pub(crate) current_hash: String,
    pub(crate) active_workflow: Option<ObservedWorkflow>,
}

impl SelectionObservation {
    pub(crate) fn lease_for(&self, selected_object_id: &str) -> SelectionLease {
        SelectionLease {
            project_id: self.project_id.clone(),
            session_id: self.session_id.clone(),
            selected_object_id: selected_object_id.to_string(),
            current_revision: self.current_revision,
            current_hash: self.current_hash.clone(),
            active_session_id: self.session_id.clone(),
            active_workflow: self.active_workflow.clone(),
        }
    }
}

pub(crate) fn lease_matches_active_workflow(
    lease: &SelectionLease,
    workflow_id: &str,
    observation: &SelectionObservation,
) -> bool {
    lease.selected_object_id == workflow_id
        && observation
            .active_workflow
            .as_ref()
            .is_some_and(|workflow| workflow.workflow_id == workflow_id)
        && observation.lease_for(workflow_id) == *lease
}

pub(crate) fn lease_matches_terminal_selection(
    lease: &SelectionLease,
    workflow_id: &str,
    observation: &SelectionObservation,
) -> bool {
    lease.selected_object_id == workflow_id && observation.lease_for(workflow_id) == *lease
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TuiGateKind {
    PatchApply,
    VerificationCommand,
}

pub(crate) enum TuiIntent {
    #[allow(dead_code)]
    Refresh { request: TuiReadRequest },
    #[allow(dead_code)]
    Inspect { request: TuiReadRequest },
    ApprovePatch {
        intent_id: String,
        proposal_id: String,
        lease: SelectionLease,
        secret: OneShotSecret,
    },
    ApproveVerification {
        intent_id: String,
        proposal_id: String,
        lease: SelectionLease,
        secret: OneShotSecret,
    },
    DenyPendingGate {
        intent_id: String,
        workflow_id: String,
        gate_id: String,
        gate_kind: TuiGateKind,
        lease: SelectionLease,
    },
    ResumeWorkflow {
        intent_id: String,
        workflow_id: String,
        lease: SelectionLease,
    },
    CancelWorkflow {
        intent_id: String,
        workflow_id: String,
        lease: SelectionLease,
    },
    #[allow(dead_code)]
    SelectSession {
        intent_id: String,
        session_id: String,
        lease: SelectionLease,
    },
    #[allow(dead_code)]
    ResumeSession {
        intent_id: String,
        session_id: String,
        lease: SelectionLease,
    },
}

pub(crate) struct OneShotSecret(Vec<u8>);

impl OneShotSecret {
    pub(crate) fn new(value: String) -> Result<Self, AppError> {
        if value.is_empty() {
            return Err(AppError::blocked(
                "비밀 입력 차단\n- 이유: 빈 비밀값은 사용할 수 없습니다.",
            ));
        }
        Ok(Self(value.into_bytes()))
    }

    pub(crate) fn expose<R>(self, use_plaintext: impl FnOnce(&str) -> R) -> R {
        let plaintext = std::str::from_utf8(&self.0)
            .expect("OneShotSecret is constructed only from valid UTF-8 String values");
        use_plaintext(plaintext)
    }
}

impl Drop for OneShotSecret {
    fn drop(&mut self) {
        for byte in &mut self.0 {
            // SAFETY: `byte` is a valid, uniquely borrowed byte in the owned buffer.
            unsafe { std::ptr::write_volatile(byte, 0) };
        }
        std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
    }
}
