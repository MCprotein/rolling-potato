use std::fs;
use std::path::{Path, PathBuf};

use crate::foundation::error::AppError;
use crate::foundation::serialization as strict_json;
use crate::runtime_core::workflow::storage_compat::ledger::{
    sha256_bytes, LedgerBinding, ParsedLedgerEvent,
};

use super::diagnostics::ledger_corrupt;

pub(in crate::app::workflow_adapter::ledger) fn ledger_head_path(path: &Path) -> PathBuf {
    path.with_extension(format!(
        "{}.head",
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or("ledger")
    ))
}

pub(super) fn read_ledger_head_read_only(path: &Path) -> Result<LedgerBinding, AppError> {
    let metadata = fs::metadata(path)
        .map_err(|err| AppError::blocked(format!("runtime ledger head metadata 실패: {err}")))?;
    if metadata.len() > 4_096 {
        return Err(AppError::blocked("runtime ledger head byte limit 초과"));
    }
    let body = fs::read_to_string(path)
        .map_err(|err| AppError::blocked(format!("runtime ledger head 읽기 실패: {err}")))?;
    let object = strict_json::parse_canonical_object(
        body.trim_end_matches('\n'),
        &["schema_version", "event_count", "last_event_hash"],
        "runtime ledger read-only head",
    )?;
    if strict_json::canonical_u64(&object, "schema_version", "runtime ledger read-only head")? != 1
    {
        return Err(AppError::blocked("runtime ledger head schema 불일치"));
    }
    let event_count =
        strict_json::canonical_u64(&object, "event_count", "runtime ledger read-only head")?;
    let event_hash = match object.get("last_event_hash") {
        Some(strict_json::CanonicalValue::String(value)) => value.clone(),
        _ => return Err(AppError::blocked("runtime ledger head hash type 불일치")),
    };
    if event_hash != "root" && !super::chain::is_sha256(&event_hash) {
        return Err(AppError::blocked("runtime ledger head hash 형식 불일치"));
    }
    Ok(LedgerBinding {
        event_count,
        event_id: None,
        event_hash,
    })
}

pub(in crate::app::workflow_adapter::ledger) fn write_ledger_head(
    path: &Path,
    count: usize,
    hash: &str,
) -> Result<(), AppError> {
    let body = format!(
        "{{\"schema_version\":1,\"event_count\":{count},\"last_event_hash\":\"{hash}\"}}\n"
    );
    crate::adapters::filesystem::atomic_write::atomic_replace_bytes(
        &ledger_head_path(path),
        body.as_bytes(),
    )
}

pub(super) fn validate_ledger_head(
    path: &Path,
    events: &[ParsedLedgerEvent],
    last_hash: Option<&str>,
    legacy_prefix: &str,
    allow_repair: bool,
) -> Result<(), AppError> {
    let count = events.len();
    let head_path = ledger_head_path(path);
    if !head_path.exists() {
        if let Some(last_hash) = last_hash {
            let chained_count = events
                .iter()
                .filter(|event| event.event_hash.is_some())
                .count();
            if allow_repair && chained_count == 1 {
                write_ledger_head(path, count, last_hash)?;
                return Ok(());
            }
            return Err(ledger_corrupt(path, count, "chained ledger head 누락"));
        }
        return Ok(());
    }
    let body = fs::read_to_string(&head_path)
        .map_err(|err| AppError::blocked(format!("ledger head 읽기 실패: {err}")))?;
    let object = strict_json::parse_object(
        &body,
        &["schema_version", "event_count", "last_event_hash"],
        "ledger head",
    )?;
    let expected_hash = last_hash.unwrap_or({
        if legacy_prefix.is_empty() {
            "root"
        } else {
            "legacy"
        }
    });
    let schema = strict_json::number(&object, "schema_version", "ledger head")?;
    let head_count = strict_json::number(&object, "event_count", "ledger head")?;
    let head_hash = strict_json::string(&object, "last_event_hash", "ledger head")?;
    if schema == 1 && head_count == count as u64 && head_hash == expected_hash {
        return Ok(());
    }
    if schema == 1 && allow_repair && head_count.checked_add(1) == Some(count as u64) {
        let chained_count = events
            .iter()
            .filter(|event| event.event_hash.is_some())
            .count();
        let previous = events
            .last()
            .and_then(|event| event.previous_event_hash.as_deref());
        let legacy_anchor = (!legacy_prefix.is_empty())
            .then(|| format!("legacy:{}", sha256_bytes(legacy_prefix.as_bytes())));
        let predecessor_matches = previous == Some(head_hash.as_str())
            || (chained_count == 1
                && head_hash == "legacy"
                && previous == legacy_anchor.as_deref());
        if predecessor_matches {
            write_ledger_head(path, count, expected_hash)?;
            return Ok(());
        }
    }
    Err(ledger_corrupt(path, count, "ledger truncation/head 불일치"))
}
