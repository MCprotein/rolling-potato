//! Surface-neutral observability records shared by projection adapters and monitors.

use std::path::PathBuf;

use crate::foundation::error::AppError;
use crate::runtime_core::inference::resource;
use crate::runtime_core::workflow::storage_compat::ledger::{
    LedgerEvent, ParsedLedgerEvent, RuntimeIdentity,
};
use crate::runtime_core::workflow::storage_compat::transcript::TranscriptRecord;

pub(crate) trait CanonicalLedgerReadPort {
    fn read_events(&self) -> Result<Vec<ParsedLedgerEvent>, AppError>;
}

pub(crate) trait CanonicalTranscriptReadPort {
    fn read_transcript_record(
        &self,
        project_id: &str,
        session_id: &str,
        event_type: &str,
        details: &str,
    ) -> Result<TranscriptRecord, AppError> {
        let _ = (project_id, session_id, event_type, details);
        Err(AppError::blocked(
            "canonical transcript read port is unavailable",
        ))
    }
}

pub(crate) trait CanonicalProjectionReadPort:
    CanonicalLedgerReadPort + CanonicalTranscriptReadPort
{
}

impl<T> CanonicalProjectionReadPort for T where
    T: CanonicalLedgerReadPort + CanonicalTranscriptReadPort + ?Sized
{
}

pub(crate) trait ObservabilityProjectionPort {
    fn initialize(
        &self,
        identity: &RuntimeIdentity,
        ledger: &dyn CanonicalProjectionReadPort,
    ) -> Result<StoreStatus, AppError>;

    fn status(&self, ledger: &dyn CanonicalProjectionReadPort) -> Result<StoreStatus, AppError>;

    fn status_read_only(&self) -> Result<StoreStatus, AppError>;

    fn monitor_snapshot_read_only(
        &self,
        limit: usize,
    ) -> Result<MonitorProjectionSnapshot, AppError>;

    fn project_event_with_ordinal(
        &self,
        event: &LedgerEvent,
        ordinal: u64,
        ledger: &dyn CanonicalProjectionReadPort,
    ) -> Result<(), AppError>;

    fn converge_from_events(
        &self,
        events: &[ParsedLedgerEvent],
        ledger: &dyn CanonicalProjectionReadPort,
    ) -> Result<(), AppError>;

    fn model_summaries(&self) -> Result<Vec<ModelMetricSummary>, AppError>;

    fn performance_baseline(
        &self,
        ledger: &dyn CanonicalProjectionReadPort,
    ) -> Result<PerformanceBaseline, AppError>;

    fn optimization_policy(
        &self,
        ledger: &dyn CanonicalProjectionReadPort,
    ) -> Result<OptimizationPolicy, AppError>;

    fn export_jsonl(&self) -> Result<String, AppError>;

    fn export_csv(&self, ledger: &dyn CanonicalProjectionReadPort) -> Result<String, AppError>;

    fn prune_preview(&self, before_days: u64) -> Result<PrunePreview, AppError>;

    fn session_history(
        &self,
        identity: &RuntimeIdentity,
        ledger: &dyn CanonicalProjectionReadPort,
        limit: usize,
    ) -> Result<Vec<SessionHistoryEntry>, AppError>;

    fn session_entry(
        &self,
        identity: &RuntimeIdentity,
        ledger: &dyn CanonicalProjectionReadPort,
        session_id: &str,
    ) -> Result<Option<SessionHistoryEntry>, AppError>;

