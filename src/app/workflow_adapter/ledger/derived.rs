use std::fs;
use std::path::Path;

use crate::adapters::filesystem::layout as paths;
use crate::foundation::error::AppError;
use crate::runtime_core::workflow::storage_compat::ledger::{
    event_chain_payload, sha256_bytes, LedgerEvent, ParsedLedgerEvent,
};

use super::{ledger_head_path, now_nanos, validate_ledger_contents, write_ledger_head};

pub(super) fn converge_derived_outputs_unlocked(
    events: &[ParsedLedgerEvent],
    project_id: &str,
) -> Result<(), AppError> {
    rebuild_project_ledger_from_events(&paths::project_session_ledger_file(), events, project_id)?;
    rebuild_operation_log_from_events(events)
}

pub(super) fn validate_derived_outputs_unlocked(
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
    crate::adapters::sqlite::ledger_projection::validate_event_sequence(events)
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
    crate::app::workflow_adapter::state::atomic_replace_bytes(
        &paths::operation_log_file(),
        body.as_bytes(),
    )
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
    crate::app::workflow_adapter::state::atomic_replace_bytes(path, body.as_bytes())?;
    write_ledger_head(path, events.len(), last_hash.as_deref().unwrap_or("root"))
}

pub(super) fn render_chained_ledger(events: &[ParsedLedgerEvent]) -> (String, Option<String>) {
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
