use std::collections::hash_map::DefaultHasher;
use std::fs::{self, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::adapters::filesystem::{layout as paths, lease};
use crate::foundation::error::AppError;
use crate::foundation::serialization as strict_json;
#[cfg(test)]
pub use crate::runtime_core::workflow::storage_compat::ledger::parse_event_line;
pub(crate) use crate::runtime_core::workflow::storage_compat::ledger::parse_event_line_strict;
pub(crate) use crate::runtime_core::workflow::storage_compat::ledger::{
    event_chain_payload, event_physical_hash, planned_event_hash, sha256_bytes,
};
pub use crate::runtime_core::workflow::storage_compat::ledger::{
    json_string, LedgerBinding, LedgerEvent, ParsedLedgerEvent, RuntimeIdentity, WorkflowCheckpoint,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlannedEvent {
    pub event: LedgerEvent,
    pub ordinal: u64,
    pub previous_event_hash: String,
    pub event_hash: String,
}

pub(crate) struct EventSink<'guard> {
    guard: &'guard LedgerWriterGuard,
    planned: &'guard [PlannedEvent],
    next_index: usize,
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
        bundle: &crate::transition::PreparedSourceBundle,
        journal: &Path,
    ) -> Result<bool, AppError> {
        crate::transition::validate_committed_bundle_cleanup_authority(bundle, journal)?;
        let planned = crate::transition::planned_events(bundle)?;
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
            planned,
            next_index: 0,
        }
    }
}

