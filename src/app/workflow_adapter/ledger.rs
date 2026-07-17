use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::adapters::filesystem::{layout as paths, lease};
use crate::foundation::error::AppError;
use crate::foundation::serialization as strict_json;
pub use crate::runtime_core::policy::redaction::{contains_sensitive_text, redact_text};
pub(crate) use crate::runtime_core::workflow::application::transaction_coordinator::PlannedEvent;
use crate::runtime_core::workflow::application::transaction_coordinator::TransactionCoordinator;
#[cfg(test)]
use crate::runtime_core::workflow::storage_compat::ledger::append_line;
#[cfg(test)]
use crate::runtime_core::workflow::storage_compat::ledger::event_chain_payload;
#[cfg(test)]
pub use crate::runtime_core::workflow::storage_compat::ledger::parse_event_line;
pub(crate) use crate::runtime_core::workflow::storage_compat::ledger::parse_event_line_strict;
pub(crate) use crate::runtime_core::workflow::storage_compat::ledger::{
    append_canonical_event, event_physical_hash, planned_event_hash, sha256_bytes,
};
pub use crate::runtime_core::workflow::storage_compat::ledger::{
    json_string, LedgerBinding, LedgerEvent, ParsedLedgerEvent, RuntimeIdentity, WorkflowCheckpoint,
};

use super::transition;

mod derived;
mod query;

#[cfg(test)]
use derived::render_chained_ledger;
use derived::{converge_derived_outputs_unlocked, validate_derived_outputs_unlocked};
pub use query::{
    event_detail_exists, event_details_match, workflow_checkpoint_exists, workflow_checkpoints,
};

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

pub fn read_runtime_events() -> Result<Vec<ParsedLedgerEvent>, AppError> {
    let _reader = lease::RecoverableLease::acquire_with_wait(
        paths::runtime_ledger_writer_lock(),
        "runtime ledger reader",
        Duration::from_secs(5),
    )?;
    read_runtime_events_unlocked()
}

