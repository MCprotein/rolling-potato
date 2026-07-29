use super::*;

pub fn status_read_only() -> Result<StoreStatus, AppError> {
    let connection = open_read_only()?;
    status_from_connection(&connection, None)
}

pub fn monitor_snapshot_read_only(limit: usize) -> Result<MonitorProjectionSnapshot, AppError> {
    let connection = open_read_only()?;
    Ok(MonitorProjectionSnapshot {
        status: status_from_connection(&connection, None)?,
        model_summaries: model_summaries_from_connection(&connection, limit)?,
    })
}

pub fn latest_model_run_for_session_read_only(
    session_id: &str,
) -> Result<Option<LatestModelRunSnapshot>, AppError> {
    let connection = open_read_only()?;
    latest_model_run_for_session_from_connection(&connection, session_id)
}

pub fn export_jsonl() -> Result<String, AppError> {
    let path = paths::runtime_ledger_file();
    if !path.exists() {
        return Ok(String::new());
    }

    fs::read_to_string(&path).map_err(|err| {
        AppError::runtime(format!(
            "monitor JSONL export를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })
}

pub fn export_csv(ledger: &dyn CanonicalProjectionReadPort) -> Result<String, AppError> {
    let (connection, _) = open_or_recover()?;
    replay_ledger_events(&connection, &ledger.read_events()?, ledger)?;

    let mut statement = connection
        .prepare(
            "SELECT event_id, ts_ms, event_type, project_id, session_id, summary
               FROM ledger_events
              ORDER BY ts_ms ASC, event_id ASC",
        )
        .map_err(sql_error("CSV export query를 준비하지 못했습니다"))?;

    let rows = statement
        .query_map([], |row| {
            Ok(vec![
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?.to_string(),
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ])
        })
        .map_err(sql_error("CSV export query를 실행하지 못했습니다"))?;

    let mut csv = String::from("event_id,ts_ms,event_type,project_id,session_id,summary\n");
    for row in rows {
        let row = row.map_err(sql_error("CSV export 결과를 읽지 못했습니다"))?;
        csv.push_str(
            &row.iter()
                .map(|value| csv_cell(value))
                .collect::<Vec<_>>()
                .join(","),
        );
        csv.push('\n');
    }

    Ok(csv)
}

pub fn prune_preview(before_days: u64) -> Result<PrunePreview, AppError> {
    let cutoff_ms = now_ms().saturating_sub((before_days as u128) * 24 * 60 * 60 * 1000);
    let cutoff = to_i64(cutoff_ms);
    let (connection, _) = open_or_recover()?;

    Ok(PrunePreview {
        cutoff_ms,
        ledger_rows: count_before(&connection, "ledger_events", "ts_ms", cutoff)?,
        model_run_rows: count_before(&connection, "model_runs", "started_at_ms", cutoff)?,
        command_run_rows: count_before(&connection, "command_runs", "started_at_ms", cutoff)?,
        resource_sample_rows: count_before(
            &connection,
            "resource_samples",
            "recorded_at_ms",
            cutoff,
        )?,
    })
}

pub(super) fn csv_cell(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}
