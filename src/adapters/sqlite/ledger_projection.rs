//! SQLite validation for the canonical runtime-ledger projection.

use rusqlite::Connection;

use crate::adapters::filesystem::layout as paths;
use crate::foundation::error::AppError;
use crate::runtime_core::workflow::storage_compat::ledger::ParsedLedgerEvent;

pub(crate) fn validate_event_sequence(events: &[ParsedLedgerEvent]) -> Result<(), AppError> {
    let connection = Connection::open(paths::observability_db_file())
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
    let expected = events
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
        .collect::<Vec<_>>();
    if projected != expected {
        return Err(AppError::blocked(
            "prepared sqlite convergence event sequence 불일치",
        ));
    }
    Ok(())
}
