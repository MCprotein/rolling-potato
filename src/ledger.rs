use std::collections::hash_map::DefaultHasher;
use std::fs::{self, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::Path;
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use crate::app::AppError;
use crate::paths;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeIdentity {
    pub project_id: String,
    pub session_id: String,
    pub project_root: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerEvent {
    pub event_id: String,
    pub ts_ms: u128,
    pub event_type: String,
    pub project_id: String,
    pub session_id: String,
    pub summary: String,
    pub details: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedLedgerEvent {
    pub event_id: String,
    pub ts_ms: u128,
    pub event_type: String,
    pub project_id: String,
    pub session_id: String,
    pub summary: String,
    pub details: String,
    pub previous_event_hash: Option<String>,
    pub event_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowCheckpoint {
    pub revision: u64,
    pub artifact_hash: String,
    pub previous_hash: String,
}

pub fn current_identity() -> RuntimeIdentity {
    validated_current_identity().unwrap_or_else(|_| fresh_identity())
}

pub fn validated_current_identity() -> Result<RuntimeIdentity, AppError> {
    let path = paths::current_state_file();
    if !path.exists() {
        return Ok(fresh_identity());
    }
    let contents = fs::read_to_string(&path)
        .map_err(|err| AppError::blocked(format!("current-state identity 읽기 실패: {err}")))?;
    let object = crate::strict_json::parse_object(
        &contents,
        &[
            "schema_version",
            "project_id",
            "project_root",
            "session_id",
            "active_workflow",
            "parent_session_id",
            "branch_from_event_id",
            "compaction_boundary",
            "resume_source",
            "terminal_states",
        ],
        "current-state identity",
    )?;
    if crate::strict_json::number(&object, "schema_version", "current-state identity")? != 1 {
        return Err(AppError::blocked("current-state identity schema 불일치"));
    }
    let fresh = fresh_identity();
    let project_id = crate::strict_json::string(&object, "project_id", "current-state identity")?;
    let project_root =
        crate::strict_json::string(&object, "project_root", "current-state identity")?;
    if project_id != fresh.project_id || project_root != fresh.project_root {
        return Err(AppError::blocked(
            "current-state identity project binding 불일치",
        ));
    }
    if !matches!(
        object.get("terminal_states"),
        Some(crate::strict_json::Value::Array(_))
    ) {
        return Err(AppError::blocked(
            "current-state terminal_states type 불일치",
        ));
    }
    Ok(RuntimeIdentity {
        project_id,
        session_id: crate::strict_json::string(&object, "session_id", "current-state identity")?,
        project_root,
    })
}

pub fn fresh_identity() -> RuntimeIdentity {
    let project_root = paths::project_root().display().to_string();
    let mut hasher = DefaultHasher::new();
    project_root.hash(&mut hasher);
    let project_id = format!("project-{:016x}", hasher.finish());
    let session_id = format!("session-{}-{}", now_ms(), process::id());

    RuntimeIdentity {
        project_id,
        session_id,
        project_root,
    }
}

pub fn new_event_for(
    identity: &RuntimeIdentity,
    event_type: &str,
    summary: &str,
    details: &str,
) -> LedgerEvent {
    let ts_ms = now_ms();
    let event_id = format!(
        "event-{}-{}-{}",
        now_nanos(),
        process::id(),
        sanitize_event_type(event_type)
    );

    LedgerEvent {
        event_id,
        ts_ms,
        event_type: event_type.to_string(),
        project_id: identity.project_id.clone(),
        session_id: identity.session_id.clone(),
        summary: summary.to_string(),
        details: redact_text(details),
    }
}

pub fn append_event(event: &LedgerEvent) -> Result<(), AppError> {
    append_chained_event(&paths::runtime_ledger_file(), event)?;
    append_chained_event(&paths::project_session_ledger_file(), event)?;
    append_line(&paths::operation_log_file(), &event.to_log_line())?;
    Ok(())
}

pub fn read_runtime_events() -> Result<Vec<ParsedLedgerEvent>, AppError> {
    let path = paths::runtime_ledger_file();
    if !path.exists() {
        return Ok(Vec::new());
    }

    let contents = fs::read_to_string(&path).map_err(|err| {
        AppError::runtime(format!(
            "runtime ledger를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;

    validate_ledger_contents(&path, &contents)
}

fn validate_ledger_contents(
    path: &Path,
    contents: &str,
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
    validate_ledger_head(path, events.len(), previous_hash.as_deref(), &legacy_prefix)?;
    Ok(events)
}

#[cfg(test)]
pub fn parse_event_line(line: &str) -> Option<ParsedLedgerEvent> {
    parse_event_line_strict(line).ok()
}

fn parse_event_line_strict(line: &str) -> Result<ParsedLedgerEvent, AppError> {
    const KEYS: &[&str] = &[
        "schema_version",
        "event_id",
        "ts_ms",
        "event_type",
        "project_id",
        "session_id",
        "summary",
        "details",
        "previous_event_hash",
        "event_hash",
    ];
    let object = crate::strict_json::parse_object(line, KEYS, "runtime ledger line")?;
    let schema = crate::strict_json::number(&object, "schema_version", "runtime ledger line")?;
    if !matches!(schema, 1 | 2) {
        return Err(AppError::blocked("runtime ledger schema version 불일치"));
    }
    let (previous_event_hash, event_hash) = if schema == 2 {
        (
            Some(crate::strict_json::string(
                &object,
                "previous_event_hash",
                "runtime ledger line",
            )?),
            Some(crate::strict_json::string(
                &object,
                "event_hash",
                "runtime ledger line",
            )?),
        )
    } else {
        if object.contains_key("previous_event_hash") || object.contains_key("event_hash") {
            return Err(AppError::blocked("legacy ledger에 chain field가 존재함"));
        }
        (None, None)
    };
    Ok(ParsedLedgerEvent {
        event_id: crate::strict_json::string(&object, "event_id", "runtime ledger line")?,
        ts_ms: crate::strict_json::number(&object, "ts_ms", "runtime ledger line")? as u128,
        event_type: crate::strict_json::string(&object, "event_type", "runtime ledger line")?,
        project_id: crate::strict_json::string(&object, "project_id", "runtime ledger line")?,
        session_id: crate::strict_json::string(&object, "session_id", "runtime ledger line")?,
        summary: crate::strict_json::string(&object, "summary", "runtime ledger line")?,
        details: crate::strict_json::string(&object, "details", "runtime ledger line")?,
        previous_event_hash,
        event_hash,
    })
}

fn append_chained_event(path: &Path, event: &LedgerEvent) -> Result<(), AppError> {
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
    let payload = event_chain_payload(event, &previous);
    let event_hash = sha256_bytes(payload.as_bytes());
    let line = format!(
        "{{{},\"event_hash\":\"{}\"}}",
        payload.trim_start_matches('{').trim_end_matches('}'),
        event_hash
    );
    append_line(path, &line)?;
    write_ledger_head(path, existing.len() + 1, &event_hash)
}

fn event_chain_payload(event: &LedgerEvent, previous: &str) -> String {
    format!(
        "{{\"schema_version\":2,\"event_id\":\"{}\",\"ts_ms\":{},\"event_type\":\"{}\",\"project_id\":\"{}\",\"session_id\":\"{}\",\"summary\":\"{}\",\"details\":\"{}\",\"previous_event_hash\":\"{}\"}}",
        json_string(&event.event_id), event.ts_ms, json_string(&event.event_type),
        json_string(&event.project_id), json_string(&event.session_id), json_string(&event.summary),
        json_string(&event.details), previous
    )
}

fn event_physical_hash(event: &ParsedLedgerEvent, previous: &str) -> String {
    let synthetic = LedgerEvent {
        event_id: event.event_id.clone(),
        ts_ms: event.ts_ms,
        event_type: event.event_type.clone(),
        project_id: event.project_id.clone(),
        session_id: event.session_id.clone(),
        summary: event.summary.clone(),
        details: event.details.clone(),
    };
    sha256_bytes(event_chain_payload(&synthetic, previous).as_bytes())
}

fn ledger_head_path(path: &Path) -> std::path::PathBuf {
    path.with_extension(format!(
        "{}.head",
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or("ledger")
    ))
}

fn write_ledger_head(path: &Path, count: usize, hash: &str) -> Result<(), AppError> {
    let body = format!(
        "{{\"schema_version\":1,\"event_count\":{count},\"last_event_hash\":\"{hash}\"}}\n"
    );
    crate::state::atomic_replace_bytes(&ledger_head_path(path), body.as_bytes())
}

fn validate_ledger_head(
    path: &Path,
    count: usize,
    last_hash: Option<&str>,
    legacy_prefix: &str,
) -> Result<(), AppError> {
    let head_path = ledger_head_path(path);
    if !head_path.exists() {
        if last_hash.is_some() {
            return Err(ledger_corrupt(path, count, "chained ledger head 누락"));
        }
        return Ok(());
    }
    let body = fs::read_to_string(&head_path)
        .map_err(|err| AppError::blocked(format!("ledger head 읽기 실패: {err}")))?;
    let object = crate::strict_json::parse_object(
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
    if crate::strict_json::number(&object, "schema_version", "ledger head")? != 1
        || crate::strict_json::number(&object, "event_count", "ledger head")? != count as u64
        || crate::strict_json::string(&object, "last_event_hash", "ledger head")? != expected_hash
    {
        return Err(ledger_corrupt(path, count, "ledger truncation/head 불일치"));
    }
    Ok(())
}

fn ledger_corrupt(path: &Path, line: usize, reason: &str) -> AppError {
    let gap = crate::state::record_validation_gap(
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

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn event_detail_exists(event_type: &str, field: &str, value: &str) -> Result<bool, AppError> {
    Ok(read_runtime_events()?.iter().any(|event| {
        event.event_type == event_type && detail_value(&event.details, field) == Some(value)
    }))
}

pub fn workflow_checkpoint_exists(
    workflow_id: &str,
    revision: u64,
    artifact_hash: &str,
) -> Result<bool, AppError> {
    Ok(workflow_checkpoints(workflow_id)?.iter().any(|checkpoint| {
        checkpoint.revision == revision && checkpoint.artifact_hash == artifact_hash
    }))
}

pub fn workflow_checkpoints(workflow_id: &str) -> Result<Vec<WorkflowCheckpoint>, AppError> {
    let mut checkpoints = Vec::new();
    for event in read_runtime_events()? {
        if event.event_type != "workflow.checkpoint"
            || detail_value(&event.details, "workflow_id") != Some(workflow_id)
        {
            continue;
        }
        let revision = detail_value(&event.details, "revision")
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or_else(|| malformed_checkpoint(&event.event_id))?;
        let artifact_hash = detail_value(&event.details, "artifact_hash")
            .filter(|value| is_sha256(value))
            .ok_or_else(|| malformed_checkpoint(&event.event_id))?
            .to_string();
        let previous_hash = detail_value(&event.details, "previous_hash")
            .filter(|value| *value == "none" || is_sha256(value))
            .ok_or_else(|| malformed_checkpoint(&event.event_id))?
            .to_string();
        checkpoints.push(WorkflowCheckpoint {
            revision,
            artifact_hash,
            previous_hash,
        });
    }
    checkpoints.sort_by_key(|checkpoint| checkpoint.revision);
    for (index, checkpoint) in checkpoints.iter().enumerate() {
        let expected_revision = index as u64 + 1;
        let expected_previous = if index == 0 {
            "none"
        } else {
            checkpoints[index - 1].artifact_hash.as_str()
        };
        if checkpoint.revision != expected_revision || checkpoint.previous_hash != expected_previous
        {
            return Err(AppError::blocked(format!(
                "workflow ledger chain 검증 차단\n- workflow id: {workflow_id}\n- revision: {}\n- 이유: latest checkpoint 또는 previous_hash chain 불일치",
                checkpoint.revision
            )));
        }
    }
    Ok(checkpoints)
}

fn detail_value<'a>(details: &'a str, key: &str) -> Option<&'a str> {
    details.split_whitespace().find_map(|field| {
        let (candidate, value) = field.split_once('=')?;
        (candidate == key).then_some(value)
    })
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn malformed_checkpoint(event_id: &str) -> AppError {
    AppError::blocked(format!(
        "workflow ledger checkpoint 검증 차단\n- event id: {event_id}\n- 이유: required checkpoint field가 malformed입니다."
    ))
}

pub fn redact_text(value: &str) -> String {
    let sensitive_keys = [
        "api_key",
        "apikey",
        "authorization",
        "bearer",
        "password",
        "secret",
        "token",
    ];

    value
        .split_whitespace()
        .map(|part| {
            let lower = part.to_ascii_lowercase();
            if sensitive_keys.iter().any(|key| lower.contains(key)) {
                "[REDACTED]".to_string()
            } else {
                part.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn append_line(path: &Path, line: &str) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            AppError::runtime(format!(
                "디렉터리를 만들지 못했습니다: {} ({err})",
                parent.display()
            ))
        })?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| {
            AppError::runtime(format!(
                "파일을 열지 못했습니다: {} ({err})",
                path.display()
            ))
        })?;

    writeln!(file, "{line}").map_err(|err| {
        AppError::runtime(format!(
            "파일에 기록하지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    file.sync_all()
        .map_err(|err| AppError::runtime(format!("ledger sync 실패: {} ({err})", path.display())))
}

impl LedgerEvent {
    #[cfg(test)]
    pub fn to_json_line(&self) -> String {
        format!(
            "{{\"schema_version\":1,\"event_id\":\"{}\",\"ts_ms\":{},\"event_type\":\"{}\",\"project_id\":\"{}\",\"session_id\":\"{}\",\"summary\":\"{}\",\"details\":\"{}\"}}",
            json_string(&self.event_id),
            self.ts_ms,
            json_string(&self.event_type),
            json_string(&self.project_id),
            json_string(&self.session_id),
            json_string(&self.summary),
            json_string(&self.details)
        )
    }

    fn to_log_line(&self) -> String {
        format!(
            "{} {} {} {}",
            self.ts_ms, self.event_type, self.session_id, self.summary
        )
    }
}

pub fn json_string(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            c if c.is_control() => escaped.push_str(&format!("\\u{:04x}", c as u32)),
            c => escaped.push(c),
        }
    }
    escaped
}

fn sanitize_event_type(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ledger_event_json_round_trip_for_projection_fields() {
        let event = LedgerEvent {
            event_id: "event-1".to_string(),
            ts_ms: 42,
            event_type: "runtime.init".to_string(),
            project_id: "project-a".to_string(),
            session_id: "session-a".to_string(),
            summary: "초기화".to_string(),
            details: "safe".to_string(),
        };

        let parsed = parse_event_line(&event.to_json_line()).unwrap();

        assert_eq!(parsed.event_id, "event-1");
        assert_eq!(parsed.ts_ms, 42);
        assert_eq!(parsed.event_type, "runtime.init");
        assert_eq!(parsed.project_id, "project-a");
        assert_eq!(parsed.session_id, "session-a");
        assert_eq!(parsed.summary, "초기화");
    }

    #[test]
    fn redacts_sensitive_words_before_persistence() {
        let redacted = redact_text("token=abc safe password=hunter2");
        assert_eq!(redacted, "[REDACTED] safe [REDACTED]");
    }

    #[test]
    fn malformed_runtime_ledger_line_fails_closed() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root =
            std::env::temp_dir().join(format!("rpotato-ledger-malformed-{}", std::process::id()));
        std::env::set_var("RPOTATO_DATA_HOME", &root);
        fs::create_dir_all(paths::state_dir()).unwrap();
        fs::write(paths::runtime_ledger_file(), "{partial\n").unwrap();

        let error = read_runtime_events().unwrap_err();

        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);
        assert_eq!(error.code, 3);
        assert!(error.message.contains("malformed JSONL"));
    }

    #[test]
    fn workflow_checkpoint_previous_hash_chain_is_strict() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root =
            std::env::temp_dir().join(format!("rpotato-ledger-chain-{}", std::process::id()));
        std::env::set_var("RPOTATO_DATA_HOME", &root);
        std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        fs::create_dir_all(paths::project_state_dir()).unwrap();
        let identity = fresh_identity();
        let first_hash = "a".repeat(64);
        let second_hash = "b".repeat(64);
        let first = new_event_for(
            &identity,
            "workflow.checkpoint",
            "first",
            &format!(
                "workflow_id=workflow-chain revision=1 artifact_hash={first_hash} previous_hash=none phase=model-pending action_id=action proposal_id=none evidence_id=none"
            ),
        );
        let stale = new_event_for(
            &identity,
            "workflow.checkpoint",
            "stale",
            &format!(
                "workflow_id=workflow-chain revision=2 artifact_hash={second_hash} previous_hash={} phase=approved action_id=action proposal_id=none evidence_id=none",
                "c".repeat(64)
            ),
        );
        append_event(&first).unwrap();
        append_event(&stale).unwrap();

        let error = workflow_checkpoints("workflow-chain").unwrap_err();

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        let _ = fs::remove_dir_all(root);
        assert_eq!(error.code, 3);
        assert!(error.message.contains("previous_hash chain"));
    }

    #[test]
    fn physical_chain_reorder_and_truncation_fail_closed() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        for mode in ["reorder", "truncate"] {
            let root = std::env::temp_dir().join(format!(
                "rpotato-ledger-physical-{mode}-{}",
                std::process::id()
            ));
            std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
            std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
            let identity = fresh_identity();
            append_event(&new_event_for(&identity, "one", "하나", "safe")).unwrap();
            append_event(&new_event_for(&identity, "two", "둘", "safe")).unwrap();
            let path = paths::runtime_ledger_file();
            let body = fs::read_to_string(&path).unwrap();
            let mut lines = body.lines().collect::<Vec<_>>();
            if mode == "reorder" {
                lines.swap(0, 1);
            } else {
                lines.pop();
            }
            fs::write(&path, format!("{}\n", lines.join("\n"))).unwrap();
            assert!(read_runtime_events().is_err(), "mode: {mode}");
            std::env::remove_var("RPOTATO_DATA_HOME");
            std::env::remove_var("RPOTATO_PROJECT_ROOT");
            let _ = fs::remove_dir_all(root);
        }
    }
}
