use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::app::AppError;
use crate::context::SourcePointer;
use crate::ledger::{self, ParsedLedgerEvent, RuntimeIdentity};
use crate::{observability, paths, state};

const TRANSCRIPT_SCHEMA_VERSION: u64 = 1;
const MAX_TRANSCRIPT_CONTENT_CHARS: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptSourcePointer {
    pub stable_ref: String,
    pub path: String,
    pub source_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptRecord {
    pub record_id: String,
    pub project_id: String,
    pub session_id: String,
    pub workflow_id: String,
    pub kind: String,
    pub causal_id: String,
    pub content: String,
    pub content_hash: String,
    pub source_pointers: Vec<TranscriptSourcePointer>,
    pub recorded_at_ms: u128,
    pub artifact_hash: String,
}

pub fn record_workflow_turn(
    workflow: &state::WorkflowRecord,
    kind: &str,
    causal_id: &str,
    content: &str,
    source_pointers: &[SourcePointer],
) -> Result<TranscriptRecord, AppError> {
    validate_kind(kind)?;
    validate_id("project id", &workflow.project_id)?;
    validate_id("workflow id", &workflow.workflow_id)?;
    validate_id("session id", &workflow.session_id)?;
    validate_id("causal id", causal_id)?;
    if content.trim().is_empty() {
        return Err(AppError::blocked("transcript content가 비어 있습니다."));
    }
    if content.chars().count() > MAX_TRANSCRIPT_CONTENT_CHARS {
        return Err(AppError::blocked(format!(
            "transcript content 저장 차단\n- 최대 문자 수: {MAX_TRANSCRIPT_CONTENT_CHARS}"
        )));
    }

    let record_id = format!(
        "transcript-{}",
        &state::sha256_text(&format!(
            "{}\n{}\n{}\n{}\n{}",
            workflow.project_id, workflow.session_id, workflow.workflow_id, kind, causal_id
        ))[..24]
    );
    let path =
        validated_transcript_path(&workflow.project_id, &workflow.session_id, &record_id, true)?;
    let _lease = crate::lease::RecoverableLease::acquire(
        path.with_extension("checkpoint.lock"),
        "transcript checkpoint",
    )?;
    let pointers = source_pointers
        .iter()
        .map(|pointer| {
            let pointer = TranscriptSourcePointer {
                stable_ref: pointer.stable_ref.clone(),
                path: pointer.path.clone(),
                source_hash: pointer.fingerprint.clone(),
            };
            validate_source_pointer(&pointer)?;
            Ok(pointer)
        })
        .collect::<Result<Vec<_>, AppError>>()?;

    if path.exists() {
        let existing = load_record_path(&path)?;
        validate_expected_record(&existing, workflow, kind, causal_id, content, &pointers)?;
        ensure_ledger_event(&existing)?;
        return Ok(existing);
    }

    let mut record = TranscriptRecord {
        record_id,
        project_id: workflow.project_id.clone(),
        session_id: workflow.session_id.clone(),
        workflow_id: workflow.workflow_id.clone(),
        kind: kind.to_string(),
        causal_id: causal_id.to_string(),
        content: content.to_string(),
        content_hash: state::sha256_text(content),
        source_pointers: pointers,
        recorded_at_ms: now_ms(),
        artifact_hash: String::new(),
    };
    record.artifact_hash = state::sha256_text(&record.artifact_payload());
    state::atomic_replace_bytes(&path, record.to_json().as_bytes())?;
    ensure_ledger_event(&record)?;
    Ok(record)
}

pub fn records_for_session(session_id: &str) -> Result<Vec<TranscriptRecord>, AppError> {
    validate_id("session id", session_id)?;
    let identity = ledger::validated_current_identity()?;
    let mut records = Vec::new();
    let mut seen = BTreeSet::new();
    for event in ledger::read_runtime_events()? {
        if event.project_id != identity.project_id
            || event.session_id != session_id
            || event.event_type != "transcript.recorded"
        {
            continue;
        }
        let record = record_from_event(&event)?;
        if !seen.insert(record.record_id.clone()) {
            return Err(AppError::blocked(format!(
                "transcript replay 차단\n- 이유: duplicate canonical record event\n- record id: {}",
                record.record_id
            )));
        }
        records.push(record);
    }
    Ok(records)
}

pub fn record_from_event(event: &ParsedLedgerEvent) -> Result<TranscriptRecord, AppError> {
    record_from_binding(
        &event.project_id,
        &event.session_id,
        &event.event_type,
        &event.details,
    )
}

pub fn record_from_binding(
    project_id: &str,
    session_id: &str,
    event_type: &str,
    details: &str,
) -> Result<TranscriptRecord, AppError> {
    if event_type != "transcript.recorded" {
        return Err(AppError::blocked("transcript event type 불일치"));
    }
    validate_id("project id", project_id)?;
    validate_id("session id", session_id)?;
    let record_id = required_detail(details, "record_id")?;
    validate_id("record id", record_id)?;
    let expected_pointer = format!(
        "state/transcripts/{}/{}/{}.json",
        project_id, session_id, record_id
    );
    if required_detail(details, "artifact_pointer")? != expected_pointer {
        return Err(AppError::blocked(format!(
            "transcript event artifact pointer 불일치\n- record id: {record_id}"
        )));
    }
    let path = validated_transcript_path(project_id, session_id, record_id, false)?;
    let record = load_record_path(&path)?;
    if record.record_id != record_id
        || record.project_id != project_id
        || record.session_id != session_id
        || record.workflow_id != required_detail(details, "workflow_id")?
        || record.kind != required_detail(details, "kind")?
        || record.content_hash != required_detail(details, "content_hash")?
        || record.artifact_hash != required_detail(details, "artifact_hash")?
    {
        return Err(AppError::blocked(format!(
            "transcript event binding 불일치\n- record id: {record_id}"
        )));
    }
    Ok(record)
}

impl TranscriptRecord {
    pub fn source_pointers_json(&self) -> String {
        render_source_pointers(&self.source_pointers)
    }

    fn artifact_payload(&self) -> String {
        format!(
            "{{\"schema_version\":{},\"record_id\":\"{}\",\"project_id\":\"{}\",\"session_id\":\"{}\",\"workflow_id\":\"{}\",\"kind\":\"{}\",\"causal_id\":\"{}\",\"content\":\"{}\",\"content_hash\":\"{}\",\"source_pointers\":{},\"recorded_at_ms\":{}}}",
            TRANSCRIPT_SCHEMA_VERSION,
            ledger::json_string(&self.record_id),
            ledger::json_string(&self.project_id),
            ledger::json_string(&self.session_id),
            ledger::json_string(&self.workflow_id),
            ledger::json_string(&self.kind),
            ledger::json_string(&self.causal_id),
            ledger::json_string(&self.content),
            self.content_hash,
            render_source_pointers(&self.source_pointers),
            self.recorded_at_ms
        )
    }

    fn to_json(&self) -> String {
        format!(
            "{{\n  \"schema_version\": {},\n  \"record_id\": \"{}\",\n  \"project_id\": \"{}\",\n  \"session_id\": \"{}\",\n  \"workflow_id\": \"{}\",\n  \"kind\": \"{}\",\n  \"causal_id\": \"{}\",\n  \"content\": \"{}\",\n  \"content_hash\": \"{}\",\n  \"source_pointers\": {},\n  \"recorded_at_ms\": {},\n  \"artifact_hash\": \"{}\"\n}}\n",
            TRANSCRIPT_SCHEMA_VERSION,
            ledger::json_string(&self.record_id),
            ledger::json_string(&self.project_id),
            ledger::json_string(&self.session_id),
            ledger::json_string(&self.workflow_id),
            ledger::json_string(&self.kind),
            ledger::json_string(&self.causal_id),
            ledger::json_string(&self.content),
            self.content_hash,
            render_source_pointers(&self.source_pointers),
            self.recorded_at_ms,
            self.artifact_hash
        )
    }
}

fn ensure_ledger_event(record: &TranscriptRecord) -> Result<(), AppError> {
    if ledger::event_detail_exists("transcript.recorded", "record_id", &record.record_id)? {
        return Ok(());
    }
    let identity = RuntimeIdentity {
        project_id: record.project_id.clone(),
        session_id: record.session_id.clone(),
        project_root: paths::project_root().display().to_string(),
    };
    let artifact_pointer = format!(
        "state/transcripts/{}/{}/{}.json",
        record.project_id, record.session_id, record.record_id
    );
    let event = ledger::new_event_for(
        &identity,
        "transcript.recorded",
        &format!("{} transcript record persisted", record.kind),
        &format!(
            "record_id={} workflow_id={} kind={} artifact_pointer={} artifact_hash={} content_hash={}",
            record.record_id,
            record.workflow_id,
            record.kind,
            artifact_pointer,
            record.artifact_hash,
            record.content_hash
        ),
    );
    ledger::append_event(&event)?;
    observability::project_event(&event)
}

fn load_record_path(path: &std::path::Path) -> Result<TranscriptRecord, AppError> {
    let body = fs::read_to_string(path).map_err(|err| {
        AppError::blocked(format!(
            "transcript artifact 읽기 실패\n- path: {}\n- error: {err}",
            path.display()
        ))
    })?;
    let object = crate::strict_json::parse_object(
        &body,
        &[
            "schema_version",
            "record_id",
            "project_id",
            "session_id",
            "workflow_id",
            "kind",
            "causal_id",
            "content",
            "content_hash",
            "source_pointers",
            "recorded_at_ms",
            "artifact_hash",
        ],
        "transcript artifact",
    )?;
    if crate::strict_json::number(&object, "schema_version", "transcript artifact")?
        != TRANSCRIPT_SCHEMA_VERSION
    {
        return Err(AppError::blocked("transcript schema version 불일치"));
    }
    let source_pointers = parse_source_pointers(object.get("source_pointers"))?;
    let mut record = TranscriptRecord {
        record_id: crate::strict_json::string(&object, "record_id", "transcript artifact")?,
        project_id: crate::strict_json::string(&object, "project_id", "transcript artifact")?,
        session_id: crate::strict_json::string(&object, "session_id", "transcript artifact")?,
        workflow_id: crate::strict_json::string(&object, "workflow_id", "transcript artifact")?,
        kind: crate::strict_json::string(&object, "kind", "transcript artifact")?,
        causal_id: crate::strict_json::string(&object, "causal_id", "transcript artifact")?,
        content: crate::strict_json::string(&object, "content", "transcript artifact")?,
        content_hash: crate::strict_json::string(&object, "content_hash", "transcript artifact")?,
        source_pointers,
        recorded_at_ms: crate::strict_json::number(
            &object,
            "recorded_at_ms",
            "transcript artifact",
        )? as u128,
        artifact_hash: crate::strict_json::string(&object, "artifact_hash", "transcript artifact")?,
    };
    validate_kind(&record.kind)?;
    validate_id("project id", &record.project_id)?;
    validate_id("record id", &record.record_id)?;
    validate_id("workflow id", &record.workflow_id)?;
    validate_id("session id", &record.session_id)?;
    validate_id("causal id", &record.causal_id)?;
    if record.content.trim().is_empty()
        || record.content.chars().count() > MAX_TRANSCRIPT_CONTENT_CHARS
    {
        return Err(AppError::blocked(format!(
            "transcript content boundary 불일치\n- record id: {}",
            record.record_id
        )));
    }
    for pointer in &record.source_pointers {
        validate_source_pointer(pointer)?;
    }
    if record.content_hash != state::sha256_text(&record.content) {
        return Err(AppError::blocked(format!(
            "transcript content hash 불일치\n- record id: {}",
            record.record_id
        )));
    }
    let expected_artifact_hash = state::sha256_text(&record.artifact_payload());
    if record.artifact_hash != expected_artifact_hash {
        return Err(AppError::blocked(format!(
            "transcript artifact hash 불일치\n- record id: {}",
            record.record_id
        )));
    }
    record.artifact_hash = expected_artifact_hash;
    Ok(record)
}

fn parse_source_pointers(
    value: Option<&crate::strict_json::Value>,
) -> Result<Vec<TranscriptSourcePointer>, AppError> {
    let Some(crate::strict_json::Value::Array(values)) = value else {
        return Err(AppError::blocked("transcript source_pointers type 불일치"));
    };
    let mut pointers = Vec::new();
    for value in values {
        let crate::strict_json::Value::Object(object) = value else {
            return Err(AppError::blocked("transcript source pointer type 불일치"));
        };
        if object
            .keys()
            .any(|key| !matches!(key.as_str(), "stable_ref" | "path" | "source_hash"))
        {
            return Err(AppError::blocked("transcript source pointer key 불일치"));
        }
        pointers.push(TranscriptSourcePointer {
            stable_ref: crate::strict_json::string(object, "stable_ref", "transcript pointer")?,
            path: crate::strict_json::string(object, "path", "transcript pointer")?,
            source_hash: crate::strict_json::string(object, "source_hash", "transcript pointer")?,
        });
    }
    Ok(pointers)
}

fn render_source_pointers(pointers: &[TranscriptSourcePointer]) -> String {
    let rows = pointers
        .iter()
        .map(|pointer| {
            format!(
                "{{\"stable_ref\":\"{}\",\"path\":\"{}\",\"source_hash\":\"{}\"}}",
                ledger::json_string(&pointer.stable_ref),
                ledger::json_string(&pointer.path),
                ledger::json_string(&pointer.source_hash)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("[{rows}]")
}

fn validate_expected_record(
    existing: &TranscriptRecord,
    workflow: &state::WorkflowRecord,
    kind: &str,
    causal_id: &str,
    content: &str,
    pointers: &[TranscriptSourcePointer],
) -> Result<(), AppError> {
    if existing.project_id != workflow.project_id
        || existing.session_id != workflow.session_id
        || existing.workflow_id != workflow.workflow_id
        || existing.kind != kind
        || existing.causal_id != causal_id
        || existing.content != content
        || existing.source_pointers != pointers
    {
        return Err(AppError::blocked(format!(
            "transcript deterministic record 충돌\n- record id: {}",
            existing.record_id
        )));
    }
    Ok(())
}

fn required_detail<'a>(details: &'a str, key: &str) -> Result<&'a str, AppError> {
    let prefix = format!("{key}=");
    details
        .split_whitespace()
        .find_map(|part| part.strip_prefix(&prefix))
        .ok_or_else(|| AppError::blocked(format!("transcript event field 누락: {key}")))
}

fn validate_kind(kind: &str) -> Result<(), AppError> {
    if matches!(kind, "user" | "model" | "tool" | "evidence") {
        Ok(())
    } else {
        Err(AppError::blocked(format!("transcript kind 불일치: {kind}")))
    }
}

fn validate_id(label: &str, value: &str) -> Result<(), AppError> {
    if value.is_empty()
        || value.len() > 160
        || matches!(value, "." | "..")
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(AppError::blocked(format!("transcript {label} 형식 불일치")));
    }
    Ok(())
}

fn validate_source_pointer(pointer: &TranscriptSourcePointer) -> Result<(), AppError> {
    if pointer.stable_ref.is_empty()
        || pointer.stable_ref.len() > 4_096
        || pointer.stable_ref.contains(['\r', '\n'])
        || pointer.path.is_empty()
        || pointer.path.len() > 4_096
        || pointer.path.contains(['\r', '\n'])
        || Path::new(&pointer.path).components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
        || pointer.source_hash.len() != 64
        || !pointer
            .source_hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(AppError::blocked(
            "transcript source pointer boundary 불일치",
        ));
    }
    Ok(())
}

