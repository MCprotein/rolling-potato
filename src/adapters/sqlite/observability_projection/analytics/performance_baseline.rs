//! Performance baseline query orchestration and aggregation.

use super::statistics::{average, percentile};
use super::*;

pub(in crate::adapters::sqlite::observability_projection) fn performance_baseline(
    ledger: &dyn CanonicalProjectionReadPort,
) -> Result<PerformanceBaseline, AppError> {
    let (connection, recovered_from) = open_or_recover()?;
    replay_ledger_events(&connection, &ledger.read_events()?, ledger)?;
    let store = status_from_connection(&connection, recovered_from)?;
    let model_rows = query_baseline_model_rows(&connection)?;
    let resource_rows = query_baseline_resource_rows(&connection)?;

    let mut latencies = Vec::new();
    let mut tokens_per_second = Vec::new();
    let mut total_prompt_tokens = 0;
    let mut total_completion_tokens = 0;
    let mut total_tokens = 0;
    let mut context_clamp_count = 0;
    let mut context_tokens_dropped = 0;
    let mut latest_context_limit_tokens = None;
    let mut groups = BTreeMap::<(String, String, String), GroupAccumulator>::new();

    for row in &model_rows {
        if row.context_limit_tokens.is_some() {
            latest_context_limit_tokens = row.context_limit_tokens;
        }
        if let Some(value) = row.total_latency_ms {
            if value.is_finite() {
                latencies.push(value);
            }
        }
        if let Some(value) = row.tokens_per_second {
            if value.is_finite() {
                tokens_per_second.push(value);
            }
        }
        total_prompt_tokens += row.prompt_tokens;
        total_completion_tokens += row.completion_tokens;
        total_tokens += row.total_tokens;
        context_tokens_dropped += row.context_tokens_dropped;
        if row.context_tokens_dropped > 0 {
            context_clamp_count += 1;
        }

        let group = groups
            .entry((
                row.model_id.clone(),
                row.backend_id.clone(),
                row.session_id.clone(),
            ))
            .or_default();
        group.runs += 1;
        group.total_tokens += row.total_tokens;
        group.context_tokens_dropped += row.context_tokens_dropped;
        if row.context_tokens_dropped > 0 {
            group.context_clamp_count += 1;
        }
        if let Some(value) = row.total_latency_ms {
            if value.is_finite() {
                group.latencies.push(value);
            }
        }
        if let Some(value) = row.tokens_per_second {
            if value.is_finite() {
                group.tokens_per_second.push(value);
            }
        }
    }

    let mut pressure_counts = BTreeMap::<String, i64>::new();
    let mut peak_rss_bytes: Option<u64> = None;
    for row in &resource_rows {
        *pressure_counts
            .entry(row.pressure_status.clone())
            .or_default() += 1;
        if let Some(value) = row.peak_rss_bytes {
            peak_rss_bytes = Some(peak_rss_bytes.map_or(value, |current| current.max(value)));
        }
    }

    let pressure_states = pressure_counts
        .into_iter()
        .map(|(pressure_status, samples)| PressureStateSummary {
            pressure_status,
            samples,
        })
        .collect();

    let mut groups = groups
        .into_iter()
        .map(
            |((model_id, backend_id, session_id), group)| PerformanceGroupSummary {
                model_id,
                backend_id,
                session_id,
                runs: group.runs,
                total_tokens: group.total_tokens,
                context_clamp_count: group.context_clamp_count,
                context_tokens_dropped: group.context_tokens_dropped,
                p50_latency_ms: percentile(group.latencies.clone(), 50.0),
                p95_latency_ms: percentile(group.latencies, 95.0),
                avg_tokens_per_second: average(&group.tokens_per_second),
            },
        )
        .collect::<Vec<_>>();
    groups.sort_by(|left, right| {
        right
            .runs
            .cmp(&left.runs)
            .then_with(|| right.total_tokens.cmp(&left.total_tokens))
            .then_with(|| left.model_id.cmp(&right.model_id))
            .then_with(|| left.backend_id.cmp(&right.backend_id))
            .then_with(|| left.session_id.cmp(&right.session_id))
    });

    Ok(PerformanceBaseline {
        store,
        model_runs: model_rows.len(),
        latest_context_limit_tokens,
        token_records: count_scalar(&connection, "SELECT COUNT(*) FROM token_usage")?,
        resource_samples: resource_rows.len(),
        total_prompt_tokens,
        total_completion_tokens,
        total_tokens,
        context_clamp_count,
        context_tokens_dropped,
        p50_latency_ms: percentile(latencies.clone(), 50.0),
        p95_latency_ms: percentile(latencies, 95.0),
        avg_tokens_per_second: average(&tokens_per_second),
        peak_rss_bytes,
        pressure_states,
        groups,
    })
}

