use std::collections::hash_map::DefaultHasher;
use std::fs::{self, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::Path;
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

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
}

pub fn current_identity() -> RuntimeIdentity {
    if let Some(identity) = identity_from_current_state() {
        return identity;
    }

    fresh_identity()
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
    append_line(&paths::runtime_ledger_file(), &event.to_json_line())?;
    append_line(&paths::project_session_ledger_file(), &event.to_json_line())?;
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

    Ok(contents.lines().filter_map(parse_event_line).collect())
}

pub fn parse_event_line(line: &str) -> Option<ParsedLedgerEvent> {
    Some(ParsedLedgerEvent {
        event_id: extract_json_string(line, "event_id")?,
        ts_ms: extract_json_u128(line, "ts_ms")?,
        event_type: extract_json_string(line, "event_type")?,
        project_id: extract_json_string(line, "project_id")?,
        session_id: extract_json_string(line, "session_id")?,
        summary: extract_json_string(line, "summary")?,
    })
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
    })
}

fn identity_from_current_state() -> Option<RuntimeIdentity> {
    let path = paths::current_state_file();
    let contents = fs::read_to_string(path).ok()?;
    let project_root = paths::project_root().display().to_string();
    let mut hasher = DefaultHasher::new();
    project_root.hash(&mut hasher);
    let project_id = format!("project-{:016x}", hasher.finish());

    if extract_json_string_tolerant(&contents, "project_id")? != project_id {
        return None;
    }

    Some(RuntimeIdentity {
        project_id,
        session_id: extract_json_string_tolerant(&contents, "session_id")?,
        project_root,
    })
}

impl LedgerEvent {
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

fn extract_json_string(line: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":\"");
    let start = line.find(&needle)? + needle.len();
    let mut value = String::new();
    let mut escaped = false;

    for ch in line[start..].chars() {
        if escaped {
            match ch {
                '"' => value.push('"'),
                '\\' => value.push('\\'),
                'n' => value.push('\n'),
                'r' => value.push('\r'),
                't' => value.push('\t'),
                other => value.push(other),
            }
            escaped = false;
            continue;
        }

        match ch {
            '\\' => escaped = true,
            '"' => return Some(value),
            other => value.push(other),
        }
    }

    None
}

fn extract_json_string_tolerant(contents: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let key_start = contents.find(&needle)? + needle.len();
    let after_key = contents[key_start..].trim_start();
    let after_colon = after_key.strip_prefix(':')?.trim_start();
    let quoted = after_colon.strip_prefix('"')?;
    let mut value = String::new();
    let mut escaped = false;

    for ch in quoted.chars() {
        if escaped {
            match ch {
                '"' => value.push('"'),
                '\\' => value.push('\\'),
                'n' => value.push('\n'),
                'r' => value.push('\r'),
                't' => value.push('\t'),
                other => value.push(other),
            }
            escaped = false;
            continue;
        }

        match ch {
            '\\' => escaped = true,
            '"' => return Some(value),
            other => value.push(other),
        }
    }

    None
}

fn extract_json_u128(line: &str, key: &str) -> Option<u128> {
    let needle = format!("\"{key}\":");
    let start = line.find(&needle)? + needle.len();
    let value = line[start..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    value.parse().ok()
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
}