pub(crate) fn read_runtime_tail_read_only(
    max_events: usize,
    max_bytes: u64,
) -> Result<ReadOnlyLedgerTail, AppError> {
    if max_events == 0 || max_bytes == 0 {
        return Err(AppError::blocked(
            "runtime ledger read-only budget은 0보다 커야 합니다.",
        ));
    }
    let path = paths::runtime_ledger_file();
    let head_path = ledger_head_path(&path);
    if !path.exists() && !head_path.exists() {
        return Ok(ReadOnlyLedgerTail {
            binding: LedgerBinding {
                event_count: 0,
                event_id: None,
                event_hash: "root".to_string(),
            },
            events: Vec::new(),
            truncated: false,
        });
    }
    ensure_read_only_regular_file(&path, "runtime ledger")?;
    ensure_read_only_regular_file(&head_path, "runtime ledger head")?;
    let head_before = read_ledger_head_read_only(&head_path)?;

    let mut file = fs::File::open(&path)
        .map_err(|err| AppError::blocked(format!("runtime ledger read-only open 실패: {err}")))?;
    let before = file
        .metadata()
        .map_err(|err| AppError::blocked(format!("runtime ledger metadata 실패: {err}")))?;
    let start = before.len().saturating_sub(max_bytes);
    file.seek(SeekFrom::Start(start))
        .map_err(|err| AppError::blocked(format!("runtime ledger tail seek 실패: {err}")))?;
    let mut bytes = Vec::new();
    file.take(max_bytes)
        .read_to_end(&mut bytes)
        .map_err(|err| AppError::blocked(format!("runtime ledger tail 읽기 실패: {err}")))?;
    let after = fs::metadata(&path)
        .map_err(|err| AppError::blocked(format!("runtime ledger reread metadata 실패: {err}")))?;
    let head_after = read_ledger_head_read_only(&head_path)?;
    if before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
        || head_before != head_after
    {
        return Err(AppError::blocked(
            "runtime ledger read-only snapshot 중 canonical head가 변경되었습니다.",
        ));
    }
    if !bytes.is_empty() && !bytes.ends_with(b"\n") {
        return Err(AppError::blocked(
            "runtime ledger read-only tail이 완결된 JSONL record로 끝나지 않습니다.",
        ));
    }
    if start > 0 {
        let Some(boundary) = bytes.iter().position(|byte| *byte == b'\n') else {
            return Err(AppError::blocked(
                "runtime ledger record가 read-only byte budget을 초과했습니다.",
            ));
        };
        bytes.drain(..=boundary);
    }
    let body = std::str::from_utf8(&bytes)
        .map_err(|_| AppError::blocked("runtime ledger tail UTF-8 불일치"))?;
    let lines = body
        .lines()
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if head_before.event_count == 0 {
        if before.len() != 0 || !lines.is_empty() || head_before.event_hash != "root" {
            return Err(AppError::blocked(
                "runtime ledger empty head/file binding 불일치",
            ));
        }
        return Ok(ReadOnlyLedgerTail {
            binding: head_before,
            events: Vec::new(),
            truncated: false,
        });
    }
    let take = lines.len().min(max_events);
    if take == 0 {
        return Err(AppError::blocked(
            "runtime ledger canonical tail이 read-only budget 안에 없습니다.",
        ));
    }
    let mut events = lines[lines.len() - take..]
        .iter()
        .map(|line| parse_event_line_strict(line))
        .collect::<Result<Vec<_>, _>>()?;
    for (index, event) in events.iter().enumerate() {
        let previous = event.previous_event_hash.as_deref().ok_or_else(|| {
            AppError::blocked("runtime ledger read-only view는 chained event만 허용합니다.")
        })?;
        let hash = event
            .event_hash
            .as_deref()
            .ok_or_else(|| AppError::blocked("runtime ledger read-only event hash 누락"))?;
        if hash != event_physical_hash(event, previous) {
            return Err(AppError::blocked(
                "runtime ledger read-only physical hash chain 불일치",
            ));
        }
        if index > 0 && Some(previous) != events[index - 1].event_hash.as_deref() {
            return Err(AppError::blocked(
                "runtime ledger read-only adjacent hash chain 불일치",
            ));
        }
    }
    let last = events
        .last()
        .ok_or_else(|| AppError::blocked("runtime ledger read-only tail 누락"))?;
    if last.event_hash.as_deref() != Some(head_before.event_hash.as_str())
        || u64::try_from(events.len()).ok().is_none()
        || head_before.event_count < events.len() as u64
    {
        return Err(AppError::blocked(
            "runtime ledger read-only tail/head binding 불일치",
        ));
    }
    let binding = LedgerBinding {
        event_count: head_before.event_count,
        event_id: Some(last.event_id.clone()),
        event_hash: head_before.event_hash,
    };
    let truncated = binding.event_count > events.len() as u64;
    events.shrink_to_fit();
    Ok(ReadOnlyLedgerTail {
        binding,
        events,
        truncated,
    })
}

fn ensure_read_only_regular_file(path: &Path, label: &str) -> Result<(), AppError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|err| AppError::blocked(format!("{label} metadata 실패: {err}")))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AppError::blocked(format!(
            "{label} read-only file boundary 불일치"
        )));
    }
    Ok(())
}

