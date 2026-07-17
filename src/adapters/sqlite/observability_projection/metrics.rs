use super::*;

pub(super) fn record_model_run(
    identity: &RuntimeIdentity,
    ledger: &dyn CanonicalProjectionReadPort,
    metric: &ModelRunMetric,
) -> Result<(), AppError> {
    let (connection, _) = open_or_recover()?;
    record_session(&connection, identity)?;
    replay_ledger_events(&connection, &ledger.read_events()?, ledger)?;
    connection
        .execute(
            "INSERT OR IGNORE INTO model_runs (
                model_run_id,
                session_id,
                workflow_id,
                model_id,
                model_artifact_hash,
                backend_id,
                backend_version,
                quantization,
                context_limit_tokens,
                started_at_ms,
                first_token_latency_ms,
                total_latency_ms,
                prompt_eval_ms,
                generation_eval_ms,
                tokens_per_second,
                cancelled
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                metric.model_run_id,
                metric.session_id,
                metric.workflow_id,
                metric.model_id,
                metric.model_artifact_hash,
                metric.backend_id,
                metric.backend_version,
                metric.quantization,
                metric.context_limit_tokens.map(i64::from),
                to_i64(metric.started_at_ms),
                metric.first_token_latency_ms,
                metric.total_latency_ms,
                metric.prompt_eval_ms,
                metric.generation_eval_ms,
                metric.tokens_per_second,
                if metric.cancelled { 1 } else { 0 },
            ],
        )
        .map_err(sql_error("model run metric을 저장하지 못했습니다"))?;

    if metric.token_usage_complete {
        connection
            .execute(
                "INSERT OR IGNORE INTO token_usage (
                token_usage_id,
                model_run_id,
                model_id,
                prompt_tokens,
                completion_tokens,
                total_tokens,
                context_tokens_used,
                context_tokens_dropped,
                ontology_tokens,
                tool_summary_tokens,
                max_output_tokens
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    format!("token-{}", metric.model_run_id),
                    metric.model_run_id,
                    metric.model_id,
                    i64::from(metric.prompt_tokens),
                    i64::from(metric.completion_tokens),
                    i64::from(metric.total_tokens),
                    i64::from(metric.context_tokens_used),
                    i64::from(metric.context_tokens_dropped),
                    i64::from(metric.ontology_tokens),
                    i64::from(metric.tool_summary_tokens),
                    metric.max_output_tokens.map(i64::from),
                ],
            )
            .map_err(sql_error("token usage metric을 저장하지 못했습니다"))?;
    }

    Ok(())
}

pub(super) fn record_resource_sample(
    identity: &RuntimeIdentity,
    ledger: &dyn CanonicalProjectionReadPort,
    metric: &ResourceSampleMetric,
) -> Result<(), AppError> {
    let (connection, _) = open_or_recover()?;
    record_session(&connection, identity)?;
    replay_ledger_events(&connection, &ledger.read_events()?, ledger)?;
    connection
        .execute(
            "INSERT OR IGNORE INTO resource_samples (
                resource_sample_id,
                session_id,
                backend_id,
                pid,
                process_cpu_percent,
                average_rss_bytes,
                peak_rss_bytes,
                disk_bytes,
                sample_count,
                pressure_status,
                recorded_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                metric.resource_sample_id,
                metric.session_id,
                metric.backend_id,
                i64::from(metric.pid),
                metric.process_cpu_percent,
                metric
                    .average_rss_bytes
                    .map(|value| to_i64(u128::from(value))),
                metric.peak_rss_bytes.map(|value| to_i64(u128::from(value))),
                metric.disk_bytes.map(|value| to_i64(u128::from(value))),
                i64::from(metric.sample_count),
                metric.pressure_status,
                to_i64(metric.recorded_at_ms),
            ],
        )
        .map_err(sql_error("resource sample metric을 저장하지 못했습니다"))?;

    Ok(())
}

pub(super) fn record_benchmark_run(
    identity: &RuntimeIdentity,
    ledger: &dyn CanonicalProjectionReadPort,
    metric: &BenchmarkRunMetric,
) -> Result<(), AppError> {
    let (connection, _) = open_or_recover()?;
    record_session(&connection, identity)?;
    replay_ledger_events(&connection, &ledger.read_events()?, ledger)?;
    connection
        .execute(
            "INSERT INTO benchmark_runs (
                benchmark_run_id,
                session_id,
                model_run_id,
                model_id,
                benchmark_name,
                fixture_id,
                fixture_sha256,
                prompt_artifact_sha256,
                prompt_chars,
                claim_state,
                score,
                score_unit,
                local_pass,
                expected_matches,
                expected_total,
                forbidden_matches,
                harness_ref,
                dataset_ref,
                backend_id,
                latency_ms,
                tokens_per_second,
                prompt_tokens,
                completion_tokens,
                total_tokens,
                resource_pressure,
                peak_rss_bytes,
                reproducibility_manifest,
                redacted_report,
                recorded_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29)",
            params![
                metric.benchmark_run_id,
                metric.session_id,
                metric.model_run_id,
                metric.model_id,
                metric.benchmark_name,
                metric.fixture_id,
                metric.fixture_sha256,
                metric.prompt_artifact_sha256,
                metric.prompt_chars.map(i64::from),
                metric.claim_state,
                metric.score,
                metric.score_unit,
                metric.local_pass.map(|value| if value { 1 } else { 0 }),
                metric.expected_matches.map(i64::from),
                metric.expected_total.map(i64::from),
                metric.forbidden_matches.map(i64::from),
                metric.harness_ref,
                metric.dataset_ref,
                metric.backend_id,
                metric.latency_ms,
                metric.tokens_per_second,
                metric.prompt_tokens.map(i64::from),
                metric.completion_tokens.map(i64::from),
                metric.total_tokens.map(i64::from),
                metric.resource_pressure,
                metric
                    .peak_rss_bytes
                    .map(|value| to_i64(u128::from(value))),
                metric.reproducibility_manifest,
                metric.redacted_report,
                to_i64(metric.recorded_at_ms),
            ],
        )
        .map_err(sql_error("benchmark run metric을 저장하지 못했습니다"))?;

    Ok(())
}