fn validated_transcript_path(
    project_id: &str,
    session_id: &str,
    record_id: &str,
    create_parent: bool,
) -> Result<PathBuf, AppError> {
    validate_id("project id", project_id)?;
    validate_id("session id", session_id)?;
    validate_id("record id", record_id)?;

    let app_root = paths::app_data_root();
    ensure_directory_boundary(&app_root, create_parent, true)?;
    let app_root_canonical = fs::canonicalize(&app_root).map_err(|err| {
        AppError::blocked(format!(
            "app-data root 해석 실패: {} ({err})",
            app_root.display()
        ))
    })?;
    let state_root = paths::state_dir();
    ensure_directory_boundary(&state_root, create_parent, false)?;
    let root = paths::transcripts_dir();
    ensure_directory_boundary(&root, create_parent, false)?;
    let root_canonical = fs::canonicalize(&root).map_err(|err| {
        AppError::blocked(format!(
            "transcript root 해석 실패: {} ({err})",
            root.display()
        ))
    })?;
    if !root_canonical.starts_with(&app_root_canonical) {
        return Err(AppError::blocked("transcript root app-data 경계 이탈 차단"));
    }

    let mut parent = root.clone();
    for component in [project_id, session_id] {
        parent.push(component);
        if let Ok(metadata) = fs::symlink_metadata(&parent) {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(AppError::blocked(format!(
                    "transcript path boundary 불일치: {}",
                    parent.display()
                )));
            }
        } else if create_parent {
            fs::create_dir(&parent).map_err(|err| {
                AppError::runtime(format!(
                    "transcript directory 생성 실패: {} ({err})",
                    parent.display()
                ))
            })?;
        }
    }
    let parent_canonical = fs::canonicalize(&parent).map_err(|err| {
        AppError::blocked(format!(
            "transcript directory 해석 실패: {} ({err})",
            parent.display()
        ))
    })?;
    if !parent_canonical.starts_with(&root_canonical) {
        return Err(AppError::blocked("transcript path root 이탈 차단"));
    }

    let path = paths::transcript_file(project_id, session_id, record_id);
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(AppError::blocked(format!(
                "transcript artifact path boundary 불일치: {}",
                path.display()
            )));
        }
        let canonical = fs::canonicalize(&path)
            .map_err(|err| AppError::blocked(format!("transcript artifact 해석 실패: {err}")))?;
        if !canonical.starts_with(&root_canonical) {
            return Err(AppError::blocked("transcript artifact root 이탈 차단"));
        }
    }
    Ok(path)
}

