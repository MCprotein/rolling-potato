use std::fs;
use std::path::Path;

use crate::foundation::error::AppError;
use crate::runtime_core::workflow::storage_compat::ledger::{
    canonical_event_line, sha256_bytes, LedgerEvent,
};

use super::super::append::append_line;
use super::chain::validate_ledger_contents;
use super::head::write_ledger_head;

pub(in crate::app::workflow_adapter::ledger) fn append_chained_event(
    path: &Path,
    event: &LedgerEvent,
) -> Result<(), AppError> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => {
            return Err(AppError::runtime(format!(
                "ledger append reread 실패: {err}"
            )))
        }
    };
    let existing = validate_ledger_contents(path, &contents)?;
    let previous = existing
        .last()
        .and_then(|entry| entry.event_hash.clone())
        .unwrap_or_else(|| {
            if contents.is_empty() {
                "root".to_string()
            } else {
                format!("legacy:{}", sha256_bytes(contents.as_bytes()))
            }
        });
    let (line, event_hash) = canonical_event_line(event, &previous);
    append_line(path, &line)?;
    write_ledger_head(path, existing.len() + 1, &event_hash)
}
