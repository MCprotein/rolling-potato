use std::cell::Cell;
use std::time::{Duration, Instant};

use super::*;

type LedgerProjectionRow = (i64, String, i64, String, String, String, String);
const TEST_LEDGER: TestCanonicalLedgerReader = TestCanonicalLedgerReader;

struct TestCanonicalLedgerReader;

impl CanonicalLedgerReadPort for TestCanonicalLedgerReader {
    fn read_events(&self) -> Result<Vec<ParsedLedgerEvent>, AppError> {
        crate::app::workflow_adapter::ledger::read_runtime_events()
    }
}

impl CanonicalTranscriptReadPort for TestCanonicalLedgerReader {}

struct CountingLedgerReader {
    reads: Cell<u64>,
}

impl CanonicalLedgerReadPort for CountingLedgerReader {
    fn read_events(&self) -> Result<Vec<ParsedLedgerEvent>, AppError> {
        self.reads.set(self.reads.get() + 1);
        Ok(Vec::new())
    }
}

impl CanonicalTranscriptReadPort for CountingLedgerReader {}

fn replay_test_event(index: u64) -> ParsedLedgerEvent {
    ParsedLedgerEvent {
        event_id: format!("event-replay-{index}"),
        ts_ms: u128::from(index),
        event_type: "test.replay".to_string(),
        project_id: "project-replay".to_string(),
        session_id: "session-replay".to_string(),
        summary: format!("summary-{index}"),
        details: format!("detail={index}"),
        previous_event_hash: None,
        event_hash: None,
    }
}

fn current_identity() -> RuntimeIdentity {
    crate::app::workflow_adapter::ledger::validated_current_identity().unwrap()
}

fn projected_status() -> StoreStatus {
    status(&TEST_LEDGER).unwrap()
}

fn record_test_model_run(metric: &ModelRunMetric) -> Result<(), AppError> {
    record_model_run(&current_identity(), &TEST_LEDGER, metric)
}

fn record_test_resource_sample(metric: &ResourceSampleMetric) -> Result<(), AppError> {
    record_resource_sample(&current_identity(), &TEST_LEDGER, metric)
}

fn record_test_benchmark_run(metric: &BenchmarkRunMetric) -> Result<(), AppError> {
    record_benchmark_run(&current_identity(), &TEST_LEDGER, metric)
}

struct FailingLedgerReader<'a> {
    database: &'a std::path::Path,
    called_after_recovery: Cell<bool>,
}

impl CanonicalLedgerReadPort for FailingLedgerReader<'_> {
    fn read_events(&self) -> Result<Vec<ParsedLedgerEvent>, AppError> {
        let file_name = self.database.file_name().unwrap().to_string_lossy();
        let recovered_prefix = format!("{file_name}.corrupt.");
        let recovered_exists = self
            .database
            .parent()
            .unwrap()
            .read_dir()
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(&recovered_prefix)
            });
        self.called_after_recovery.set(recovered_exists);
        Err(AppError::blocked("injected canonical ledger read failure"))
    }
}

impl CanonicalTranscriptReadPort for FailingLedgerReader<'_> {}

fn ledger_projection_rows(connection: &Connection) -> Vec<LedgerProjectionRow> {
    let mut statement = connection
        .prepare(
            "SELECT rowid, event_id, ts_ms, event_type, project_id, session_id, summary
               FROM ledger_events
           ORDER BY rowid",
        )
        .unwrap();
    statement
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

include!("tests/recovery.rs");
include!("tests/projection.rs");
include!("tests/storage.rs");
