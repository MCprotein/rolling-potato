//! Session-scoped latest model-run projection query.

use super::*;

pub(in crate::adapters::sqlite::observability_projection) fn latest_model_run_for_session_from_connection(
    connection: &Connection,
    session_id: &str,
) -> Result<Option<LatestModelRunSnapshot>, AppError> {
    let mut statement = connection
        .prepare(
            "SELECT model_runs.model_id,
                    model_runs.context_limit_tokens,
                    token_usage.context_tokens_used,
                    token_usage.total_tokens,
                    model_runs.started_at_ms
               FROM model_runs
          LEFT JOIN token_usage
                 ON token_usage.model_run_id = model_runs.model_run_id
             WHERE model_runs.session_id = ?1
              ORDER BY model_runs.started_at_ms DESC,
                       model_runs.model_run_id DESC
                 LIMIT 1",
        )
        .map_err(sql_error("latest model run query 준비 실패"))?;
    let mut rows = statement
        .query(params![session_id])
        .map_err(sql_error("latest model run query 실행 실패"))?;
    let Some(row) = rows
        .next()
        .map_err(sql_error("latest model run row 읽기 실패"))?
    else {
        return Ok(None);
    };
    let context_limit = row
        .get::<_, Option<i64>>(1)
        .map_err(sql_error("latest model run context limit 읽기 실패"))?;
    let context_used = row
        .get::<_, Option<i64>>(2)
        .map_err(sql_error("latest model run context usage 읽기 실패"))?;
    let total_tokens = row
        .get::<_, Option<i64>>(3)
        .map_err(sql_error("latest model run token usage 읽기 실패"))?;
    let started_at_ms = row
        .get::<_, i64>(4)
        .map_err(sql_error("latest model run timestamp 읽기 실패"))?;
    Ok(Some(LatestModelRunSnapshot {
        model_id: row
            .get(0)
            .map_err(sql_error("latest model run model id 읽기 실패"))?,
        context_limit_tokens: checked_optional_u32(context_limit, "context limit")?,
        context_tokens_used: checked_optional_u32(context_used, "context usage")?,
        total_tokens: checked_optional_u32(total_tokens, "total tokens")?,
        started_at_ms: u128::try_from(started_at_ms)
            .map_err(|_| AppError::blocked("latest model run timestamp가 유효하지 않습니다."))?,
    }))
}

fn checked_optional_u32(value: Option<i64>, label: &str) -> Result<Option<u32>, AppError> {
    value
        .map(|value| {
            u32::try_from(value).map_err(|_| {
                AppError::blocked(format!("latest model run {label} 값이 유효하지 않습니다."))
            })
        })
        .transpose()
}

#[cfg(test)]
#[path = "latest_model_run/tests.rs"]
mod tests;
