use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::adapters::filesystem::{layout as paths, lease};
use crate::foundation::error::AppError;
pub use crate::runtime_core::policy::redaction::{contains_sensitive_text, redact_text};
pub(crate) use crate::runtime_core::workflow::application::transaction_coordinator::PlannedEvent;
use crate::runtime_core::workflow::application::transaction_coordinator::TransactionCoordinator;
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

use super::transition;

mod derived;
mod query;
mod storage;

#[cfg(test)]
use derived::render_chained_ledger;
use derived::{converge_derived_outputs_unlocked, validate_derived_outputs_unlocked};
pub use query::{
    event_detail_exists, event_details_match, workflow_checkpoint_exists, workflow_checkpoints,
};
pub use storage::read_runtime_events;
pub(crate) use storage::read_runtime_tail_read_only;
use storage::{append_chained_event, read_runtime_events_unlocked};
#[cfg(test)]
use storage::{ledger_head_path, validate_ledger_contents, write_ledger_head};

static EVENT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReadOnlyLedgerTail {
    pub binding: LedgerBinding,
    pub events: Vec<ParsedLedgerEvent>,
    pub truncated: bool,
}

pub(crate) struct LedgerWriterGuard {
    _lease: lease::RecoverableLease,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AppendedEvent {
    pub ordinal: u64,
    pub event_hash: String,
}

pub(crate) struct EventSink<'guard> {
    guard: &'guard LedgerWriterGuard,
    coordinator: TransactionCoordinator<'guard>,
}

impl LedgerWriterGuard {
    pub(crate) fn acquire() -> Result<Self, AppError> {
        let lease = lease::RecoverableLease::acquire_with_wait(
            paths::runtime_ledger_writer_lock(),
            "runtime ledger writer",
            Duration::from_secs(5),
        )?;
        read_runtime_events_unlocked()?;
        Ok(Self { _lease: lease })
    }

    #[cfg(test)]
    fn acquire_after_first_block(on_first_block: impl FnOnce()) -> Result<Self, AppError> {
        let lease = lease::RecoverableLease::acquire_with_wait_after_first_block(
            paths::runtime_ledger_writer_lock(),
            "runtime ledger writer",
            Duration::from_secs(5),
            on_first_block,
        )?;
        read_runtime_events_unlocked()?;
        Ok(Self { _lease: lease })
    }

    pub(crate) fn events(&self) -> Result<Vec<ParsedLedgerEvent>, AppError> {
        read_runtime_events_unlocked()
    }

    pub(crate) fn binding(&self) -> Result<LedgerBinding, AppError> {
        let events = read_runtime_events_unlocked()?;
        let Some(last) = events.last() else {
            return Ok(LedgerBinding {
                event_count: 0,
                event_id: None,
                event_hash: "root".to_string(),
            });
        };
        Ok(LedgerBinding {
            event_count: u64::try_from(events.len())
                .map_err(|_| AppError::blocked("ledger event count overflow"))?,
            event_id: Some(last.event_id.clone()),
            event_hash: last
                .event_hash
                .clone()
                .ok_or_else(|| AppError::blocked("ledger head hash 누락"))?,
        })
    }

    pub(crate) fn append_planned(&self, event: &LedgerEvent) -> Result<AppendedEvent, AppError> {
        let existing = read_runtime_events_unlocked()?;
        if let Some((index, installed)) = existing
            .iter()
            .enumerate()
            .find(|(_, installed)| installed.event_id == event.event_id)
        {
            if !same_semantic_event(installed, event) {
                return Err(AppError::blocked(format!(
                    "planned ledger event id 충돌\n- event id: {}",
                    event.event_id
                )));
            }
            self.converge_derived(&event.project_id)?;
            return Ok(AppendedEvent {
                ordinal: u64::try_from(index + 1)
                    .map_err(|_| AppError::blocked("ledger ordinal overflow"))?,
                event_hash: installed
                    .event_hash
                    .clone()
                    .ok_or_else(|| AppError::blocked("planned ledger event hash 누락"))?,
            });
        }
        let planned = self.plan_events(std::slice::from_ref(event))?;
        let appended = self.append_runtime_planned(&planned[0])?;
        self.converge_derived(&event.project_id)?;
        Ok(appended)
    }