impl EventSink<'_> {
    pub(crate) fn append_planned_under_guard(
        &mut self,
        index: usize,
        event: &LedgerEvent,
    ) -> Result<AppendedEvent, AppError> {
        if index != self.next_index {
            return Err(AppError::blocked(format!(
                "transaction event sink 순서 불일치\n- expected index: {}\n- requested index: {index}",
                self.next_index
            )));
        }
        let planned = self
            .planned
            .get(index)
            .ok_or_else(|| AppError::blocked("transaction event sink index 범위 초과"))?;
        if &planned.event != event {
            return Err(AppError::blocked(
                "transaction event sink semantic event binding 불일치",
            ));
        }
        let appended = self.guard.append_runtime_planned(planned)?;
        self.next_index = self
            .next_index
            .checked_add(1)
            .ok_or_else(|| AppError::blocked("transaction event sink index overflow"))?;
        Ok(appended)
    }

    pub(crate) fn finish(&self) -> Result<(), AppError> {
        if self.next_index != self.planned.len() {
            return Err(AppError::blocked(format!(
                "transaction event sink 미완료\n- appended: {}\n- planned: {}",
                self.next_index,
                self.planned.len()
            )));
        }
        Ok(())
    }

    pub(crate) fn converge_derived(&self, project_id: &str) -> Result<(), AppError> {
        self.guard.converge_derived(project_id)?;
        let events = self.guard.events()?;
        crate::observability::converge_from_events(&events)
    }

    pub(crate) fn converge_prepared(
        &self,
        bundle: &crate::transition::PreparedSourceBundle,
        journal: &Path,
    ) -> Result<(), AppError> {
        self.finish()?;
        crate::transition::validate_committed_bundle_cleanup_authority(bundle, journal)?;
        let prepared = crate::transition::planned_events(bundle)?;
        if prepared.len() != self.planned.len()
            || prepared.iter().zip(self.planned).any(|(left, right)| {
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
        crate::observability::converge_from_events(&events)?;
        validate_prepared_runtime_suffix(&events, &prepared)?;
        validate_derived_outputs_unlocked(&events, &bundle.project_id)?;
        crate::transition::validate_committed_bundle_cleanup_authority(bundle, journal)
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
    crate::state::validated_identity_from_current_state(&contents, &fresh)
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

fn converge_derived_outputs_unlocked(
    events: &[ParsedLedgerEvent],
    project_id: &str,
) -> Result<(), AppError> {
    rebuild_project_ledger_from_events(&paths::project_session_ledger_file(), events, project_id)?;
    rebuild_operation_log_from_events(events)
}

fn validate_derived_outputs_unlocked(
    events: &[ParsedLedgerEvent],
    project_id: &str,
) -> Result<(), AppError> {
    let project_events = events
        .iter()
        .filter(|event| event.project_id == project_id)
        .cloned()
        .collect::<Vec<_>>();
    let (expected_project, expected_head_hash) = render_chained_ledger(&project_events);
    let expected_head = format!(
        "{{\"schema_version\":1,\"event_count\":{},\"last_event_hash\":\"{}\"}}\n",
        project_events.len(),
        expected_head_hash.as_deref().unwrap_or("root")
    );
    let project_path = paths::project_session_ledger_file();
    if fs::read(&project_path).map_err(|err| {
        AppError::blocked(format!("prepared project ledger 재검증 읽기 실패: {err}"))
    })? != expected_project.as_bytes()
        || fs::read(ledger_head_path(&project_path)).map_err(|err| {
            AppError::blocked(format!("prepared project head 재검증 읽기 실패: {err}"))
        })? != expected_head.as_bytes()
    {
        return Err(AppError::blocked(
            "prepared project ledger/head convergence 불일치",
        ));
    }
    let expected_operation_log = events
        .iter()
        .map(|event| {
            LedgerEvent {
                event_id: event.event_id.clone(),
                ts_ms: event.ts_ms,
                event_type: event.event_type.clone(),
                project_id: event.project_id.clone(),
                session_id: event.session_id.clone(),
                summary: event.summary.clone(),
                details: event.details.clone(),
            }
            .to_log_line()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let expected_operation_log = if expected_operation_log.is_empty() {
        expected_operation_log
    } else {
        format!("{expected_operation_log}\n")
    };
    if fs::read(paths::operation_log_file()).map_err(|err| {
        AppError::blocked(format!("prepared operation log 재검증 읽기 실패: {err}"))
    })? != expected_operation_log.as_bytes()
    {
        return Err(AppError::blocked(
            "prepared operation log convergence 불일치",
        ));
    }
    let connection = rusqlite::Connection::open(paths::observability_db_file())
        .map_err(|err| AppError::blocked(format!("prepared sqlite 재검증 열기 실패: {err}")))?;
    let mut statement = connection
        .prepare(
            "SELECT rowid, event_id, ts_ms, event_type, project_id, session_id, summary
               FROM ledger_events
           ORDER BY rowid",
        )
        .map_err(|err| AppError::blocked(format!("prepared sqlite 재검증 준비 실패: {err}")))?;
    let projected = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })
        .map_err(|err| AppError::blocked(format!("prepared sqlite 재검증 query 실패: {err}")))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| AppError::blocked(format!("prepared sqlite 재검증 row 실패: {err}")))?;
    if projected
        != events
            .iter()
            .enumerate()
            .map(|(index, event)| {
                (
                    i64::try_from(index + 1).unwrap_or(i64::MAX),
                    event.event_id.clone(),
                    i64::try_from(event.ts_ms).unwrap_or(i64::MAX),
                    event.event_type.clone(),
                    event.project_id.clone(),
                    event.session_id.clone(),
                    event.summary.clone(),
                )
            })
            .collect::<Vec<_>>()
    {
        return Err(AppError::blocked(
            "prepared sqlite convergence event sequence 불일치",
        ));
    }
    Ok(())
}

fn rebuild_operation_log_from_events(events: &[ParsedLedgerEvent]) -> Result<(), AppError> {
    let body = events
        .iter()
        .map(|event| {
            LedgerEvent {
                event_id: event.event_id.clone(),
                ts_ms: event.ts_ms,
                event_type: event.event_type.clone(),
                project_id: event.project_id.clone(),
                session_id: event.session_id.clone(),
                summary: event.summary.clone(),
                details: event.details.clone(),
            }
            .to_log_line()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let body = if body.is_empty() {
        body
    } else {
        format!("{body}\n")
    };
    crate::state::atomic_replace_bytes(&paths::operation_log_file(), body.as_bytes())
}

fn rebuild_project_ledger_from_events(
    path: &Path,
    events: &[ParsedLedgerEvent],
    project_id: &str,
) -> Result<(), AppError> {
    let events = events
        .iter()
        .filter(|event| event.project_id == project_id)
        .cloned()
        .collect::<Vec<_>>();
    let (body, last_hash) = render_chained_ledger(&events);

    if path.exists() {
        let existing = fs::read_to_string(path).map_err(|err| {
            AppError::blocked(format!("project ledger convergence read 실패: {err}"))
        })?;
        if validate_ledger_contents(path, &existing).is_err() {
            preserve_corrupt_ledger_file(path)?;
            preserve_corrupt_ledger_file(&ledger_head_path(path))?;
        }
    }
    crate::state::atomic_replace_bytes(path, body.as_bytes())?;
    write_ledger_head(path, events.len(), last_hash.as_deref().unwrap_or("root"))
}

fn render_chained_ledger(events: &[ParsedLedgerEvent]) -> (String, Option<String>) {
    let mut body = String::new();
    let mut previous = "root".to_string();
    for event in events {
        let event = LedgerEvent {
            event_id: event.event_id.clone(),
            ts_ms: event.ts_ms,
            event_type: event.event_type.clone(),
            project_id: event.project_id.clone(),
            session_id: event.session_id.clone(),
            summary: event.summary.clone(),
            details: event.details.clone(),
        };
        let payload = event_chain_payload(&event, &previous);
        let event_hash = sha256_bytes(payload.as_bytes());
        body.push_str(&format!(
            "{{{},\"event_hash\":\"{}\"}}\n",
            payload.trim_start_matches('{').trim_end_matches('}'),
            event_hash
        ));
        previous = event_hash;
    }
    let last_hash = (!events.is_empty()).then_some(previous);
    (body, last_hash)
}

fn preserve_corrupt_ledger_file(path: &Path) -> Result<Option<std::path::PathBuf>, AppError> {
    if !path.exists() {
        return Ok(None);
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("ledger");
    let backup = path.with_extension(format!("{extension}.corrupt.{}", now_nanos()));
    fs::rename(path, &backup).map_err(|err| {
        AppError::runtime(format!(
            "손상 ledger 백업 실패: {} -> {} ({err})",
            path.display(),
            backup.display()
        ))
    })?;
    Ok(Some(backup))
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
    let payload = event_chain_payload(event, &previous);
    let event_hash = sha256_bytes(payload.as_bytes());
    let line = format!(
        "{{{},\"event_hash\":\"{}\"}}",
        payload.trim_start_matches('{').trim_end_matches('}'),
        event_hash
    );
    append_line(path, &line)?;
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
    crate::state::atomic_replace_bytes(&ledger_head_path(path), body.as_bytes())
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
    let gap = crate::state::record_validation_gap(
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

pub fn event_detail_exists(event_type: &str, field: &str, value: &str) -> Result<bool, AppError> {
    Ok(read_runtime_events()?.iter().any(|event| {
        event.event_type == event_type && detail_value(&event.details, field) == Some(value)
    }))
}

pub fn event_details_match(event_type: &str, fields: &[(&str, &str)]) -> Result<bool, AppError> {
    Ok(read_runtime_events()?.iter().any(|event| {
        event.event_type == event_type
            && fields
                .iter()
                .all(|(field, value)| detail_value(&event.details, field) == Some(*value))
    }))
}

pub fn workflow_checkpoint_exists(
    workflow_id: &str,
    revision: u64,
    artifact_hash: &str,
) -> Result<bool, AppError> {
    Ok(workflow_checkpoints(workflow_id)?.iter().any(|checkpoint| {
        checkpoint.revision == revision && checkpoint.artifact_hash == artifact_hash
    }))
}

pub fn workflow_checkpoints(workflow_id: &str) -> Result<Vec<WorkflowCheckpoint>, AppError> {
    let mut checkpoints = Vec::new();
    for event in read_runtime_events()? {
        if event.event_type != "workflow.checkpoint"
            || detail_value(&event.details, "workflow_id") != Some(workflow_id)
        {
            continue;
        }
        let revision = detail_value(&event.details, "revision")
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or_else(|| malformed_checkpoint(&event.event_id))?;
        let artifact_hash = detail_value(&event.details, "artifact_hash")
            .filter(|value| is_sha256(value))
            .ok_or_else(|| malformed_checkpoint(&event.event_id))?
            .to_string();
        let previous_hash = detail_value(&event.details, "previous_hash")
            .filter(|value| *value == "none" || is_sha256(value))
            .ok_or_else(|| malformed_checkpoint(&event.event_id))?
            .to_string();
        checkpoints.push(WorkflowCheckpoint {
            revision,
            artifact_hash,
            previous_hash,
        });
    }
    checkpoints.sort_by_key(|checkpoint| checkpoint.revision);
    for (index, checkpoint) in checkpoints.iter().enumerate() {
        let expected_revision = index as u64 + 1;
        let expected_previous = if index == 0 {
            "none"
        } else {
            checkpoints[index - 1].artifact_hash.as_str()
        };
        if checkpoint.revision != expected_revision || checkpoint.previous_hash != expected_previous
        {
            return Err(AppError::blocked(format!(
                "workflow ledger chain 검증 차단\n- workflow id: {workflow_id}\n- revision: {}\n- 이유: latest checkpoint 또는 previous_hash chain 불일치",
                checkpoint.revision
            )));
        }
    }
    Ok(checkpoints)
}

fn detail_value<'a>(details: &'a str, key: &str) -> Option<&'a str> {
    details.split_whitespace().find_map(|field| {
        let (candidate, value) = field.split_once('=')?;
        (candidate == key).then_some(value)
    })
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn malformed_checkpoint(event_id: &str) -> AppError {
    AppError::blocked(format!(
        "workflow ledger checkpoint 검증 차단\n- event id: {event_id}\n- 이유: required checkpoint field가 malformed입니다."
    ))
}

pub fn contains_sensitive_text(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let normalized = lower
        .chars()
        .filter(|character| !matches!(character, '"' | '\'' | '\\'))
        .collect::<String>();
    [
        "api_key",
        "apikey",
        "authorization",
        "password",
        "secret",
        "token",
    ]
    .iter()
    .any(|key| contains_sensitive_assignment(&normalized, key))
        || normalized.split_whitespace().any(|part| part == "bearer")
        || contains_bounded_prefix(&lower, "sk-", 8)
        || contains_bounded_prefix(&lower, "ghp_", 8)
        || contains_bounded_prefix(&lower, "github_pat_", 8)
        || value.contains("-----BEGIN PRIVATE KEY-----")
        || value
            .split(|character: char| !character.is_ascii_alphanumeric())
            .any(|part| {
                part.len() == 20
                    && part.starts_with("AKIA")
                    && part
                        .bytes()
                        .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
            })
}

fn contains_sensitive_assignment(value: &str, key: &str) -> bool {
    value.match_indices(key).any(|(index, _)| {
        if index > 0 && value.as_bytes()[index - 1].is_ascii_alphanumeric() {
            return false;
        }
        let tail = value[index + key.len()..].trim_start();
        let Some(tail) = tail.strip_prefix('=').or_else(|| tail.strip_prefix(':')) else {
            return false;
        };
        tail.trim_start()
            .chars()
            .next()
            .is_some_and(|character| !matches!(character, ',' | '}' | ']'))
    })
}

fn contains_bounded_prefix(value: &str, prefix: &str, minimum_suffix: usize) -> bool {
    value.match_indices(prefix).any(|(index, _)| {
        let boundary = index == 0 || !value.as_bytes()[index - 1].is_ascii_alphanumeric();
        if !boundary {
            return false;
        }
        value[index + prefix.len()..]
            .chars()
            .take_while(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            })
            .count()
            >= minimum_suffix
    })
}

pub fn redact_text(value: &str) -> String {
    let parts = value.split_whitespace().collect::<Vec<_>>();
    parts
        .iter()
        .enumerate()
        .map(|(index, part)| {
            let follows_bearer = index > 0 && parts[index - 1].eq_ignore_ascii_case("bearer");
            if contains_sensitive_text(part)
                || part.eq_ignore_ascii_case("bearer")
                || follows_bearer
            {
                "[REDACTED]".to_string()
            } else {
                (*part).to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn append_line(path: &Path, line: &str) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            AppError::runtime(format!(
                "디렉터리를 만들지 못했습니다: {} ({err})",
                parent.display()
            ))
        })?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| {
            AppError::runtime(format!(
                "파일을 열지 못했습니다: {} ({err})",
                path.display()
            ))
        })?;

    writeln!(file, "{line}").map_err(|err| {
        AppError::runtime(format!(
            "파일에 기록하지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    file.sync_all()
        .map_err(|err| AppError::runtime(format!("ledger sync 실패: {} ({err})", path.display())))
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
mod tests {
    use super::*;

    #[test]
    fn ledger_event_json_round_trip_for_projection_fields() {
        let event = LedgerEvent {
            event_id: "event-1".to_string(),
            ts_ms: 42,
            event_type: "runtime.init".to_string(),
            project_id: "project-a".to_string(),
            session_id: "session-a".to_string(),
            summary: "초기화".to_string(),
            details: "safe".to_string(),
        };

        let parsed = parse_event_line(&event.to_json_line()).unwrap();

        assert_eq!(parsed.event_id, "event-1");
        assert_eq!(parsed.ts_ms, 42);
        assert_eq!(parsed.event_type, "runtime.init");
        assert_eq!(parsed.project_id, "project-a");
        assert_eq!(parsed.session_id, "session-a");
        assert_eq!(parsed.summary, "초기화");
    }

    #[test]
    fn redacts_sensitive_words_before_persistence() {
        let redacted = redact_text("token=abc safe password=hunter2");
        assert_eq!(redacted, "[REDACTED] safe [REDACTED]");
        assert!(contains_sensitive_text("Authorization: Bearer abc123"));
        assert!(contains_sensitive_text("key=sk-12345678"));
        assert!(!contains_sensitive_text("token budget 검증 완료"));
    }

    #[test]
    fn malformed_runtime_ledger_line_fails_closed() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root =
            std::env::temp_dir().join(format!("rpotato-ledger-malformed-{}", std::process::id()));
        std::env::set_var("RPOTATO_DATA_HOME", &root);
        fs::create_dir_all(paths::state_dir()).unwrap();
        fs::write(paths::runtime_ledger_file(), "{partial\n").unwrap();

        let error = read_runtime_events().unwrap_err();

        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);
        assert_eq!(error.code, 3);
        assert!(error.message.contains("malformed JSONL"));
    }

    #[test]
    fn workflow_checkpoint_previous_hash_chain_is_strict() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root =
            std::env::temp_dir().join(format!("rpotato-ledger-chain-{}", std::process::id()));
        std::env::set_var("RPOTATO_DATA_HOME", &root);
        std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        fs::create_dir_all(paths::project_state_dir()).unwrap();
        let identity = fresh_identity();
        let first_hash = "a".repeat(64);
        let second_hash = "b".repeat(64);
        let first = new_event_for(
            &identity,
            "workflow.checkpoint",
            "first",
            &format!(
                "workflow_id=workflow-chain revision=1 artifact_hash={first_hash} previous_hash=none phase=model-pending action_id=action proposal_id=none evidence_id=none"
            ),
        );
        let stale = new_event_for(
            &identity,
            "workflow.checkpoint",
            "stale",
            &format!(
                "workflow_id=workflow-chain revision=2 artifact_hash={second_hash} previous_hash={} phase=approved action_id=action proposal_id=none evidence_id=none",
                "c".repeat(64)
            ),
        );
        append_event(&first).unwrap();
        append_event(&stale).unwrap();

        let error = workflow_checkpoints("workflow-chain").unwrap_err();

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        let _ = fs::remove_dir_all(root);
        assert_eq!(error.code, 3);
        assert!(error.message.contains("previous_hash chain"));
    }

    #[test]
    fn physical_chain_reorder_and_truncation_fail_closed() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        for mode in ["reorder", "truncate"] {
            let root = std::env::temp_dir().join(format!(
                "rpotato-ledger-physical-{mode}-{}",
                std::process::id()
            ));
            std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
            std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
            let identity = fresh_identity();
            append_event(&new_event_for(&identity, "one", "하나", "safe")).unwrap();
            append_event(&new_event_for(&identity, "two", "둘", "safe")).unwrap();
            let path = paths::runtime_ledger_file();
            let body = fs::read_to_string(&path).unwrap();
            let mut lines = body.lines().collect::<Vec<_>>();
            if mode == "reorder" {
                lines.swap(0, 1);
            } else {
                lines.pop();
            }
            fs::write(&path, format!("{}\n", lines.join("\n"))).unwrap();
            assert!(read_runtime_events().is_err(), "mode: {mode}");
            std::env::remove_var("RPOTATO_DATA_HOME");
            std::env::remove_var("RPOTATO_PROJECT_ROOT");
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn runtime_head_repairs_only_the_single_durable_append_gap() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-ledger-head-repair-{}-{}",
            std::process::id(),
            now_nanos()
        ));
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        let identity = fresh_identity();
        let first = new_event_for(&identity, "head.first", "첫 이벤트", "safe");
        append_event(&first).unwrap();
        let path = paths::runtime_ledger_file();
        let first_events = read_runtime_events().unwrap();
        let first_hash = first_events[0].event_hash.clone().unwrap();
        let second = new_event_for(&identity, "head.second", "두 번째 이벤트", "safe");
        let payload = event_chain_payload(&second, &first_hash);
        let second_hash = sha256_bytes(payload.as_bytes());
        let line = format!(
            "{{{},\"event_hash\":\"{}\"}}",
            payload.trim_start_matches('{').trim_end_matches('}'),
            second_hash
        );

        append_line(&path, &line).unwrap();
        let repaired = read_runtime_events().unwrap();
        let head = fs::read_to_string(ledger_head_path(&path)).unwrap();

        assert_eq!(repaired.len(), 2);
        assert!(head.contains("\"event_count\":2"));
        assert!(head.contains(&second_hash));

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn missing_head_is_repaired_only_for_the_first_chained_append() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-ledger-first-head-repair-{}-{}",
            std::process::id(),
            now_nanos()
        ));
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        let identity = fresh_identity();
        append_event(&new_event_for(&identity, "head.first", "첫 이벤트", "safe")).unwrap();
        let path = paths::runtime_ledger_file();
        fs::remove_file(ledger_head_path(&path)).unwrap();

        let repaired = read_runtime_events().unwrap();

        assert_eq!(repaired.len(), 1);
        assert!(ledger_head_path(&path).is_file());

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn orphan_runtime_head_without_jsonl_fails_closed() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-ledger-orphan-head-{}-{}",
            std::process::id(),
            now_nanos()
        ));
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        let path = paths::runtime_ledger_file();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        write_ledger_head(&path, 0, "root").unwrap();

        let error = read_runtime_events().unwrap_err();

        assert!(error.message.contains("orphan head"));
        assert!(!path.exists());
        assert!(ledger_head_path(&path).exists());
        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn corrupt_project_mirror_is_preserved_and_rebuilt_from_runtime() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let identity = fresh_identity();
        let first = new_event_for(&identity, "mirror.first", "첫 이벤트", "safe");
        let second = new_event_for(&identity, "mirror.second", "두 번째 이벤트", "safe");
        append_event(&first).unwrap();

        let project_path = paths::project_session_ledger_file();
        let head_path = ledger_head_path(&project_path);
        fs::write(&project_path, "{malformed\n").unwrap();
        fs::write(&head_path, "{stale-head}\n").unwrap();

        append_event(&second).unwrap();

        let body = fs::read_to_string(&project_path).unwrap();
        let rebuilt = validate_ledger_contents(&project_path, &body).unwrap();
        let backups = fs::read_dir(paths::project_state_dir())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".corrupt."))
            .collect::<Vec<_>>();
        let runtime = read_runtime_events().unwrap();

        assert_eq!(runtime.len(), 2);
        assert_eq!(rebuilt.len(), 2);
        assert_eq!(rebuilt[0].event_id, first.event_id);
        assert_eq!(rebuilt[1].event_id, second.event_id);
        assert_eq!(backups.len(), 2);
    }

    #[test]
    fn concurrent_writers_preserve_both_ledger_chains() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-ledger-concurrent-{}-{}",
            std::process::id(),
            now_nanos()
        ));
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        let identity = fresh_identity();
        let writers = 12;
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(writers));
        let handles = (0..writers)
            .map(|index| {
                let barrier = barrier.clone();
                let identity = identity.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    append_event(&new_event_for(
                        &identity,
                        "concurrent.write",
                        &format!("writer {index}"),
                        "safe",
                    ))
                })
            })
            .collect::<Vec<_>>();

        for handle in handles {
            handle.join().unwrap().unwrap();
        }
        let runtime_events = read_runtime_events().unwrap();
        let project_path = paths::project_session_ledger_file();
        let project_contents = fs::read_to_string(&project_path).unwrap();
        let project_events = validate_ledger_contents(&project_path, &project_contents).unwrap();
        let operation_log = fs::read_to_string(paths::operation_log_file()).unwrap();

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        let _ = fs::remove_dir_all(root);
        assert_eq!(runtime_events.len(), writers);
        assert_eq!(project_events.len(), writers);
        assert_eq!(operation_log.lines().count(), writers);
    }

    #[test]
    fn event_sink_single_acquisition_concurrency_matrix() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-ledger-event-sink-{}-{}",
            std::process::id(),
            now_nanos()
        ));
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        let identity = fresh_identity();
        let events = [
            new_event_for(&identity, "sink.first", "첫 이벤트", "index=0"),
            new_event_for(&identity, "sink.second", "두 번째 이벤트", "index=1"),
        ];
        let writer = LedgerWriterGuard::acquire().unwrap();
        let planned = writer.plan_events(&events).unwrap();
        let mut sink = writer.event_sink(&planned);
        let concurrent_identity = identity.clone();
        let (ready_sender, ready_receiver) = std::sync::mpsc::channel();
        let (sender, receiver) = std::sync::mpsc::channel();
        let concurrent = std::thread::spawn(move || {
            let event = new_event_for(
                &concurrent_identity,
                "sink.concurrent",
                "경쟁 이벤트",
                "index=2",
            );
            let result = LedgerWriterGuard::acquire_after_first_block(|| {
                ready_sender.send(()).unwrap();
            })
            .and_then(|writer| writer.append_planned(&event).map(|_| ()));
            sender.send(result).unwrap();
        });
        ready_receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("contender가 held lease에서 실제로 차단되어야 합니다.");
        assert!(receiver.recv_timeout(Duration::from_millis(100)).is_err());

        assert!(sink.append_planned_under_guard(1, &events[1]).is_err());
        sink.append_planned_under_guard(0, &events[0]).unwrap();
        assert!(sink.append_planned_under_guard(1, &events[0]).is_err());
        sink.append_planned_under_guard(1, &events[1]).unwrap();
        sink.finish().unwrap();
        sink.converge_derived(&identity.project_id).unwrap();
        drop(writer);
        receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        concurrent.join().unwrap();

        let runtime = read_runtime_events().unwrap();
        assert_eq!(runtime.len(), 3);
        assert_eq!(runtime[0].event_id, events[0].event_id);
        assert_eq!(runtime[1].event_id, events[1].event_id);
        assert_eq!(runtime[2].event_type, "sink.concurrent");
        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn event_sink_crash_recovery_never_nests_ledger_lease() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-ledger-event-sink-restart-{}-{}",
            std::process::id(),
            now_nanos()
        ));
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        let identity = fresh_identity();
        let first = new_event_for(&identity, "sink.restart.first", "첫 이벤트", "index=0");
        let second = new_event_for(&identity, "sink.restart.second", "둘째 이벤트", "index=1");
        {
            let writer = LedgerWriterGuard::acquire().unwrap();
            let planned = writer.plan_events(std::slice::from_ref(&first)).unwrap();
            let mut sink = writer.event_sink(&planned);
            sink.append_planned_under_guard(0, &first).unwrap();
        }
        {
            let writer = LedgerWriterGuard::acquire().unwrap();
            let planned = writer.plan_events(std::slice::from_ref(&second)).unwrap();
            let mut sink = writer.event_sink(&planned);
            sink.append_planned_under_guard(0, &second).unwrap();
            sink.finish().unwrap();
            sink.converge_derived(&identity.project_id).unwrap();
        }
        let events = read_runtime_events().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_id, first.event_id);
        assert_eq!(events[1].event_id, second.event_id);
        let source = include_str!("ledger.rs")
            .split("impl EventSink<'_> {")
            .nth(1)
            .unwrap()
            .split("fn validate_prepared_runtime_suffix")
            .next()
            .unwrap();
        assert!(!source.contains("LedgerWriterGuard::acquire"));
        assert!(!source.contains("RecoverableLease::acquire"));
        assert!(!source.contains("append_event("));
        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn t10_rebuilds_all_derived_outputs_from_runtime_authority() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-ledger-t10-convergence-{}-{}",
            std::process::id(),
            now_nanos()
        ));
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        fs::create_dir_all(paths::project_root()).unwrap();
        let identity = fresh_identity();
        let first = new_event_for(&identity, "t10.first", "첫 이벤트", "safe=one");
        let second = new_event_for(&identity, "t10.second", "두 번째 이벤트", "safe=two");
        append_event(&first).unwrap();
        append_event(&second).unwrap();
        crate::observability::converge_from_events(&read_runtime_events().unwrap()).unwrap();

        let project_path = paths::project_session_ledger_file();
        {
            let connection = rusqlite::Connection::open(paths::observability_db_file()).unwrap();
            connection
                .execute(
                    "UPDATE ledger_events SET summary = 'tampered-same-id' WHERE event_id = ?1",
                    rusqlite::params![first.event_id],
                )
                .unwrap();
        }
        assert!(validate_derived_outputs_unlocked(
            &read_runtime_events().unwrap(),
            &identity.project_id
        )
        .unwrap_err()
        .message
        .contains("sqlite convergence event sequence"));
        fs::write(&project_path, b"{corrupt-project-ledger\n").unwrap();
        fs::write(ledger_head_path(&project_path), b"{corrupt-head}\n").unwrap();
        fs::write(paths::operation_log_file(), b"stale extra operation\n").unwrap();

        let writer = LedgerWriterGuard::acquire().unwrap();
        writer.converge_derived(&identity.project_id).unwrap();
        let runtime = writer.events().unwrap();
        crate::observability::converge_from_events(&runtime).unwrap();
        drop(writer);

        let project_events = runtime
            .iter()
            .filter(|event| event.project_id == identity.project_id)
            .cloned()
            .collect::<Vec<_>>();
        let (expected_project, expected_head_hash) = render_chained_ledger(&project_events);
        let expected_head = format!(
            "{{\"schema_version\":1,\"event_count\":{},\"last_event_hash\":\"{}\"}}\n",
            project_events.len(),
            expected_head_hash.as_deref().unwrap_or("root")
        );
        let expected_operation_log = runtime
            .iter()
            .map(|event| {
                format!(
                    "{} {} {} {}\n",
                    event.ts_ms, event.event_type, event.session_id, event.summary
                )
            })
            .collect::<String>();

        assert_eq!(fs::read_to_string(&project_path).unwrap(), expected_project);
        assert_eq!(
            fs::read_to_string(ledger_head_path(&project_path)).unwrap(),
            expected_head
        );
        assert_eq!(
            fs::read_to_string(paths::operation_log_file()).unwrap(),
            expected_operation_log
        );
        let projected_rows = {
            let connection = rusqlite::Connection::open(paths::observability_db_file()).unwrap();
            let mut statement = connection
                .prepare(
                    "SELECT rowid, event_id, ts_ms, event_type, project_id, session_id, summary
                       FROM ledger_events
                   ORDER BY rowid",
                )
                .unwrap();
            statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                    ))
                })
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert_eq!(
            projected_rows,
            runtime
                .iter()
                .enumerate()
                .map(|(index, event)| (
                    i64::try_from(index + 1).unwrap(),
                    event.event_id.clone(),
                    i64::try_from(event.ts_ms).unwrap(),
                    event.event_type.clone(),
                    event.project_id.clone(),
                    event.session_id.clone(),
                    event.summary.clone(),
                ))
                .collect::<Vec<_>>()
        );

        let before_restart = (
            fs::read(&project_path).unwrap(),
            fs::read(ledger_head_path(&project_path)).unwrap(),
            fs::read(paths::operation_log_file()).unwrap(),
        );
        let writer = LedgerWriterGuard::acquire().unwrap();
        writer.converge_derived(&identity.project_id).unwrap();
        crate::observability::converge_from_events(&writer.events().unwrap()).unwrap();
        drop(writer);
        assert_eq!(
            before_restart,
            (
                fs::read(&project_path).unwrap(),
                fs::read(ledger_head_path(&project_path)).unwrap(),
                fs::read(paths::operation_log_file()).unwrap(),
            )
        );

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        let _ = fs::remove_dir_all(root);
    }
}