fn ensure_directory_boundary(
    path: &Path,
    create: bool,
    create_ancestors: bool,
) -> Result<(), AppError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(AppError::blocked(format!(
                "transcript directory boundary 불일치: {}",
                path.display()
            )));
        }
        Ok(_) => return Ok(()),
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
            return Err(AppError::blocked(format!(
                "transcript directory 검사 실패: {} ({err})",
                path.display()
            )));
        }
        Err(_) if !create => {
            return Err(AppError::blocked(format!(
                "transcript directory 누락: {}",
                path.display()
            )));
        }
        Err(_) => {}
    }

    let result = if create_ancestors {
        fs::create_dir_all(path)
    } else {
        fs::create_dir(path)
    };
    result.map_err(|err| {
        AppError::runtime(format!(
            "transcript directory 생성 실패: {} ({err})",
            path.display()
        ))
    })
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcript_record_is_idempotent_and_sqlite_rebuilds_from_canonical_artifacts() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-transcript-rebuild-test-{}",
            std::process::id()
        ));
        let project = root.join("project");
        let data = root.join("data");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(project.join("src")).unwrap();
        fs::write(project.join("src/lib.rs"), "pub const VALUE: i32 = 1;\n").unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
        std::env::set_var("RPOTATO_DATA_HOME", &data);

        state::initialize().unwrap();
        let workflow = state::create_workflow("값을 확인해줘").unwrap();
        let first =
            record_workflow_turn(&workflow, "user", "request", "값을 확인해줘", &[]).unwrap();
        let repeated =
            record_workflow_turn(&workflow, "user", "request", "값을 확인해줘", &[]).unwrap();
        let second =
            record_workflow_turn(&workflow, "tool", "context", "context prepared", &[]).unwrap();
        assert_eq!(first, repeated);
        assert_eq!(records_for_session(&workflow.session_id).unwrap().len(), 2);
        assert_eq!(
            crate::observability::status().unwrap().transcript_records,
            2
        );
        assert_projection_order(&[&first.record_id, &second.record_id]);

        let mut escaped = workflow.clone();
        escaped.project_id = "../escape".to_string();
        assert_eq!(
            record_workflow_turn(&escaped, "user", "request", "차단", &[])
                .unwrap_err()
                .code,
            3
        );
        let bad_pointer = SourcePointer {
            path: "src/lib.rs".to_string(),
            stable_ref: "src/lib.rs:1".to_string(),
            chars: 0,
            fingerprint: "not-a-sha256".to_string(),
            snippet: String::new(),
        };
        assert_eq!(
            record_workflow_turn(&workflow, "tool", "bad-pointer", "차단", &[bad_pointer])
                .unwrap_err()
                .code,
            3
        );
        let traversal_pointer = SourcePointer {
            path: "../secret".to_string(),
            stable_ref: "../secret:1".to_string(),
            chars: 0,
            fingerprint: "a".repeat(64),
            snippet: String::new(),
        };
        assert_eq!(
            record_workflow_turn(
                &workflow,
                "tool",
                "traversal-pointer",
                "차단",
                &[traversal_pointer]
            )
            .unwrap_err()
            .code,
            3
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;

            let outside = root.join("outside");
            fs::create_dir_all(&outside).unwrap();
            symlink(&outside, paths::transcripts_dir().join("symlink-project")).unwrap();
            assert_eq!(
                validated_transcript_path("symlink-project", "session-safe", "record-safe", true)
                    .unwrap_err()
                    .code,
                3
            );
        }

        let db = paths::observability_db_file();
        let _ = fs::remove_file(&db);
        let _ = fs::remove_file(db.with_extension("sqlite-wal"));
        let _ = fs::remove_file(db.with_extension("sqlite-shm"));
        assert_eq!(
            crate::observability::status().unwrap().transcript_records,
            2
        );
        assert_projection_order(&[&first.record_id, &second.record_id]);

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;

            let outside_root = root.join("outside-root");
            fs::create_dir_all(&outside_root).unwrap();
            fs::remove_dir_all(paths::transcripts_dir()).unwrap();
            symlink(&outside_root, paths::transcripts_dir()).unwrap();
            assert_eq!(
                validated_transcript_path("project-safe", "session-safe", "record-safe", true)
                    .unwrap_err()
                    .code,
                3
            );
        }

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);
    }

    fn assert_projection_order(expected: &[&str]) {
        let connection = rusqlite::Connection::open(paths::observability_db_file()).unwrap();
        let mut statement = connection
            .prepare(
                "SELECT record_id, ledger_event_id, event_ordinal
                   FROM transcript_records
               ORDER BY event_ordinal",
            )
            .unwrap();
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            rows.iter().map(|row| row.0.as_str()).collect::<Vec<_>>(),
            expected
        );
        assert!(rows.iter().all(|row| !row.1.is_empty() && row.2 > 0));
        assert!(rows.windows(2).all(|pair| pair[0].2 < pair[1].2));
    }
}
