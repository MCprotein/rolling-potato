use super::read::TuiReadRequest;
use super::selection::SelectionLease;
use crate::foundation::error::AppError;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

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
