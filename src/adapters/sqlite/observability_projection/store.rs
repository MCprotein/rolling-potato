use super::*;

pub(super) fn open_or_recover() -> Result<(Connection, Option<PathBuf>), AppError> {
    let path = paths::observability_db_file();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            AppError::runtime(format!(
                "observability 디렉터리를 만들지 못했습니다: {} ({err})",
                parent.display()
            ))
        })?;
    }

    match Connection::open(&path) {
        Ok(connection) => match migrate(&connection) {
            Ok(()) => Ok((connection, None)),
            Err(_err) if path.exists() => {
                drop(connection);
                let recovered = recover_corrupt_db(&path)?;
                let connection = Connection::open(&path)
                    .map_err(sql_error("복구 후 observability DB를 열지 못했습니다"))?;
                migrate(&connection)?;
                Ok((connection, Some(recovered)))
            }
            Err(err) => Err(err),
        },
        Err(_err) if path.exists() => {
            let recovered = recover_corrupt_db(&path)?;
            let connection = Connection::open(&path)
                .map_err(sql_error("복구 후 observability DB를 열지 못했습니다"))?;
            migrate(&connection)?;
            Ok((connection, Some(recovered)))
        }
        Err(err) => Err(AppError::runtime(format!(
            "observability DB를 열지 못했습니다: {} ({err})",
            path.display()
        ))),
    }
}

pub(super) fn status_from_connection(
    connection: &Connection,
    recovered_from: Option<PathBuf>,
) -> Result<StoreStatus, AppError> {
    Ok(StoreStatus {
        path: paths::observability_db_file(),
        recovered_from,
        migration_version: count_scalar(
            connection,
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        )?,
        ledger_events: count_scalar(connection, "SELECT COUNT(*) FROM ledger_events")?,
        sessions: count_scalar(connection, "SELECT COUNT(*) FROM sessions")?,
        workflows: count_scalar(connection, "SELECT COUNT(*) FROM workflows")?,
        transcript_records: count_scalar(connection, "SELECT COUNT(*) FROM transcript_records")?,
        model_runs: count_scalar(connection, "SELECT COUNT(*) FROM model_runs")?,
        token_records: count_scalar(connection, "SELECT COUNT(*) FROM token_usage")?,
        resource_samples: count_scalar(connection, "SELECT COUNT(*) FROM resource_samples")?,
        benchmark_runs: count_scalar(connection, "SELECT COUNT(*) FROM benchmark_runs")?,
        evidence_records: count_scalar(connection, "SELECT COUNT(*) FROM evidence_records")?,
        stop_gate_results: count_scalar(connection, "SELECT COUNT(*) FROM stop_gate_results")?,
    })
}

pub(super) fn count_scalar(connection: &Connection, sql: &str) -> Result<i64, AppError> {
    connection
        .query_row(sql, [], |row| row.get(0))
        .map_err(sql_error("observability count query를 실행하지 못했습니다"))
}

pub(super) fn count_before(
    connection: &Connection,
    table: &str,
    column: &str,
    cutoff_ms: i64,
) -> Result<i64, AppError> {
    let sql = format!("SELECT COUNT(*) FROM {table} WHERE {column} < ?1");
    connection
        .query_row(&sql, params![cutoff_ms], |row| row.get(0))
        .map_err(sql_error(
            "monitor prune dry-run count를 실행하지 못했습니다",
        ))
}

fn recover_corrupt_db(path: &std::path::Path) -> Result<PathBuf, AppError> {
    let recovered = path.with_extension(format!("sqlite.corrupt.{}", now_ms()));
    fs::rename(path, &recovered).map_err(|err| {
        AppError::runtime(format!(
            "손상된 observability DB를 보존 이동하지 못했습니다: {} -> {} ({err})",
            path.display(),
            recovered.display()
        ))
    })?;
    Ok(recovered)
}

pub(super) fn sql_error(context: &'static str) -> impl FnOnce(rusqlite::Error) -> AppError {
    move |err| AppError::runtime(format!("{context}: {err}"))
}

pub(super) fn to_i64(value: u128) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

pub(super) fn i64_to_u128(value: i64) -> u128 {
    u128::try_from(value).unwrap_or_default()
}

pub(super) fn i64_to_u32(value: i64) -> u32 {
    u32::try_from(value).unwrap_or_default()
}

pub(super) fn option_i64_to_u32(value: Option<i64>) -> Option<u32> {
    value.and_then(|value| u32::try_from(value).ok())
}

pub(super) fn option_i64_to_bool(value: Option<i64>) -> Option<bool> {
    value.map(|value| value != 0)
}

pub(super) fn option_i64_to_u64(value: Option<i64>) -> Option<u64> {
    value.and_then(|value| u64::try_from(value).ok())
}

pub(super) fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}