    fn session_events(
        &self,
        identity: &RuntimeIdentity,
        ledger: &dyn CanonicalProjectionReadPort,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<SessionEventEntry>, AppError>;

    fn record_model_run(
        &self,
        identity: &RuntimeIdentity,
        ledger: &dyn CanonicalProjectionReadPort,
        metric: &ModelRunMetric,
    ) -> Result<(), AppError>;

    fn record_resource_sample(
        &self,
        identity: &RuntimeIdentity,
        ledger: &dyn CanonicalProjectionReadPort,
        metric: &ResourceSampleMetric,
    ) -> Result<(), AppError>;

    fn record_benchmark_run(
        &self,
        identity: &RuntimeIdentity,
        ledger: &dyn CanonicalProjectionReadPort,
        metric: &BenchmarkRunMetric,
    ) -> Result<(), AppError>;

    fn benchmark_run_reports(
        &self,
        ledger: &dyn CanonicalProjectionReadPort,
    ) -> Result<Vec<BenchmarkRunReport>, AppError>;

    fn latest_resource_sample(&self) -> Result<Option<ResourceSampleMetric>, AppError>;

    fn latest_model_run_for_session(
        &self,
        session_id: &str,
    ) -> Result<Option<LatestModelRunSnapshot>, AppError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreStatus {
    pub path: PathBuf,
    pub recovered_from: Option<PathBuf>,
    pub migration_version: i64,
    pub ledger_events: i64,
    pub sessions: i64,
    pub workflows: i64,
    pub transcript_records: i64,
    pub model_runs: i64,
    pub token_records: i64,
    pub resource_samples: i64,
    pub benchmark_runs: i64,
    pub evidence_records: i64,
    pub stop_gate_results: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelMetricSummary {
    pub model_id: String,
    pub runs: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub avg_latency_ms: Option<f64>,
    pub avg_tokens_per_second: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatestModelRunSnapshot {
    pub model_id: String,
    pub context_limit_tokens: Option<u32>,
    pub context_tokens_used: Option<u32>,
    pub total_tokens: Option<u32>,
    pub started_at_ms: u128,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MonitorProjectionSnapshot {
    pub status: StoreStatus,
    pub model_summaries: Vec<ModelMetricSummary>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PerformanceBaseline {
    pub store: StoreStatus,
    pub model_runs: usize,
    pub token_records: i64,
    pub resource_samples: usize,
    pub total_prompt_tokens: i64,
    pub total_completion_tokens: i64,
    pub total_tokens: i64,
    pub context_clamp_count: i64,
    pub context_tokens_dropped: i64,
    pub p50_latency_ms: Option<f64>,
    pub p95_latency_ms: Option<f64>,
    pub avg_tokens_per_second: Option<f64>,
    pub peak_rss_bytes: Option<u64>,
    pub pressure_states: Vec<PressureStateSummary>,
    pub groups: Vec<PerformanceGroupSummary>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PressureStateSummary {
    pub pressure_status: String,
    pub samples: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PerformanceGroupSummary {
    pub model_id: String,
    pub backend_id: String,
    pub session_id: String,
    pub runs: i64,
    pub total_tokens: i64,
    pub context_clamp_count: i64,
    pub context_tokens_dropped: i64,
    pub p50_latency_ms: Option<f64>,
    pub p95_latency_ms: Option<f64>,
    pub avg_tokens_per_second: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkRunMetric {
    pub benchmark_run_id: String,
    pub session_id: String,
    pub model_run_id: Option<String>,
    pub model_id: String,
    pub benchmark_name: String,
    pub fixture_id: String,
    pub fixture_sha256: String,
    pub prompt_artifact_sha256: Option<String>,
    pub prompt_chars: Option<u32>,
    pub claim_state: String,
    pub score: Option<f64>,
    pub score_unit: Option<String>,
    pub local_pass: Option<bool>,
    pub expected_matches: Option<u32>,
    pub expected_total: Option<u32>,
    pub forbidden_matches: Option<u32>,
    pub harness_ref: String,
    pub dataset_ref: Option<String>,
    pub backend_id: Option<String>,
    pub latency_ms: Option<f64>,
    pub tokens_per_second: Option<f64>,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
    pub resource_pressure: Option<String>,
    pub peak_rss_bytes: Option<u64>,
    pub reproducibility_manifest: String,
    pub redacted_report: String,
    pub recorded_at_ms: u128,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkRunReport {
    pub benchmark_run_id: String,
    pub session_id: String,
    pub model_run_id: Option<String>,
    pub model_id: String,
    pub benchmark_name: String,
    pub fixture_id: String,
    pub fixture_sha256: String,
    pub prompt_artifact_sha256: Option<String>,
    pub prompt_chars: Option<u32>,
    pub claim_state: String,
    pub score: Option<f64>,
    pub score_unit: Option<String>,
    pub local_pass: Option<bool>,
    pub expected_matches: Option<u32>,
    pub expected_total: Option<u32>,
    pub forbidden_matches: Option<u32>,
    pub harness_ref: String,
    pub dataset_ref: Option<String>,
    pub backend_id: Option<String>,
    pub latency_ms: Option<f64>,
    pub tokens_per_second: Option<f64>,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
    pub resource_pressure: Option<String>,
    pub peak_rss_bytes: Option<u64>,
    pub reproducibility_manifest: String,
    pub redacted_report: String,
    pub recorded_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkEvidenceSummary {
    pub measured_runs: usize,
    pub passed_runs: usize,
    pub failed_runs: usize,
    pub avg_score: Option<f64>,
    pub latest_benchmark_run_id: Option<String>,
    pub latest_model_id: Option<String>,
    pub latest_benchmark_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OptimizationPolicy {
    pub store: StoreStatus,
    pub model_runs: usize,
    pub resource_samples: usize,
    pub latest_resource_pressure: String,
    pub context_clamp_count: i64,
    pub context_tokens_dropped: i64,
    pub p95_latency_ms: Option<f64>,
    pub avg_tokens_per_second: Option<f64>,
    pub peak_rss_bytes: Option<u64>,
    pub benchmark_evidence: BenchmarkEvidenceSummary,
    pub decision: resource::OptimizationPolicyDecision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrunePreview {
    pub cutoff_ms: u128,
    pub ledger_rows: i64,
    pub model_run_rows: i64,
    pub command_run_rows: i64,
    pub resource_sample_rows: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionHistoryEntry {
    pub session_id: String,
    pub project_id: String,
    pub project_root: String,
    pub started_at_ms: i64,
    pub event_count: i64,
    pub last_event_at_ms: Option<i64>,
    pub last_summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionEventEntry {
    pub event_id: String,
    pub ts_ms: i64,
    pub event_type: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelRunMetric {
    pub model_run_id: String,
    pub session_id: String,
    pub workflow_id: Option<String>,
    pub model_id: String,
    pub model_artifact_hash: Option<String>,
    pub backend_id: Option<String>,
    pub backend_version: Option<String>,
    pub quantization: Option<String>,
    pub context_limit_tokens: Option<u32>,
    pub started_at_ms: u128,
    pub first_token_latency_ms: Option<f64>,
    pub total_latency_ms: Option<f64>,
    pub prompt_eval_ms: Option<f64>,
    pub generation_eval_ms: Option<f64>,
    pub tokens_per_second: Option<f64>,
    pub cancelled: bool,
    pub token_usage_complete: bool,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    pub context_tokens_used: u32,
    pub context_tokens_dropped: u32,
    pub ontology_tokens: u32,
    pub tool_summary_tokens: u32,
    pub max_output_tokens: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResourceSampleMetric {
    pub resource_sample_id: String,
    pub session_id: String,
    pub backend_id: String,
    pub pid: u32,
    pub process_cpu_percent: Option<f64>,
    pub average_rss_bytes: Option<u64>,
    pub peak_rss_bytes: Option<u64>,
    pub disk_bytes: Option<u64>,
    pub sample_count: u32,
    pub pressure_status: String,
    pub recorded_at_ms: u128,
}
