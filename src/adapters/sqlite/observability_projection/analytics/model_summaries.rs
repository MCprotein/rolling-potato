//! Model-level metric summary queries.

use super::*;

pub(in crate::adapters::sqlite::observability_projection) fn model_summaries_from_connection(
    connection: &Connection,
    limit: usize,
) -> Result<Vec<ModelMetricSummary>, AppError> {
    let mut statement = connection
        .prepare(
            "SELECT token_usage.model_id,
                    COUNT(*) AS runs,
                    COALESCE(SUM(token_usage.prompt_tokens), 0),
                    COALESCE(SUM(token_usage.completion_tokens), 0),
                    COALESCE(SUM(token_usage.total_tokens), 0),
                    AVG(model_runs.total_latency_ms),
                    AVG(model_runs.tokens_per_second)
               FROM token_usage
          LEFT JOIN model_runs
                 ON token_usage.model_run_id = model_runs.model_run_id
              GROUP BY token_usage.model_id
              ORDER BY SUM(token_usage.total_tokens) DESC, token_usage.model_id ASC
                 LIMIT ?1",
        )
        .map_err(sql_error("read-only model metric query 준비 실패"))?;
    let rows = statement
        .query_map(params![i64::try_from(limit).unwrap_or(i64::MAX)], |row| {
            Ok(ModelMetricSummary {
                model_id: row.get(0)?,
                runs: row.get(1)?,
                prompt_tokens: row.get(2)?,
                completion_tokens: row.get(3)?,
                total_tokens: row.get(4)?,
                avg_latency_ms: row.get(5)?,
                avg_tokens_per_second: row.get(6)?,
            })
        })
        .map_err(sql_error("read-only model metric query 실행 실패"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error("read-only model metric 결과 읽기 실패"))?;
    Ok(rows)
}

pub(in crate::adapters::sqlite::observability_projection) fn model_summaries(
) -> Result<Vec<ModelMetricSummary>, AppError> {
    let (connection, _) = open_or_recover()?;
    let mut statement = connection
        .prepare(
            "SELECT token_usage.model_id,
                    COUNT(*) AS runs,
                    COALESCE(SUM(token_usage.prompt_tokens), 0),
                    COALESCE(SUM(token_usage.completion_tokens), 0),
                    COALESCE(SUM(token_usage.total_tokens), 0),
                    AVG(model_runs.total_latency_ms),
                    AVG(model_runs.tokens_per_second)
               FROM token_usage
          LEFT JOIN model_runs
                 ON token_usage.model_run_id = model_runs.model_run_id
              GROUP BY token_usage.model_id
              ORDER BY SUM(token_usage.total_tokens) DESC, token_usage.model_id ASC",
        )
        .map_err(sql_error("model metric query를 준비하지 못했습니다"))?;

    let rows = statement
        .query_map([], |row| {
            Ok(ModelMetricSummary {
                model_id: row.get(0)?,
                runs: row.get(1)?,
                prompt_tokens: row.get(2)?,
                completion_tokens: row.get(3)?,
                total_tokens: row.get(4)?,
                avg_latency_ms: row.get(5)?,
                avg_tokens_per_second: row.get(6)?,
            })
        })
        .map_err(sql_error("model metric query를 실행하지 못했습니다"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(sql_error("model metric 결과를 읽지 못했습니다"))
}
