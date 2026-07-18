use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use rusqlite::{params, Connection};

use super::{now_ms, sql_error, to_i64};
use crate::foundation::error::AppError;
use crate::runtime_core::observability::facade::CanonicalProjectionReadPort;
use crate::runtime_core::workflow::storage_compat::ledger::{
    LedgerEvent, ParsedLedgerEvent, RuntimeIdentity,
};

pub(super) fn record_session(
    connection: &Connection,
    identity: &RuntimeIdentity,
) -> Result<(), AppError> {
    connection
        .execute(
            "INSERT OR IGNORE INTO sessions (
                session_id,
                project_id,
                project_root,
                started_at_ms,
                parent_session_id,
                branch_from_event_id,
                compacted_summary_path
             ) VALUES (?1, ?2, ?3, ?4, NULL, NULL, NULL)",
            params![
                identity.session_id,
                identity.project_id,
                identity.project_root,
                to_i64(now_ms())
            ],
        )
        .map_err(sql_error("session record를 저장하지 못했습니다"))?;
    Ok(())
}

pub(super) fn replay_ledger_events(
    connection: &Connection,
    events: &[ParsedLedgerEvent],
    ledger: &dyn CanonicalProjectionReadPort,
) -> Result<(), AppError> {
    let transaction = connection
        .unchecked_transaction()
        .map_err(sql_error("ledger replay transaction을 시작하지 못했습니다"))?;
    transaction
        .execute("DELETE FROM ledger_events", [])
        .map_err(sql_error("ledger replay projection 초기화에 실패했습니다"))?;
    transaction
        .execute("DELETE FROM transcript_records", [])
        .map_err(sql_error(
            "transcript replay projection 초기화에 실패했습니다",
        ))?;
    sqlite_replay_fault("after-clear")?;
    sqlite_replay_pause("after-clear")?;
    for (index, event) in events.iter().enumerate() {
        transaction
            .execute(
                "INSERT OR IGNORE INTO ledger_events (
                    event_id, ts_ms, event_type, project_id, session_id, summary
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    event.event_id,
                    to_i64(event.ts_ms),
                    event.event_type,
                    event.project_id,
                    event.session_id,
                    event.summary
                ],
            )
            .map_err(sql_error("ledger replay projection을 저장하지 못했습니다"))?;
        project_workflow_checkpoint(
            &transaction,
            &event.event_type,
            &event.details,
            &event.session_id,
            event.ts_ms,
        )?;
        project_patch_evidence_event(
            &transaction,
            &event.event_type,
            &event.details,
            &event.session_id,
            event.ts_ms,
        )?;
        crate::adapters::sqlite::transcript_projection::project_event(
            &transaction,
            crate::adapters::sqlite::transcript_projection::TranscriptProjectionEvent {
                project_id: &event.project_id,
                session_id: &event.session_id,
                event_type: &event.event_type,
                details: &event.details,
                ledger_event_id: &event.event_id,
                event_ordinal: to_i64((index + 1) as u128),
            },
            ledger,
        )?;
        if index == 0 {
            sqlite_replay_fault("after-first-event")?;
        }
    }
    transaction
        .commit()
        .map_err(sql_error("ledger replay transaction commit에 실패했습니다"))
}

fn sqlite_replay_fault(point: &str) -> Result<(), AppError> {
    if cfg!(debug_assertions)
        && std::env::var("RPOTATO_TEST_SQLITE_REPLAY_FAULT").as_deref() == Ok(point)
    {
        return Err(AppError::runtime(format!(
            "injected sqlite replay fault: {point}"
        )));
    }
    Ok(())
}

