use std::fs;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Duration;

use crate::adapters::filesystem::{layout as paths, lease};
use crate::foundation::error::AppError;
use crate::foundation::serialization as strict_json;
use crate::runtime_core::workflow::storage_compat::ledger::{
    append_canonical_event, event_physical_hash, parse_event_line_strict, sha256_bytes,
    LedgerBinding, LedgerEvent, ParsedLedgerEvent,
};

use super::ReadOnlyLedgerTail;

pub fn read_runtime_events() -> Result<Vec<ParsedLedgerEvent>, AppError> {
    let _reader = lease::RecoverableLease::acquire_with_wait(
        paths::runtime_ledger_writer_lock(),
        "runtime ledger reader",
        Duration::from_secs(5),
    )?;
    read_runtime_events_unlocked()
}

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

fn validate_read_only_event_sequence(
    lines: &[&str],
    events: &[ParsedLedgerEvent],
    starts_at_file_beginning: bool,
    truncated_legacy_genesis: bool,
) -> Result<(), AppError> {
    if truncated_legacy_genesis {
        return Err(AppError::blocked(
            "runtime ledger legacy prefix가 read-only byte budget 안에 없습니다.",
        ));
    }
    let mut legacy_prefix = String::new();
    let mut previous_hash: Option<&str> = None;
    for (line, event) in lines.iter().zip(events) {
        match (
            event.previous_event_hash.as_deref(),
            event.event_hash.as_deref(),
        ) {
            (None, None) if previous_hash.is_none() => {
                if !starts_at_file_beginning {
                    return Err(AppError::blocked(
                        "runtime ledger legacy prefix가 read-only byte budget 안에 없습니다.",
                    ));
                }
                legacy_prefix.push_str(line);
                legacy_prefix.push('\n');
            }
            (Some(previous), Some(hash)) => {
                if hash != event_physical_hash(event, previous) {
                    return Err(AppError::blocked(
                        "runtime ledger read-only physical hash chain 불일치",
                    ));
                }
                let predecessor_matches = if let Some(expected) = previous_hash {
                    previous == expected
                } else if starts_at_file_beginning {
                    let expected = if legacy_prefix.is_empty() {
                        "root".to_string()
                    } else {
                        format!("legacy:{}", sha256_bytes(legacy_prefix.as_bytes()))
                    };
                    previous == expected
                } else {
                    true
                };
                if !predecessor_matches {
                    return Err(AppError::blocked(
                        "runtime ledger read-only adjacent hash chain 불일치",
                    ));
                }
                previous_hash = Some(hash);
            }
            (None, None) => {
                return Err(AppError::blocked(
                    "runtime ledger read-only legacy event가 chained suffix 뒤에 존재합니다.",
                ));
            }
            _ => {
                return Err(AppError::blocked(
                    "runtime ledger read-only chain field 조합 불일치",
                ));
            }
        }
    }
    Ok(())
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

fn read_ledger_head_read_only(path: &Path) -> Result<LedgerBinding, AppError> {
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
    if event_hash != "root" && !is_sha256(&event_hash) {
        return Err(AppError::blocked("runtime ledger head hash 형식 불일치"));
    }
    Ok(LedgerBinding {
        event_count,
        event_id: None,
        event_hash,
    })
}

pub(super) fn read_runtime_events_unlocked() -> Result<Vec<ParsedLedgerEvent>, AppError> {
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

pub(super) fn validate_ledger_contents(
    path: &Path,
    contents: &str,
) -> Result<Vec<ParsedLedgerEvent>, AppError> {
    validate_ledger_contents_inner(path, contents, false)
}

fn validate_ledger_contents_with_head_repair(
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

pub(super) fn append_chained_event(path: &Path, event: &LedgerEvent) -> Result<(), AppError> {
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
    let event_hash = append_canonical_event(path, event, &previous)?;
    write_ledger_head(path, existing.len() + 1, &event_hash)
}

pub(super) fn ledger_head_path(path: &Path) -> std::path::PathBuf {
    path.with_extension(format!(
        "{}.head",
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or("ledger")
    ))
}

pub(super) fn write_ledger_head(path: &Path, count: usize, hash: &str) -> Result<(), AppError> {
    let body = format!(
        "{{\"schema_version\":1,\"event_count\":{count},\"last_event_hash\":\"{hash}\"}}\n"
    );
    crate::adapters::filesystem::atomic_write::atomic_replace_bytes(
        &ledger_head_path(path),
        body.as_bytes(),
    )
}

fn validate_ledger_head(
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

fn ledger_corrupt(path: &Path, line: usize, reason: &str) -> AppError {
    let gap = crate::app::workflow_adapter::state::record_validation_gap(
        "corrupt-ledger",
        &format!("{}:{line}:{reason}", path.display()),
    );
    let suffix = gap
        .err()
        .map(|err| format!("\n- validation-gap 저장 실패: {}", err.message))
        .unwrap_or_default();
    AppError::blocked(format!(
        "runtime ledger 검증 차단\n- 이유: {reason}\n- path: {}\n- line: {line}{suffix}",
        path.display()
    ))
}

pub(super) fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