    pub(crate) fn plan_events(
        &self,
        events: &[LedgerEvent],
    ) -> Result<Vec<PlannedEvent>, AppError> {
        let before = read_runtime_events_unlocked()?;
        let base = u64::try_from(before.len())
            .map_err(|_| AppError::blocked("ledger event count overflow"))?;
        let mut previous = ledger_previous_hash(&before)?;
        let mut seen = std::collections::BTreeSet::new();
        let mut planned = Vec::with_capacity(events.len());
        for (index, event) in events.iter().enumerate() {
            if !seen.insert(event.event_id.as_str()) {
                return Err(AppError::blocked("planned ledger duplicate event id"));
            }
            if before
                .iter()
                .any(|existing| existing.event_id == event.event_id)
            {
                return Err(AppError::blocked(format!(
                    "planned ledger event는 이미 존재함\n- event id: {}",
                    event.event_id
                )));
            }
            if before
                .iter()
                .any(|existing| same_semantic_payload(existing, event))
            {
                return Err(AppError::blocked(format!(
                    "planned ledger semantic payload가 다른 event id로 이미 존재함\n- event id: {}",
                    event.event_id
                )));
            }
            let offset = u64::try_from(index + 1)
                .map_err(|_| AppError::blocked("ledger ordinal overflow"))?;
            let ordinal = base
                .checked_add(offset)
                .ok_or_else(|| AppError::blocked("ledger ordinal overflow"))?;
            let event_hash = planned_event_hash(event, &previous);
            planned.push(PlannedEvent {
                event: event.clone(),
                ordinal,
                previous_event_hash: previous,
                event_hash: event_hash.clone(),
            });
            previous = event_hash;
        }
        Ok(planned)
    }

    pub(crate) fn append_runtime_planned(
        &self,
        planned: &PlannedEvent,
    ) -> Result<AppendedEvent, AppError> {
        let before = read_runtime_events_unlocked()?;
        if let Some((index, existing)) = before
            .iter()
            .enumerate()
            .find(|(_, existing)| existing.event_id == planned.event.event_id)
        {
            let ordinal = u64::try_from(index + 1)
                .map_err(|_| AppError::blocked("ledger ordinal overflow"))?;
            if !same_semantic_event(existing, &planned.event)
                || ordinal != planned.ordinal
                || existing.previous_event_hash.as_deref()
                    != Some(planned.previous_event_hash.as_str())
                || existing.event_hash.as_deref() != Some(planned.event_hash.as_str())
            {
                return Err(AppError::blocked(
                    "planned ledger installed event binding 충돌",
                ));
            }
            return Ok(AppendedEvent {
                ordinal,
                event_hash: planned.event_hash.clone(),
            });
        }
        let ordinal = u64::try_from(before.len() + 1)
            .map_err(|_| AppError::blocked("ledger ordinal overflow"))?;
        if ordinal != planned.ordinal
            || ledger_previous_hash(&before)? != planned.previous_event_hash
        {
            return Err(AppError::blocked(
                "planned ledger predecessor/ordinal changed before append",
            ));
        }
        append_chained_event(&paths::runtime_ledger_file(), &planned.event)?;
        let after = read_runtime_events_unlocked()?;
        let installed = after
            .get(before.len())
            .ok_or_else(|| AppError::blocked("planned ledger append reread 누락"))?;
        if !same_semantic_event(installed, &planned.event)
            || installed.previous_event_hash.as_deref()
                != Some(planned.previous_event_hash.as_str())
            || installed.event_hash.as_deref() != Some(planned.event_hash.as_str())
        {
            return Err(AppError::blocked(
                "planned ledger append ordinal/semantic binding 불일치",
            ));
        }
        Ok(AppendedEvent {
            ordinal: planned.ordinal,
            event_hash: planned.event_hash.clone(),
        })
    }

    pub(crate) fn converge_derived(&self, project_id: &str) -> Result<(), AppError> {
        let events = read_runtime_events_unlocked()?;
        converge_derived_outputs_unlocked(&events, project_id)
    }

    pub(crate) fn prepared_target_is_converged(
        &self,
        bundle: &transition::PreparedSourceBundle,
        journal: &Path,
    ) -> Result<bool, AppError> {
        transition::validate_committed_bundle_cleanup_authority(bundle, journal)?;
        let planned = transition::planned_events(bundle)?;
        let events = read_runtime_events_unlocked()?;
        let target_ordinal = planned
            .last()
            .map(|event| event.ordinal)
            .ok_or_else(|| AppError::blocked("prepared convergence target event 누락"))?;
        let installed_count = u64::try_from(events.len())
            .map_err(|_| AppError::blocked("prepared convergence runtime count overflow"))?;
        if installed_count > target_ordinal {
            return Err(AppError::blocked(
                "prepared convergence runtime head가 journal target을 초과했습니다.",
            ));
        }
        for expected in planned
            .iter()
            .filter(|event| event.ordinal <= installed_count)
        {
            let index = usize::try_from(expected.ordinal.saturating_sub(1))
                .map_err(|_| AppError::blocked("prepared convergence ordinal overflow"))?;
            let installed = events
                .get(index)
                .ok_or_else(|| AppError::blocked("prepared convergence runtime prefix 누락"))?;
            if !same_semantic_event(installed, &expected.event)
                || installed.previous_event_hash.as_deref()
                    != Some(expected.previous_event_hash.as_str())
                || installed.event_hash.as_deref() != Some(expected.event_hash.as_str())
            {
                return Err(AppError::blocked(
                    "prepared convergence runtime prefix가 journal과 충돌합니다.",
                ));
            }
        }
        if installed_count != target_ordinal {
            return Ok(false);
        }
        validate_prepared_runtime_suffix(&events, &planned)?;
        Ok(validate_derived_outputs_unlocked(&events, &bundle.project_id).is_ok())
    }

