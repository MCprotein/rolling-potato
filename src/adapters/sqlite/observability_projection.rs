use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};

use crate::adapters::filesystem::layout as paths;
use crate::foundation::error::AppError;
use crate::runtime_core::inference::resource;
use crate::runtime_core::observability::facade::{
    BenchmarkEvidenceSummary, BenchmarkRunMetric, BenchmarkRunReport, CanonicalProjectionReadPort,
    ModelMetricSummary, ModelRunMetric, MonitorProjectionSnapshot, ObservabilityProjectionPort,
    OptimizationPolicy, PerformanceBaseline, PerformanceGroupSummary, PressureStateSummary,
    PrunePreview, ResourceSampleMetric, SessionEventEntry, SessionHistoryEntry, StoreStatus,
};
#[cfg(test)]
use crate::runtime_core::observability::facade::{
    CanonicalLedgerReadPort, CanonicalTranscriptReadPort,
};
use crate::runtime_core::workflow::storage_compat::ledger::{
    LedgerEvent, ParsedLedgerEvent, RuntimeIdentity,
};

mod read_snapshot;
mod schema;
use read_snapshot::open_read_only;
#[cfg(test)]
use read_snapshot::open_read_only_path;
use schema::migrate;

pub(crate) struct SqliteObservabilityProjection;

impl ObservabilityProjectionPort for SqliteObservabilityProjection {
    fn initialize(
        &self,
        identity: &RuntimeIdentity,
        ledger: &dyn CanonicalProjectionReadPort,
    ) -> Result<StoreStatus, AppError> {
        initialize(identity, ledger)
    }

    fn status(&self, ledger: &dyn CanonicalProjectionReadPort) -> Result<StoreStatus, AppError> {
        status(ledger)
    }

    fn status_read_only(&self) -> Result<StoreStatus, AppError> {
        status_read_only()
    }

    fn monitor_snapshot_read_only(
        &self,
        limit: usize,
    ) -> Result<MonitorProjectionSnapshot, AppError> {
        monitor_snapshot_read_only(limit)
    }

    fn project_event(
        &self,
        event: &LedgerEvent,
        ledger: &dyn CanonicalProjectionReadPort,
    ) -> Result<(), AppError> {
        project_event(event, ledger)
    }

    fn project_event_with_ordinal(
        &self,
        event: &LedgerEvent,
        ordinal: u64,
        ledger: &dyn CanonicalProjectionReadPort,
    ) -> Result<(), AppError> {
        project_event_with_ordinal(event, ordinal, ledger)
    }

    fn converge_from_events(
        &self,
        events: &[ParsedLedgerEvent],
        ledger: &dyn CanonicalProjectionReadPort,
    ) -> Result<(), AppError> {
        converge_from_events(events, ledger)
    }

    fn model_summaries(&self) -> Result<Vec<ModelMetricSummary>, AppError> {
        model_summaries()
    }

    fn performance_baseline(
        &self,
        ledger: &dyn CanonicalProjectionReadPort,
    ) -> Result<PerformanceBaseline, AppError> {
        performance_baseline(ledger)
    }

    fn optimization_policy(
        &self,
        ledger: &dyn CanonicalProjectionReadPort,
    ) -> Result<OptimizationPolicy, AppError> {
        optimization_policy(ledger)
    }

    fn export_jsonl(&self) -> Result<String, AppError> {
        export_jsonl()
    }

    fn export_csv(&self, ledger: &dyn CanonicalProjectionReadPort) -> Result<String, AppError> {
        export_csv(ledger)
    }

    fn prune_preview(&self, before_days: u64) -> Result<PrunePreview, AppError> {
        prune_preview(before_days)
    }

    fn session_history(
        &self,
        identity: &RuntimeIdentity,
        ledger: &dyn CanonicalProjectionReadPort,
        limit: usize,
    ) -> Result<Vec<SessionHistoryEntry>, AppError> {
        session_history(identity, ledger, limit)
    }

    fn session_entry(
        &self,
        identity: &RuntimeIdentity,
        ledger: &dyn CanonicalProjectionReadPort,
        session_id: &str,
    ) -> Result<Option<SessionHistoryEntry>, AppError> {
        session_entry(identity, ledger, session_id)
    }