fn read_ledger_head_read_only(path: &Path) -> Result<LedgerBinding, AppError> {
    let metadata = fs::metadata(path)
        .map_err(|err| AppError::blocked(format!("runtime ledger head metadata 실패: {err}")))?;
    if metadata.len() > 4_096 {
        return Err(AppError::blocked("runtime ledger head byte limit 초과"));
    }
    let body = fs::read_to_string(path)
        .map_err(|err| AppError::blocked(format!("runtime ledger head 읽기 실패: {err}")))?;
    let object = strict_json::parse_canonical_object(
        body.trim_end_matches('\n'),
        &["schema_version", "event_count", "last_event_hash"],
        "runtime ledger read-only head",
    )?;
    if strict_json::canonical_u64(&object, "schema_version", "runtime ledger read-only head")? != 1
    {
        return Err(AppError::blocked("runtime ledger head schema 불일치"));
    }
    let event_count =
        strict_json::canonical_u64(&object, "event_count", "runtime ledger read-only head")?;
    let event_hash = match object.get("last_event_hash") {
        Some(strict_json::CanonicalValue::String(value)) => value.clone(),
        _ => return Err(AppError::blocked("runtime ledger head hash type 불일치")),
    };
    if event_hash != "root" && !is_sha256(&event_hash) {
        return Err(AppError::blocked("runtime ledger head hash 형식 불일치"));
    }
    Ok(LedgerBinding {
        event_count,
        event_id: None,
        event_hash,
    })
}

