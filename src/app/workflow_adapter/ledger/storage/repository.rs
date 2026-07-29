use std::fs;
use std::time::Duration;

use crate::adapters::filesystem::{layout as paths, lease};
use crate::foundation::error::AppError;
use crate::runtime_core::workflow::storage_compat::ledger::ParsedLedgerEvent;

use super::chain::validate_ledger_contents_with_head_repair;
use super::diagnostics::ledger_corrupt;
use super::head::ledger_head_path;

pub fn read_runtime_events() -> Result<Vec<ParsedLedgerEvent>, AppError> {
    let _reader = lease::RecoverableLease::acquire_with_wait(
        paths::runtime_ledger_writer_lock(),
        "runtime ledger reader",
        Duration::from_secs(5),
    )?;
    read_runtime_events_unlocked()
}

pub(in crate::app::workflow_adapter::ledger) fn read_runtime_events_unlocked(
) -> Result<Vec<ParsedLedgerEvent>, AppError> {
    let path = paths::runtime_ledger_file();
    if !path.exists() {
        if ledger_head_path(&path).exists() {
            return Err(ledger_corrupt(
                &path,
                0,
                "ledger JSONL 없이 orphan head가 존재합니다",
            ));
        }
        return Ok(Vec::new());
    }

    let contents = fs::read_to_string(&path).map_err(|err| {
        AppError::runtime(format!(
            "runtime ledger를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;

    validate_ledger_contents_with_head_repair(&path, &contents)
}