    fn session_events(
        &self,
        identity: &RuntimeIdentity,
        ledger: &dyn CanonicalProjectionReadPort,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<SessionEventEntry>, AppError> {
        session_events(identity, ledger, session_id, limit)
    }

    fn record_model_run(
        &self,
        identity: &RuntimeIdentity,
        ledger: &dyn CanonicalProjectionReadPort,
        metric: &ModelRunMetric,
    ) -> Result<(), AppError> {
        record_model_run(identity, ledger, metric)
    }

    fn record_resource_sample(
        &self,
        identity: &RuntimeIdentity,
        ledger: &dyn CanonicalProjectionReadPort,
        metric: &ResourceSampleMetric,
    ) -> Result<(), AppError> {
        record_resource_sample(identity, ledger, metric)
    }

    fn record_benchmark_run(
        &self,
        identity: &RuntimeIdentity,
        ledger: &dyn CanonicalProjectionReadPort,
        metric: &BenchmarkRunMetric,
    ) -> Result<(), AppError> {
        record_benchmark_run(identity, ledger, metric)
    }

    fn benchmark_run_reports(
        &self,
        ledger: &dyn CanonicalProjectionReadPort,
    ) -> Result<Vec<BenchmarkRunReport>, AppError> {
        benchmark_run_reports(ledger)
    }

    fn latest_resource_sample(&self) -> Result<Option<ResourceSampleMetric>, AppError> {
        latest_resource_sample()
    }
}

pub fn initialize(
    identity: &RuntimeIdentity,
    ledger: &dyn CanonicalProjectionReadPort,
) -> Result<StoreStatus, AppError> {
    let (connection, recovered_from) = open_or_recover()?;
    record_session(&connection, identity)?;
    replay_ledger_events(&connection, &ledger.read_events()?, ledger)?;
    project_sessions_from_events(&connection, identity)?;
    status_from_connection(&connection, recovered_from)
}

pub fn status(ledger: &dyn CanonicalProjectionReadPort) -> Result<StoreStatus, AppError> {
    let (connection, recovered_from) = open_or_recover()?;
    replay_ledger_events(&connection, &ledger.read_events()?, ledger)?;
    status_from_connection(&connection, recovered_from)
}

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

fn model_summaries_from_connection(
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

pub fn project_event(
    event: &LedgerEvent,
    ledger: &dyn CanonicalProjectionReadPort,
) -> Result<(), AppError> {
    let (connection, _) = open_or_recover()?;
    insert_ledger_event(&connection, event, None, ledger)
}

pub(crate) fn project_event_with_ordinal(
    event: &LedgerEvent,
    ordinal: u64,
    ledger: &dyn CanonicalProjectionReadPort,
) -> Result<(), AppError> {
    let ordinal = i64::try_from(ordinal)
        .map_err(|_| AppError::blocked("observability event ordinal 범위 초과"))?;
    let (connection, _) = open_or_recover()?;
    insert_ledger_event(&connection, event, Some(ordinal), ledger)
}

pub(crate) fn converge_from_events(
    events: &[ParsedLedgerEvent],
    ledger: &dyn CanonicalProjectionReadPort,
) -> Result<(), AppError> {
    let (connection, _) = open_or_recover()?;
    replay_ledger_events(&connection, events, ledger)
}

pub fn model_summaries() -> Result<Vec<ModelMetricSummary>, AppError> {
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

pub fn performance_baseline(
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

pub fn optimization_policy(
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

pub fn record_model_run(
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

pub fn record_resource_sample(
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

pub fn record_benchmark_run(
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

pub fn benchmark_run_reports(
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

pub fn latest_resource_sample() -> Result<Option<ResourceSampleMetric>, AppError> {
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

fn open_or_recover() -> Result<(Connection, Option<PathBuf>), AppError> {
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

fn record_session(connection: &Connection, identity: &RuntimeIdentity) -> Result<(), AppError> {
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

fn replay_ledger_events(
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

fn project_sessions_from_events(
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

fn insert_ledger_event(
    connection: &Connection,
    event: &LedgerEvent,
    supplied_ordinal: Option<i64>,
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
            event_ordinal: match supplied_ordinal {
                Some(ordinal) => ordinal,
                None => canonical_event_ordinal(&event.event_id, ledger)?,
            },
        },
        ledger,
    )
}

fn canonical_event_ordinal(
    event_id: &str,
    ledger: &dyn CanonicalProjectionReadPort,
) -> Result<i64, AppError> {
    ledger
        .read_events()?
        .iter()
        .position(|event| event.event_id == event_id)
        .map(|index| to_i64((index + 1) as u128))
        .ok_or_else(|| {
            AppError::blocked(format!(
                "observability projection 차단\n- 이유: canonical ledger event ordinal을 찾지 못했습니다.\n- event id: {event_id}"
            ))
        })
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

fn project_workflow_checkpoint(
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

fn status_from_connection(
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

fn count_scalar(connection: &Connection, sql: &str) -> Result<i64, AppError> {
    connection
        .query_row(sql, [], |row| row.get(0))
        .map_err(sql_error("observability count query를 실행하지 못했습니다"))
}

fn count_before(
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

fn csv_cell(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn sql_error(context: &'static str) -> impl FnOnce(rusqlite::Error) -> AppError {
    move |err| AppError::runtime(format!("{context}: {err}"))
}

fn to_i64(value: u128) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn i64_to_u128(value: i64) -> u128 {
    u128::try_from(value).unwrap_or_default()
}

fn i64_to_u32(value: i64) -> u32 {
    u32::try_from(value).unwrap_or_default()
}

fn option_i64_to_u32(value: Option<i64>) -> Option<u32> {
    value.and_then(|value| u32::try_from(value).ok())
}

fn option_i64_to_bool(value: Option<i64>) -> Option<bool> {
    value.map(|value| value != 0)
}

fn option_i64_to_u64(value: Option<i64>) -> Option<u64> {
    value.and_then(|value| u64::try_from(value).ok())
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "observability_projection/tests.rs"]
mod tests;