fn read_runtime_events_unlocked() -> Result<Vec<ParsedLedgerEvent>, AppError> {
    let path = paths::runtime_ledger_file();
    if !path.exists() {
        if ledger_head_path(&path).exists() {
            return Err(ledger_corrupt(
                &path,
                0,
                "ledger JSONL 없이 orphan head가 존재합니다",
            ));
        }
        return Ok(Vec::new());
    }

    let contents = fs::read_to_string(&path).map_err(|err| {
        AppError::runtime(format!(
            "runtime ledger를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;

    validate_ledger_contents_with_head_repair(&path, &contents)
}

fn validate_ledger_contents(
    path: &Path,
    contents: &str,
) -> Result<Vec<ParsedLedgerEvent>, AppError> {
    validate_ledger_contents_inner(path, contents, false)
}

fn validate_ledger_contents_with_head_repair(
    path: &Path,
    contents: &str,
) -> Result<Vec<ParsedLedgerEvent>, AppError> {
    validate_ledger_contents_inner(path, contents, true)
}

fn validate_ledger_contents_inner(
    path: &Path,
    contents: &str,
    allow_head_repair: bool,
) -> Result<Vec<ParsedLedgerEvent>, AppError> {
    let mut events = Vec::new();
    let mut legacy_prefix = String::new();
    let mut previous_hash: Option<String> = None;
    for (index, line) in contents.lines().enumerate() {
        if line.trim().is_empty() {
            return Err(ledger_corrupt(path, index + 1, "빈 JSONL record"));
        }
        let event = parse_event_line_strict(line)
            .map_err(|_| ledger_corrupt(path, index + 1, "malformed JSONL record"))?;
        match (&event.previous_event_hash, &event.event_hash) {
            (None, None) if previous_hash.is_none() => {
                legacy_prefix.push_str(line);
                legacy_prefix.push('\n');
            }
            (Some(previous), Some(hash)) => {
                let expected_previous = previous_hash.clone().unwrap_or_else(|| {
                    if legacy_prefix.is_empty() {
                        "root".to_string()
                    } else {
                        format!("legacy:{}", sha256_bytes(legacy_prefix.as_bytes()))
                    }
                });
                if previous != &expected_previous || hash != &event_physical_hash(&event, previous)
                {
                    return Err(ledger_corrupt(
                        path,
                        index + 1,
                        "physical hash chain 불일치",
                    ));
                }
                previous_hash = Some(hash.clone());
            }
            _ => {
                return Err(ledger_corrupt(
                    path,
                    index + 1,
                    "legacy event가 chained suffix 뒤에 존재함",
                ))
            }
        }
        events.push(event);
    }
    validate_ledger_head(
        path,
        &events,
        previous_hash.as_deref(),
        &legacy_prefix,
        allow_head_repair,
    )?;
    Ok(events)
}

fn append_chained_event(path: &Path, event: &LedgerEvent) -> Result<(), AppError> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => {
            return Err(AppError::runtime(format!(
                "ledger append reread 실패: {err}"
            )))
        }
    };
    let existing = validate_ledger_contents(path, &contents)?;
    let previous = existing
        .last()
        .and_then(|entry| entry.event_hash.clone())
        .unwrap_or_else(|| {
            if contents.is_empty() {
                "root".to_string()
            } else {
                format!("legacy:{}", sha256_bytes(contents.as_bytes()))
            }
        });
    let event_hash = append_canonical_event(path, event, &previous)?;
    write_ledger_head(path, existing.len() + 1, &event_hash)
}

fn ledger_head_path(path: &Path) -> std::path::PathBuf {
    path.with_extension(format!(
        "{}.head",
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or("ledger")
    ))
}

fn write_ledger_head(path: &Path, count: usize, hash: &str) -> Result<(), AppError> {
    let body = format!(
        "{{\"schema_version\":1,\"event_count\":{count},\"last_event_hash\":\"{hash}\"}}\n"
    );
    crate::app::workflow_adapter::state::atomic_replace_bytes(
        &ledger_head_path(path),
        body.as_bytes(),
    )
}

fn validate_ledger_head(
    path: &Path,
    events: &[ParsedLedgerEvent],
    last_hash: Option<&str>,
    legacy_prefix: &str,
    allow_repair: bool,
) -> Result<(), AppError> {
    let count = events.len();
    let head_path = ledger_head_path(path);
    if !head_path.exists() {
        if let Some(last_hash) = last_hash {
            let chained_count = events
                .iter()
                .filter(|event| event.event_hash.is_some())
                .count();
            if allow_repair && chained_count == 1 {
                write_ledger_head(path, count, last_hash)?;
                return Ok(());
            }
            return Err(ledger_corrupt(path, count, "chained ledger head 누락"));
        }
        return Ok(());
    }
    let body = fs::read_to_string(&head_path)
        .map_err(|err| AppError::blocked(format!("ledger head 읽기 실패: {err}")))?;
    let object = strict_json::parse_object(
        &body,
        &["schema_version", "event_count", "last_event_hash"],
        "ledger head",
    )?;
    let expected_hash = last_hash.unwrap_or({
        if legacy_prefix.is_empty() {
            "root"
        } else {
            "legacy"
        }
    });
    let schema = strict_json::number(&object, "schema_version", "ledger head")?;
    let head_count = strict_json::number(&object, "event_count", "ledger head")?;
    let head_hash = strict_json::string(&object, "last_event_hash", "ledger head")?;
    if schema == 1 && head_count == count as u64 && head_hash == expected_hash {
        return Ok(());
    }
    if schema == 1 && allow_repair && head_count.checked_add(1) == Some(count as u64) {
        let chained_count = events
            .iter()
            .filter(|event| event.event_hash.is_some())
            .count();
        let previous = events
            .last()
            .and_then(|event| event.previous_event_hash.as_deref());
        let legacy_anchor = (!legacy_prefix.is_empty())
            .then(|| format!("legacy:{}", sha256_bytes(legacy_prefix.as_bytes())));
        let predecessor_matches = previous == Some(head_hash.as_str())
            || (chained_count == 1
                && head_hash == "legacy"
                && previous == legacy_anchor.as_deref());
        if predecessor_matches {
            write_ledger_head(path, count, expected_hash)?;
            return Ok(());
        }
    }
    Err(ledger_corrupt(path, count, "ledger truncation/head 불일치"))
}

fn ledger_corrupt(path: &Path, line: usize, reason: &str) -> AppError {
    let gap = crate::app::workflow_adapter::state::record_validation_gap(
        "corrupt-ledger",
        &format!("{}:{line}:{reason}", path.display()),
    );
    let suffix = gap
        .err()
        .map(|err| format!("\n- validation-gap 저장 실패: {}", err.message))
        .unwrap_or_default();
    AppError::blocked(format!(
        "runtime ledger 검증 차단\n- 이유: {reason}\n- path: {}\n- line: {line}{suffix}",
        path.display()
    ))
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
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