    pub(crate) fn event_sink<'guard>(
        &'guard self,
        planned: &'guard [PlannedEvent],
    ) -> EventSink<'guard> {
        EventSink {
            guard: self,
            coordinator: TransactionCoordinator::new(planned),
        }
    }
}

impl EventSink<'_> {
    pub(crate) fn append_planned_under_guard(
        &mut self,
        index: usize,
        event: &LedgerEvent,
    ) -> Result<AppendedEvent, AppError> {
        let planned = self.coordinator.validate_next(index, event)?;
        let appended = self.guard.append_runtime_planned(planned)?;
        self.coordinator.record_appended(index)?;
        Ok(appended)
    }

    pub(crate) fn finish(&self) -> Result<(), AppError> {
        self.coordinator.finish()
    }

    pub(crate) fn converge_derived(&self, project_id: &str) -> Result<(), AppError> {
        self.guard.converge_derived(project_id)?;
        let events = self.guard.events()?;
        crate::app::observability_adapter::converge_from_events(&events)
    }

    pub(crate) fn converge_prepared(
        &self,
        bundle: &transition::PreparedSourceBundle,
        journal: &Path,
    ) -> Result<(), AppError> {
        self.finish()?;
        transition::validate_committed_bundle_cleanup_authority(bundle, journal)?;
        let prepared = transition::planned_events(bundle)?;
        let planned = self.coordinator.planned();
        if prepared.len() != planned.len()
            || prepared.iter().zip(planned).any(|(left, right)| {
                left.event != right.event
                    || left.ordinal != right.ordinal
                    || left.previous_event_hash != right.previous_event_hash
                    || left.event_hash != right.event_hash
            })
        {
            return Err(AppError::blocked(
                "prepared convergence event sink/journal binding 불일치",
            ));
        }
        self.guard.converge_derived(&bundle.project_id)?;
        let events = self.guard.events()?;
        crate::app::observability_adapter::converge_from_events(&events)?;
        validate_prepared_runtime_suffix(&events, &prepared)?;
        validate_derived_outputs_unlocked(&events, &bundle.project_id)?;
        transition::validate_committed_bundle_cleanup_authority(bundle, journal)
    }
}

fn validate_prepared_runtime_suffix(
    events: &[ParsedLedgerEvent],
    planned: &[PlannedEvent],
) -> Result<(), AppError> {
    let Some(final_event) = planned.last() else {
        return Err(AppError::blocked("prepared convergence event plan 누락"));
    };
    if u64::try_from(events.len()).ok() != Some(final_event.ordinal) {
        return Err(AppError::blocked(
            "prepared convergence runtime ledger head ordinal 불일치",
        ));
    }
    for expected in planned {
        let index = usize::try_from(expected.ordinal.saturating_sub(1))
            .map_err(|_| AppError::blocked("prepared convergence ordinal overflow"))?;
        let installed = events
            .get(index)
            .ok_or_else(|| AppError::blocked("prepared convergence runtime event 누락"))?;
        if !same_semantic_event(installed, &expected.event)
            || installed.previous_event_hash.as_deref()
                != Some(expected.previous_event_hash.as_str())
            || installed.event_hash.as_deref() != Some(expected.event_hash.as_str())
        {
            return Err(AppError::blocked(
                "prepared convergence runtime event/head binding 불일치",
            ));
        }
    }
    Ok(())
}

fn ledger_previous_hash(events: &[ParsedLedgerEvent]) -> Result<String, AppError> {
    events.last().map_or_else(
        || Ok("root".to_string()),
        |last| {
            last.event_hash
                .clone()
                .ok_or_else(|| AppError::blocked("planned ledger predecessor hash 누락"))
        },
    )
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

pub fn append_event(event: &LedgerEvent) -> Result<(), AppError> {
    LedgerWriterGuard::acquire()?.append_planned(event)?;
    Ok(())
}

fn same_semantic_event(existing: &ParsedLedgerEvent, event: &LedgerEvent) -> bool {
    existing.event_id == event.event_id && same_semantic_payload(existing, event)
}

fn same_semantic_payload(existing: &ParsedLedgerEvent, event: &LedgerEvent) -> bool {
    existing.ts_ms == event.ts_ms
        && existing.event_type == event.event_type
        && existing.project_id == event.project_id
        && existing.session_id == event.session_id
        && existing.summary == event.summary
        && existing.details == event.details
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
