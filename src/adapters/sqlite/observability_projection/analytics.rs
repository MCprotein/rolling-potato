use super::*;

pub(super) fn model_summaries_from_connection(
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

pub(super) fn model_summaries() -> Result<Vec<ModelMetricSummary>, AppError> {
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

pub(super) fn latest_model_run_from_connection(
    connection: &Connection,
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
              ORDER BY model_runs.started_at_ms DESC,
                       model_runs.model_run_id DESC
                 LIMIT 1",
        )
        .map_err(sql_error("latest model run query 준비 실패"))?;
    let mut rows = statement
        .query([])
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

pub(super) fn performance_baseline(
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
    let mut groups = BTreeMap::<(String, String, String), GroupAccumulator>::new();

    for row in &model_rows {
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

pub(super) fn optimization_policy(
    ledger: &dyn CanonicalProjectionReadPort,
) -> Result<OptimizationPolicy, AppError> {
    let baseline = performance_baseline(ledger)?;
    let latest_resource = latest_resource_sample()?;
    let latest_resource_pressure = latest_resource
        .as_ref()
        .map(|sample| sample.pressure_status.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let benchmark_evidence = benchmark_evidence_summary(&benchmark_run_reports(ledger)?);
    let decision = resource::optimization_policy_decision(resource::OptimizationPolicyInput {
        pressure: resource_pressure_from_status(&latest_resource_pressure),
        model_runs: baseline.model_runs,
        measured_benchmark_runs: benchmark_evidence.measured_runs,
        failed_benchmark_runs: benchmark_evidence.failed_runs,
        context_clamp_count: baseline.context_clamp_count,
        p95_latency_ms: baseline.p95_latency_ms,
        avg_tokens_per_second: baseline.avg_tokens_per_second,
    });

    Ok(OptimizationPolicy {
        store: baseline.store.clone(),
        model_runs: baseline.model_runs,
        resource_samples: baseline.resource_samples,
        latest_resource_pressure,
        context_clamp_count: baseline.context_clamp_count,
        context_tokens_dropped: baseline.context_tokens_dropped,
        p95_latency_ms: baseline.p95_latency_ms,
        avg_tokens_per_second: baseline.avg_tokens_per_second,
        peak_rss_bytes: baseline.peak_rss_bytes,
        benchmark_evidence,
        decision,
    })
}

#[derive(Debug)]
struct BaselineModelRow {
    session_id: String,
    model_id: String,
    backend_id: String,
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
                total_latency_ms: row.get(3)?,
                tokens_per_second: row.get(4)?,
                prompt_tokens: row.get(5)?,
                completion_tokens: row.get(6)?,
                total_tokens: row.get(7)?,
                context_tokens_dropped: row.get(8)?,
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

fn benchmark_evidence_summary(rows: &[BenchmarkRunReport]) -> BenchmarkEvidenceSummary {
    let measured = rows
        .iter()
        .filter(|row| row.claim_state == "measured-locally")
        .collect::<Vec<_>>();
    let scores = measured
        .iter()
        .filter_map(|row| row.score)
        .filter(|score| score.is_finite())
        .collect::<Vec<_>>();
    let latest = measured.iter().max_by(|left, right| {
        left.recorded_at_ms
            .cmp(&right.recorded_at_ms)
            .then_with(|| left.benchmark_run_id.cmp(&right.benchmark_run_id))
    });

    BenchmarkEvidenceSummary {
        measured_runs: measured.len(),
        passed_runs: measured
            .iter()
            .filter(|row| row.local_pass == Some(true))
            .count(),
        failed_runs: measured
            .iter()
            .filter(|row| row.local_pass == Some(false))
            .count(),
        avg_score: average(&scores),
        latest_benchmark_run_id: latest.map(|row| row.benchmark_run_id.clone()),
        latest_model_id: latest.map(|row| row.model_id.clone()),
        latest_benchmark_name: latest.map(|row| row.benchmark_name.clone()),
    }
}

fn resource_pressure_from_status(value: &str) -> resource::ResourcePressure {
    match value {
        "normal" => resource::ResourcePressure::Normal,
        "degraded" => resource::ResourcePressure::Degraded,
        "critical" => resource::ResourcePressure::Critical,
        _ => resource::ResourcePressure::Unknown,
    }
}

fn average(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    Some(values.iter().sum::<f64>() / values.len() as f64)
}

fn percentile(mut values: Vec<f64>, percentile: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let percentile = percentile.clamp(0.0, 100.0);
    let position = (percentile / 100.0) * (values.len() - 1) as f64;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    if lower == upper {
        return Some(values[lower]);
    }
    let weight = position - lower as f64;
    Some(values[lower] + (values[upper] - values[lower]) * weight)
}