fn sqlite_replay_pause(point: &str) -> Result<(), AppError> {
    if !cfg!(debug_assertions) {
        return Ok(());
    }
    let Ok(root) = std::env::var("RPOTATO_TEST_SQLITE_REPLAY_PAUSE_DIR") else {
        return Ok(());
    };
    let root = PathBuf::from(root);
    fs::create_dir_all(&root)
        .map_err(|err| AppError::runtime(format!("sqlite replay pause dir 생성 실패: {err}")))?;
    let entered = root.join(format!("{point}.entered"));
    let release = root.join(format!("{point}.release"));
    fs::write(&entered, b"entered")
        .map_err(|err| AppError::runtime(format!("sqlite replay pause marker 실패: {err}")))?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while !release.exists() {
        if Instant::now() >= deadline {
            return Err(AppError::runtime(format!(
                "sqlite replay pause release timeout: {point}"
            )));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}

pub(super) fn project_sessions_from_events(
    connection: &Connection,
    identity: &RuntimeIdentity,
) -> Result<(), AppError> {
    connection
        .execute(
            "DELETE FROM sessions
              WHERE project_id = ?1
                AND session_id NOT IN (
                    SELECT session_id
                      FROM ledger_events
                     WHERE project_id = ?1
                )",
            params![identity.project_id],
        )
        .map_err(sql_error(
            "canonical ledger에 없는 session projection 제거에 실패했습니다",
        ))?;
    connection
        .execute(
            "INSERT OR IGNORE INTO sessions (
                session_id,
                project_id,
                project_root,
                started_at_ms,
                parent_session_id,
                branch_from_event_id,
                compacted_summary_path
             )
             SELECT
                ledger_events.session_id,
                ledger_events.project_id,
                ?2,
                MIN(ledger_events.ts_ms),
                NULL,
                NULL,
                NULL
               FROM ledger_events
              WHERE ledger_events.project_id = ?1
           GROUP BY ledger_events.session_id,
                    ledger_events.project_id",
            params![identity.project_id, identity.project_root],
        )
        .map_err(sql_error("ledger session projection을 복원하지 못했습니다"))?;
    Ok(())
}

pub(super) fn insert_ledger_event(
    connection: &Connection,
    event: &LedgerEvent,
    event_ordinal: i64,
    ledger: &dyn CanonicalProjectionReadPort,
) -> Result<(), AppError> {
    connection
        .execute(
            "INSERT OR IGNORE INTO ledger_events (
                event_id, ts_ms, event_type, project_id, session_id, summary
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                event.event_id,
                to_i64(event.ts_ms),
                event.event_type,
                event.project_id,
                event.session_id,
                event.summary
            ],
        )
        .map_err(sql_error("ledger event projection을 저장하지 못했습니다"))?;
    project_workflow_checkpoint(
        connection,
        &event.event_type,
        &event.details,
        &event.session_id,
        event.ts_ms,
    )?;
    project_patch_evidence_event(
        connection,
        &event.event_type,
        &event.details,
        &event.session_id,
        event.ts_ms,
    )?;
    crate::adapters::sqlite::transcript_projection::project_event(
        connection,
        crate::adapters::sqlite::transcript_projection::TranscriptProjectionEvent {
            project_id: &event.project_id,
            session_id: &event.session_id,
            event_type: &event.event_type,
            details: &event.details,
            ledger_event_id: &event.event_id,
            event_ordinal,
        },
        ledger,
    )
}

fn project_patch_evidence_event(
    connection: &Connection,
    event_type: &str,
    details: &str,
    session_id: &str,
    ts_ms: u128,
) -> Result<(), AppError> {
    let field = |key: &str| {
        details.split_whitespace().find_map(|item| {
            item.split_once('=')
                .and_then(|(candidate, value)| (candidate == key).then_some(value))
        })
    };
    if event_type == "verification.evidence.recorded" {
        let Some(evidence_id) = field("evidence_id") else {
            return Ok(());
        };
        connection.execute(
            "INSERT OR REPLACE INTO evidence_records (evidence_id, session_id, workflow_id, evidence_type, artifact_pointer, artifact_hash, stale_after_ms, recorded_at_ms) VALUES (?1, ?2, ?3, 'patch-verification', ?4, ?5, NULL, ?6)",
            params![evidence_id, session_id, field("workflow_id"), format!(".rpotato/evidence/{evidence_id}.json"), field("artifact_hash"), to_i64(ts_ms)],
        ).map_err(sql_error("patch evidence projection 저장 실패"))?;
    }
    if matches!(
        event_type,
        "workflow.stop_gate.passed" | "workflow.stop_gate.failed"
    ) {
        let workflow_id = field("workflow_id").unwrap_or("unknown");
        let passed = i64::from(event_type.ends_with("passed"));
        connection.execute(
            "INSERT OR REPLACE INTO stop_gate_results (stop_gate_result_id, session_id, workflow_id, passed, missing_evidence_count, recorded_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![format!("stop-gate-{workflow_id}"), session_id, workflow_id, passed, i64::from(passed == 0), to_i64(ts_ms)],
        ).map_err(sql_error("stop gate projection 저장 실패"))?;
    }
    Ok(())
}

pub(super) fn project_workflow_checkpoint(
    connection: &Connection,
    event_type: &str,
    details: &str,
    session_id: &str,
    ts_ms: u128,
) -> Result<(), AppError> {
    if event_type != "workflow.checkpoint" {
        return Ok(());
    }
    let Some(workflow_id) = detail_value(details, "workflow_id") else {
        return Ok(());
    };
    let state = detail_value(details, "phase").unwrap_or("unknown");
    let active_skill_id = detail_value(details, "active_skill_id");
    connection
        .execute(
            "INSERT INTO workflows (workflow_id, session_id, state, active_skill_id, updated_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(workflow_id) DO UPDATE SET
                session_id=excluded.session_id,
                state=excluded.state,
                active_skill_id=excluded.active_skill_id,
                updated_at_ms=excluded.updated_at_ms",
            params![
                workflow_id,
                session_id,
                state,
                active_skill_id,
                to_i64(ts_ms)
            ],
        )
        .map_err(sql_error(
            "workflow checkpoint projection을 저장하지 못했습니다",
        ))?;
    Ok(())
}

fn detail_value<'a>(details: &'a str, key: &str) -> Option<&'a str> {
    let prefix = format!("{key}=");
    details
        .split_whitespace()
        .find_map(|part| part.strip_prefix(&prefix))
}
