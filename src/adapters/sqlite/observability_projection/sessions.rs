use super::*;

pub fn session_history(
    identity: &RuntimeIdentity,
    ledger: &dyn CanonicalProjectionReadPort,
    limit: usize,
) -> Result<Vec<SessionHistoryEntry>, AppError> {
    let (connection, _) = open_or_recover()?;
    replay_ledger_events(&connection, &ledger.read_events()?, ledger)?;
    project_sessions_from_events(&connection, identity)?;
    query_session_history(&connection, &identity.project_id, limit)
}

pub fn session_entry(
    identity: &RuntimeIdentity,
    ledger: &dyn CanonicalProjectionReadPort,
    session_id: &str,
) -> Result<Option<SessionHistoryEntry>, AppError> {
    let (connection, _) = open_or_recover()?;
    replay_ledger_events(&connection, &ledger.read_events()?, ledger)?;
    project_sessions_from_events(&connection, identity)?;
    let entries = query_session_history(&connection, &identity.project_id, usize::MAX)?;
    Ok(entries
        .into_iter()
        .find(|entry| entry.session_id == session_id))
}

pub fn session_events(
    identity: &RuntimeIdentity,
    ledger: &dyn CanonicalProjectionReadPort,
    session_id: &str,
    limit: usize,
) -> Result<Vec<SessionEventEntry>, AppError> {
    let (connection, _) = open_or_recover()?;
    replay_ledger_events(&connection, &ledger.read_events()?, ledger)?;
    project_sessions_from_events(&connection, identity)?;
    query_session_events(&connection, &identity.project_id, session_id, limit)
}

fn query_session_history(
    connection: &Connection,
    project_id: &str,
    limit: usize,
) -> Result<Vec<SessionHistoryEntry>, AppError> {
    let sql = "
        SELECT
            sessions.session_id,
            sessions.project_id,
            sessions.project_root,
            sessions.started_at_ms,
            COUNT(ledger_events.event_id) AS event_count,
            MAX(ledger_events.ts_ms) AS last_event_at_ms,
            (
                SELECT latest.summary
                  FROM ledger_events latest
                 WHERE latest.session_id = sessions.session_id
                 ORDER BY latest.ts_ms DESC, latest.event_id DESC
                 LIMIT 1
            ) AS last_summary
          FROM sessions
     LEFT JOIN ledger_events
            ON ledger_events.session_id = sessions.session_id
         WHERE sessions.project_id = ?1
      GROUP BY sessions.session_id,
               sessions.project_id,
               sessions.project_root,
               sessions.started_at_ms
      ORDER BY COALESCE(MAX(ledger_events.ts_ms), sessions.started_at_ms) DESC,
               sessions.started_at_ms DESC
         LIMIT ?2";

    let mut statement = connection
        .prepare(sql)
        .map_err(sql_error("session history query를 준비하지 못했습니다"))?;
    let rows = statement
        .query_map(
            params![project_id, i64::try_from(limit).unwrap_or(i64::MAX)],
            |row| {
                Ok(SessionHistoryEntry {
                    session_id: row.get(0)?,
                    project_id: row.get(1)?,
                    project_root: row.get(2)?,
                    started_at_ms: row.get(3)?,
                    event_count: row.get(4)?,
                    last_event_at_ms: row.get(5)?,
                    last_summary: row.get(6)?,
                })
            },
        )
        .map_err(sql_error("session history query를 실행하지 못했습니다"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(sql_error("session history 결과를 읽지 못했습니다"))
}

fn query_session_events(
    connection: &Connection,
    project_id: &str,
    session_id: &str,
    limit: usize,
) -> Result<Vec<SessionEventEntry>, AppError> {
    let mut statement = connection
        .prepare(
            "
        SELECT event_id,
               ts_ms,
               event_type,
               summary
          FROM ledger_events
         WHERE project_id = ?1
           AND session_id = ?2
      ORDER BY ts_ms ASC,
               event_id ASC
         LIMIT ?3",
        )
        .map_err(sql_error("session event query를 준비하지 못했습니다"))?;
    let rows = statement
        .query_map(
            params![
                project_id,
                session_id,
                i64::try_from(limit).unwrap_or(i64::MAX)
            ],
            |row| {
                Ok(SessionEventEntry {
                    event_id: row.get(0)?,
                    ts_ms: row.get(1)?,
                    event_type: row.get(2)?,
                    summary: row.get(3)?,
                })
            },
        )
        .map_err(sql_error("session event query를 실행하지 못했습니다"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(sql_error("session event 결과를 읽지 못했습니다"))
}
