use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::adapters::filesystem::layout as paths;
use crate::foundation::error::AppError;
pub use crate::runtime_core::policy::redaction::{contains_sensitive_text, redact_text};
pub(crate) use crate::runtime_core::workflow::application::transaction_coordinator::PlannedEvent;
#[cfg(test)]
use crate::runtime_core::workflow::storage_compat::ledger::append_line;
#[cfg(test)]
use crate::runtime_core::workflow::storage_compat::ledger::event_chain_payload;
#[cfg(test)]
pub use crate::runtime_core::workflow::storage_compat::ledger::parse_event_line;
pub(crate) use crate::runtime_core::workflow::storage_compat::ledger::planned_event_hash;
#[cfg(test)]
use crate::runtime_core::workflow::storage_compat::ledger::sha256_bytes;
pub use crate::runtime_core::workflow::storage_compat::ledger::{
    json_string, LedgerBinding, LedgerEvent, ParsedLedgerEvent, RuntimeIdentity, WorkflowCheckpoint,
};

mod derived;
mod query;
mod storage;
mod writer;

#[cfg(test)]
use derived::{render_chained_ledger, validate_derived_outputs_unlocked};
pub use query::{
    event_detail_exists, event_details_match, workflow_checkpoint_exists, workflow_checkpoints,
};
pub use storage::read_runtime_events;
pub(crate) use storage::read_runtime_tail_read_only;
#[cfg(test)]
use storage::{ledger_head_path, validate_ledger_contents, write_ledger_head};
pub(crate) use writer::{AppendedEvent, EventSink, LedgerWriterGuard};

static EVENT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReadOnlyLedgerTail {
    pub binding: LedgerBinding,
    pub events: Vec<ParsedLedgerEvent>,
    pub truncated: bool,
}

pub fn validated_ledger_binding() -> Result<LedgerBinding, AppError> {
    let events = read_runtime_events()?;
    let event_count = u64::try_from(events.len())
        .map_err(|_| AppError::blocked("runtime ledger event count 범위 초과"))?;
    let Some(last) = events.last() else {
        return Ok(LedgerBinding {
            event_count,
            event_id: None,
            event_hash: "root".to_string(),
        });
    };
    let event_hash = last.event_hash.clone().ok_or_else(|| {
        AppError::blocked(
            "current-state v2 ledger binding 차단\n- 이유: legacy ledger에는 canonical chained head가 없습니다.",
        )
    })?;
    Ok(LedgerBinding {
        event_count,
        event_id: Some(last.event_id.clone()),
        event_hash,
    })
}

pub fn validated_current_identity() -> Result<RuntimeIdentity, AppError> {
    let path = paths::current_state_file();
    if !path.exists() {
        return Ok(fresh_identity());
    }
    let contents = fs::read_to_string(&path)
        .map_err(|err| AppError::blocked(format!("current-state identity 읽기 실패: {err}")))?;
    let fresh = fresh_identity();
    crate::app::workflow_adapter::state::validated_identity_from_current_state(&contents, &fresh)
}

pub fn fresh_identity() -> RuntimeIdentity {
    let project_root = paths::project_root().display().to_string();
    let mut hasher = DefaultHasher::new();
    project_root.hash(&mut hasher);
    let project_id = format!("project-{:016x}", hasher.finish());
    let session_id = format!("session-{}-{}", now_ms(), process::id());

    RuntimeIdentity {
        project_id,
        session_id,
        project_root,
    }
}

pub fn new_event_for(
    identity: &RuntimeIdentity,
    event_type: &str,
    summary: &str,
    details: &str,
) -> LedgerEvent {
    let ts_ms = now_ms();
    let event_id = format!(
        "event-{}-{}-{}-{}",
        now_nanos(),
        process::id(),
        EVENT_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        sanitize_event_type(event_type)
    );

    LedgerEvent {
        event_id,
        ts_ms,
        event_type: event_type.to_string(),
        project_id: identity.project_id.clone(),
        session_id: identity.session_id.clone(),
        summary: summary.to_string(),
        details: redact_text(details),
    }
}

pub(crate) fn append_event(event: &LedgerEvent) -> Result<AppendedEvent, AppError> {
    LedgerWriterGuard::acquire()?.append_planned(event)
}

fn sanitize_event_type(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "ledger/tests.rs"]
mod tests;