#[derive(Debug)]
struct BaselineModelRow {
    session_id: String,
    model_id: String,
    backend_id: String,
    context_limit_tokens: Option<u32>,
    total_latency_ms: Option<f64>,
    tokens_per_second: Option<f64>,
    prompt_tokens: i64,
    completion_tokens: i64,
    total_tokens: i64,
    context_tokens_dropped: i64,
}

#[derive(Debug)]
struct BaselineResourceRow {
    pressure_status: String,
    peak_rss_bytes: Option<u64>,
}

#[derive(Debug, Default)]
struct GroupAccumulator {
    runs: i64,
    total_tokens: i64,
    context_clamp_count: i64,
    context_tokens_dropped: i64,
    latencies: Vec<f64>,
    tokens_per_second: Vec<f64>,
}

fn query_baseline_model_rows(connection: &Connection) -> Result<Vec<BaselineModelRow>, AppError> {
    let mut statement = connection
        .prepare(
            "SELECT
                model_runs.session_id,
                model_runs.model_id,
                COALESCE(model_runs.backend_id, 'unknown'),
                model_runs.context_limit_tokens,
                model_runs.total_latency_ms,
                model_runs.tokens_per_second,
                COALESCE(token_usage.prompt_tokens, 0),
                COALESCE(token_usage.completion_tokens, 0),
                COALESCE(token_usage.total_tokens, 0),
                COALESCE(token_usage.context_tokens_dropped, 0)
               FROM model_runs
          LEFT JOIN (
                SELECT model_run_id,
                       SUM(prompt_tokens) AS prompt_tokens,
                       SUM(completion_tokens) AS completion_tokens,
                       SUM(total_tokens) AS total_tokens,
                       SUM(context_tokens_dropped) AS context_tokens_dropped
                  FROM token_usage
              GROUP BY model_run_id
          ) token_usage
                 ON token_usage.model_run_id = model_runs.model_run_id
              ORDER BY model_runs.started_at_ms ASC,
                       model_runs.model_run_id ASC",
        )
        .map_err(sql_error(
            "performance baseline model query를 준비하지 못했습니다",
        ))?;

    let rows = statement
        .query_map([], |row| {
            Ok(BaselineModelRow {
                session_id: row.get(0)?,
                model_id: row.get(1)?,
                backend_id: row.get(2)?,
                context_limit_tokens: option_i64_to_u32(row.get(3)?),
                total_latency_ms: row.get(4)?,
                tokens_per_second: row.get(5)?,
                prompt_tokens: row.get(6)?,
                completion_tokens: row.get(7)?,
                total_tokens: row.get(8)?,
                context_tokens_dropped: row.get(9)?,
            })
        })
        .map_err(sql_error(
            "performance baseline model query를 실행하지 못했습니다",
        ))?;

    rows.collect::<Result<Vec<_>, _>>().map_err(sql_error(
        "performance baseline model 결과를 읽지 못했습니다",
    ))
}

fn query_baseline_resource_rows(
    connection: &Connection,
) -> Result<Vec<BaselineResourceRow>, AppError> {
    let mut statement = connection
        .prepare(
            "SELECT pressure_status,
                    peak_rss_bytes
               FROM resource_samples
              ORDER BY recorded_at_ms ASC,
                       resource_sample_id ASC",
        )
        .map_err(sql_error(
            "performance baseline resource query를 준비하지 못했습니다",
        ))?;

    let rows = statement
        .query_map([], |row| {
            Ok(BaselineResourceRow {
                pressure_status: row.get(0)?,
                peak_rss_bytes: option_i64_to_u64(row.get(1)?),
            })
        })
        .map_err(sql_error(
            "performance baseline resource query를 실행하지 못했습니다",
        ))?;

    rows.collect::<Result<Vec<_>, _>>().map_err(sql_error(
        "performance baseline resource 결과를 읽지 못했습니다",
    ))
}
