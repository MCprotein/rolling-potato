use std::path::Path;

use crate::foundation::error::AppError;
use crate::runtime_core::workflow::storage_compat::ledger::{
    event_physical_hash, parse_event_line_strict, sha256_bytes, ParsedLedgerEvent,
};

use super::diagnostics::ledger_corrupt;
use super::head::validate_ledger_head;

pub(in crate::app::workflow_adapter::ledger) fn validate_ledger_contents(
    path: &Path,
    contents: &str,
) -> Result<Vec<ParsedLedgerEvent>, AppError> {
    validate_ledger_contents_inner(path, contents, false)
}

pub(super) fn validate_ledger_contents_with_head_repair(
    path: &Path,
    contents: &str,
) -> Result<Vec<ParsedLedgerEvent>, AppError> {
    validate_ledger_contents_inner(path, contents, true)
}

fn validate_ledger_contents_inner(
    path: &Path,
    contents: &str,
    allow_head_repair: bool,
) -> Result<Vec<ParsedLedgerEvent>, AppError> {
    let mut events = Vec::new();
    let mut legacy_prefix = String::new();
    let mut previous_hash: Option<String> = None;
    for (index, line) in contents.lines().enumerate() {
        if line.trim().is_empty() {
            return Err(ledger_corrupt(path, index + 1, "빈 JSONL record"));
        }
        let event = parse_event_line_strict(line)
            .map_err(|_| ledger_corrupt(path, index + 1, "malformed JSONL record"))?;
        match (&event.previous_event_hash, &event.event_hash) {
            (None, None) if previous_hash.is_none() => {
                legacy_prefix.push_str(line);
                legacy_prefix.push('\n');
            }
            (Some(previous), Some(hash)) => {
                let expected_previous = previous_hash.clone().unwrap_or_else(|| {
                    if legacy_prefix.is_empty() {
                        "root".to_string()
                    } else {
                        format!("legacy:{}", sha256_bytes(legacy_prefix.as_bytes()))
                    }
                });
                if previous != &expected_previous || hash != &event_physical_hash(&event, previous)
                {
                    return Err(ledger_corrupt(
                        path,
                        index + 1,
                        "physical hash chain 불일치",
                    ));
                }
                previous_hash = Some(hash.clone());
            }
            _ => {
                return Err(ledger_corrupt(
                    path,
                    index + 1,
                    "legacy event가 chained suffix 뒤에 존재함",
                ))
            }
        }
        events.push(event);
    }
    validate_ledger_head(
        path,
        &events,
        previous_hash.as_deref(),
        &legacy_prefix,
        allow_head_repair,
    )?;
    Ok(events)
}

pub(in crate::app::workflow_adapter::ledger) fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
