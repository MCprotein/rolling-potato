use std::fs;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use crate::adapters::filesystem::layout as paths;
use crate::foundation::error::AppError;
use crate::runtime_core::workflow::storage_compat::ledger::{
    event_physical_hash, parse_event_line_strict, LedgerBinding,
};

use super::super::{validate_read_only_event_sequence, ReadOnlyLedgerTail};
use super::head::{ledger_head_path, read_ledger_head_read_only};

pub(crate) fn read_runtime_tail_read_only(
    max_events: usize,
    max_bytes: u64,
) -> Result<ReadOnlyLedgerTail, AppError> {
    if max_events == 0 || max_bytes == 0 {
        return Err(AppError::blocked(
            "runtime ledger read-only budget은 0보다 커야 합니다.",
        ));
    }
    let path = paths::runtime_ledger_file();
    let head_path = ledger_head_path(&path);
    if !path.exists() && !head_path.exists() {
        return Ok(ReadOnlyLedgerTail {
            binding: LedgerBinding {
                event_count: 0,
                event_id: None,
                event_hash: "root".to_string(),
            },
            events: Vec::new(),
            truncated: false,
        });
    }
    ensure_read_only_regular_file(&path, "runtime ledger")?;
    ensure_read_only_regular_file(&head_path, "runtime ledger head")?;
    let head_before = read_ledger_head_read_only(&head_path)?;

    let mut file = fs::File::open(&path)
        .map_err(|err| AppError::blocked(format!("runtime ledger read-only open 실패: {err}")))?;
    let before = file
        .metadata()
        .map_err(|err| AppError::blocked(format!("runtime ledger metadata 실패: {err}")))?;
    let start = before.len().saturating_sub(max_bytes);
    file.seek(SeekFrom::Start(start))
        .map_err(|err| AppError::blocked(format!("runtime ledger tail seek 실패: {err}")))?;
    let mut bytes = Vec::new();
    (&mut file)
        .take(max_bytes)
        .read_to_end(&mut bytes)
        .map_err(|err| AppError::blocked(format!("runtime ledger tail 읽기 실패: {err}")))?;
    let truncated_legacy_genesis =
        start > 0 && read_ledger_genesis_is_legacy(&mut file, max_bytes)?;
    let after = fs::metadata(&path)
        .map_err(|err| AppError::blocked(format!("runtime ledger reread metadata 실패: {err}")))?;
    let head_after = read_ledger_head_read_only(&head_path)?;
    if before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
        || head_before != head_after
    {
        return Err(AppError::blocked(
            "runtime ledger read-only snapshot 중 canonical head가 변경되었습니다.",
        ));
    }
    if !bytes.is_empty() && !bytes.ends_with(b"\n") {
        return Err(AppError::blocked(
            "runtime ledger read-only tail이 완결된 JSONL record로 끝나지 않습니다.",
        ));
    }
    if start > 0 {
        let Some(boundary) = bytes.iter().position(|byte| *byte == b'\n') else {
            return Err(AppError::blocked(
                "runtime ledger record가 read-only byte budget을 초과했습니다.",
            ));
        };
        bytes.drain(..=boundary);
    }
    let body = std::str::from_utf8(&bytes)
        .map_err(|_| AppError::blocked("runtime ledger tail UTF-8 불일치"))?;
    let lines = body
        .lines()
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if head_before.event_count == 0 {
        if before.len() != 0 || !lines.is_empty() || head_before.event_hash != "root" {
            return Err(AppError::blocked(
                "runtime ledger empty head/file binding 불일치",
            ));
        }
        return Ok(ReadOnlyLedgerTail {
            binding: head_before,
            events: Vec::new(),
            truncated: false,
        });
    }
    let mut parsed_events = lines
        .iter()
        .map(|line| parse_event_line_strict(line))
        .collect::<Result<Vec<_>, _>>()?;
    validate_read_only_event_sequence(
        &lines,
        &parsed_events,
        start == 0,
        truncated_legacy_genesis,
    )?;
    let visible_event_count = u64::try_from(parsed_events.len())
        .map_err(|_| AppError::blocked("runtime ledger read-only event count overflow"))?;
    if (start == 0 && head_before.event_count != visible_event_count)
        || (start > 0 && head_before.event_count < visible_event_count)
    {
        return Err(AppError::blocked(
            "runtime ledger read-only tail/head event count 불일치",
        ));
    }
    let take = parsed_events.len().min(max_events);
    if take == 0 {
        return Err(AppError::blocked(
            "runtime ledger canonical tail이 read-only budget 안에 없습니다.",
        ));
    }
    let mut events = parsed_events.split_off(parsed_events.len() - take);
    let last = events
        .last()
        .ok_or_else(|| AppError::blocked("runtime ledger read-only tail 누락"))?;
    if last.event_hash.as_deref() != Some(head_before.event_hash.as_str())
        || head_before.event_count < visible_event_count
    {
        return Err(AppError::blocked(
            "runtime ledger read-only tail/head binding 불일치",
        ));
    }
    let binding = LedgerBinding {
        event_count: head_before.event_count,
        event_id: Some(last.event_id.clone()),
        event_hash: head_before.event_hash,
    };
    let truncated = binding.event_count > events.len() as u64;
    events.shrink_to_fit();
    Ok(ReadOnlyLedgerTail {
        binding,
        events,
        truncated,
    })
}

fn read_ledger_genesis_is_legacy(file: &mut fs::File, max_bytes: u64) -> Result<bool, AppError> {
    file.seek(SeekFrom::Start(0))
        .map_err(|err| AppError::blocked(format!("runtime ledger genesis seek 실패: {err}")))?;
    let mut line = Vec::new();
    BufReader::new(file.take(max_bytes))
        .read_until(b'\n', &mut line)
        .map_err(|err| AppError::blocked(format!("runtime ledger genesis 읽기 실패: {err}")))?;
    if !line.ends_with(b"\n") {
        return Err(AppError::blocked(
            "runtime ledger genesis record가 read-only byte budget을 초과했습니다.",
        ));
    }
    line.pop();
    let body = std::str::from_utf8(&line)
        .map_err(|_| AppError::blocked("runtime ledger genesis UTF-8 불일치"))?;
    let event = parse_event_line_strict(body)?;
    match (
        event.previous_event_hash.as_deref(),
        event.event_hash.as_deref(),
    ) {
        (None, None) => Ok(true),
        (Some("root"), Some(hash)) if hash == event_physical_hash(&event, "root") => Ok(false),
        (Some(_), Some(_)) => Err(AppError::blocked(
            "runtime ledger read-only genesis hash chain 불일치",
        )),
        _ => Err(AppError::blocked(
            "runtime ledger read-only genesis chain field 조합 불일치",
        )),
    }
}

fn ensure_read_only_regular_file(path: &Path, label: &str) -> Result<(), AppError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|err| AppError::blocked(format!("{label} metadata 실패: {err}")))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AppError::blocked(format!(
            "{label} read-only file boundary 불일치"
        )));
    }
    Ok(())
}