pub(super) fn benchmark_run_reports(
    ledger: &dyn CanonicalProjectionReadPort,
) -> Result<Vec<BenchmarkRunReport>, AppError> {
    let (connection, _) = open_or_recover()?;
    replay_ledger_events(&connection, &ledger.read_events()?, ledger)?;
    let mut statement = connection
        .prepare(
            "SELECT
                benchmark_run_id,
                session_id,
                model_run_id,
                model_id,
                benchmark_name,
                fixture_id,
                fixture_sha256,
                prompt_artifact_sha256,
                prompt_chars,
                claim_state,
                score,
                score_unit,
                local_pass,
                expected_matches,
                expected_total,
                forbidden_matches,
                harness_ref,
                dataset_ref,
                backend_id,
                latency_ms,
                tokens_per_second,
                prompt_tokens,
                completion_tokens,
                total_tokens,
                resource_pressure,
                peak_rss_bytes,
                reproducibility_manifest,
                redacted_report,
                recorded_at_ms
               FROM benchmark_runs
              ORDER BY recorded_at_ms ASC,
                       benchmark_run_id ASC",
        )
        .map_err(sql_error(
            "benchmark run report query를 준비하지 못했습니다",
        ))?;

    let rows = statement
        .query_map([], |row| {
            Ok(BenchmarkRunReport {
                benchmark_run_id: row.get(0)?,
                session_id: row.get(1)?,
                model_run_id: row.get(2)?,
                model_id: row.get(3)?,
                benchmark_name: row.get(4)?,
                fixture_id: row.get(5)?,
                fixture_sha256: row.get(6)?,
                prompt_artifact_sha256: row.get(7)?,
                prompt_chars: option_i64_to_u32(row.get(8)?),
                claim_state: row.get(9)?,
                score: row.get(10)?,
                score_unit: row.get(11)?,
                local_pass: option_i64_to_bool(row.get(12)?),
                expected_matches: option_i64_to_u32(row.get(13)?),
                expected_total: option_i64_to_u32(row.get(14)?),
                forbidden_matches: option_i64_to_u32(row.get(15)?),
                harness_ref: row.get(16)?,
                dataset_ref: row.get(17)?,
                backend_id: row.get(18)?,
                latency_ms: row.get(19)?,
                tokens_per_second: row.get(20)?,
                prompt_tokens: option_i64_to_u32(row.get(21)?),
                completion_tokens: option_i64_to_u32(row.get(22)?),
                total_tokens: option_i64_to_u32(row.get(23)?),
                resource_pressure: row.get(24)?,
                peak_rss_bytes: option_i64_to_u64(row.get(25)?),
                reproducibility_manifest: row.get(26)?,
                redacted_report: row.get(27)?,
                recorded_at_ms: row.get(28)?,
            })
        })
        .map_err(sql_error(
            "benchmark run report query를 실행하지 못했습니다",
        ))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(sql_error("benchmark run report 결과를 읽지 못했습니다"))
}

pub(super) fn latest_resource_sample() -> Result<Option<ResourceSampleMetric>, AppError> {
    let (connection, _) = open_or_recover()?;
    let result = connection.query_row(
        "SELECT
            resource_sample_id,
            session_id,
            backend_id,
            pid,
            process_cpu_percent,
            average_rss_bytes,
            peak_rss_bytes,
            disk_bytes,
            sample_count,
            pressure_status,
            recorded_at_ms
           FROM resource_samples
       ORDER BY recorded_at_ms DESC,
                resource_sample_id DESC
          LIMIT 1",
        [],
        |row| {
            Ok(ResourceSampleMetric {
                resource_sample_id: row.get(0)?,
                session_id: row.get(1)?,
                backend_id: row.get(2)?,
                pid: i64_to_u32(row.get(3)?),
                process_cpu_percent: row.get(4)?,
                average_rss_bytes: option_i64_to_u64(row.get(5)?),
                peak_rss_bytes: option_i64_to_u64(row.get(6)?),
                disk_bytes: option_i64_to_u64(row.get(7)?),
                sample_count: i64_to_u32(row.get(8)?),
                pressure_status: row.get(9)?,
                recorded_at_ms: i64_to_u128(row.get(10)?),
            })
        },
    );

    match result {
        Ok(metric) => Ok(Some(metric)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(err) => Err(sql_error(
            "latest resource sample query를 실행하지 못했습니다",
        )(err)),
    }
}
