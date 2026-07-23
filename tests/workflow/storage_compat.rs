use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::foundation::error::AppError;
use crate::{ledger, record, transcript};

fn fixture_path(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must follow the Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("rpotato-storage-compat-{name}-{nonce}"))
}

fn corrupt(path: &Path) -> AppError {
    AppError::blocked(format!("corrupt storage fixture: {}", path.display()))
}

#[test]
fn workflow_snapshot_and_pointer_bytes_round_trip_without_schema_drift() {
    let identity = ledger::RuntimeIdentity {
        project_id: "project-storage".to_string(),
        session_id: "session-storage".to_string(),
        project_root: "/tmp/project-storage".to_string(),
    };
    let mut workflow = record::WorkflowRecord::new(&identity, "storage contract");
    workflow.workflow_id = "workflow-storage".to_string();
    workflow.action_id = "action-storage".to_string();
    workflow.revision = 7;
    workflow.previous_hash = "previous-storage-hash".to_string();
    workflow.phase = "complete".to_string();
    workflow.action_kind = "read-only".to_string();
    workflow.action_status = "complete".to_string();
    workflow.result_summary = "완료".to_string();
    workflow.artifact_hash = ledger::sha256_bytes(record::payload(&workflow).as_bytes());

    let snapshot = record::render(&workflow);
    let parsed = record::parse_snapshot(Path::new("workflow.json"), &snapshot, corrupt).unwrap();
    assert_eq!(parsed, workflow);
    assert_eq!(record::render(&parsed), snapshot);
    let lines = snapshot.lines().map(str::trim).collect::<Vec<_>>();
    assert_eq!(lines.len(), 39);
    assert_eq!(
        &lines[..6],
        [
            "{",
            "\"schema_version\": 4,",
            "\"artifact_version\": \"workflow-v4\",",
            "\"workflow_id\": \"workflow-storage\",",
            "\"revision\": 7,",
            "\"previous_hash\": \"previous-storage-hash\",",
        ]
    );
    assert_eq!(
        lines[6],
        format!("\"artifact_hash\": \"{}\",", workflow.artifact_hash)
    );
    assert_eq!(lines[7], "\"project_id\": \"project-storage\",");
    assert_eq!(lines[8], "\"session_id\": \"session-storage\",");
    assert_eq!(lines[9], "\"phase\": \"complete\",");
    assert_eq!(lines[18], "\"action_id\": \"action-storage\",");
    assert_eq!(lines[19], "\"action_kind\": \"read-only\",");
    assert_eq!(lines[20], "\"action_status\": \"complete\",");
    assert_eq!(lines[21], "\"result_summary\": \"완료\",");
    assert_eq!(lines[32], "\"approval_state\": \"not-requested\",");
    assert_eq!(lines[37], "\"failure_reason\": \"\"");
    assert_eq!(lines[38], "}");

    let pointer = record::render_pointer(&workflow, 4).unwrap();
    assert_eq!(
        pointer,
        format!(
            "{{\n  \"schema_version\": 4,\n  \"artifact_version\": \"workflow-commit-v4\",\n  \"workflow_id\": \"workflow-storage\",\n  \"committed_revision\": 7,\n  \"artifact_hash\": \"{}\"\n}}\n",
            workflow.artifact_hash
        )
    );
    let parsed_pointer =
        record::parse_pointer(Path::new("workflow.pointer"), &pointer, corrupt).unwrap();
    assert_eq!(parsed_pointer.schema_version, 4);
    assert_eq!(parsed_pointer.workflow_id, workflow.workflow_id);
    assert_eq!(parsed_pointer.committed_revision, workflow.revision);
    assert_eq!(parsed_pointer.artifact_hash, workflow.artifact_hash);
}

#[test]
fn ledger_codec_preserves_order_and_hash_chain() {
    let first = ledger::LedgerEvent {
        event_id: "event-1".to_string(),
        ts_ms: 11,
        event_type: "workflow.created".to_string(),
        project_id: "project-storage".to_string(),
        session_id: "session-storage".to_string(),
        summary: "first".to_string(),
        details: "revision=1".to_string(),
    };
    let second = ledger::LedgerEvent {
        event_id: "event-2".to_string(),
        ts_ms: 12,
        event_type: "transcript.recorded".to_string(),
        project_id: "project-storage".to_string(),
        session_id: "session-storage".to_string(),
        summary: "second".to_string(),
        details: "record_id=transcript-1".to_string(),
    };

    let (first_line, first_hash) = ledger::canonical_event_line(&first, "none");
    assert_eq!(first_hash, ledger::planned_event_hash(&first, "none"));
    let (second_line, second_hash) = ledger::canonical_event_line(&second, &first_hash);
    assert_eq!(
        second_hash,
        ledger::planned_event_hash(&second, &first_hash)
    );

    let parsed = [first_line, second_line]
        .iter()
        .map(|line| ledger::parse_event_line_strict(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        parsed
            .iter()
            .map(|event| event.event_id.as_str())
            .collect::<Vec<_>>(),
        ["event-1", "event-2"]
    );
    assert_eq!(parsed[0].previous_event_hash.as_deref(), Some("none"));
    assert_eq!(parsed[0].event_hash.as_deref(), Some(first_hash.as_str()));
    assert_eq!(
        parsed[1].previous_event_hash.as_deref(),
        Some(first_hash.as_str())
    );
    assert_eq!(parsed[1].event_hash.as_deref(), Some(second_hash.as_str()));
    assert_eq!(ledger::event_physical_hash(&parsed[0], "none"), first_hash);
    assert_eq!(
        ledger::event_physical_hash(&parsed[1], &first_hash),
        second_hash
    );
}

fn transcript_record(content: &str) -> transcript::TranscriptRecord {
    let mut record = transcript::TranscriptRecord {
        schema_version: transcript::TRANSCRIPT_SCHEMA_V2,
        record_id: "transcript-storage".to_string(),
        project_id: "project-storage".to_string(),
        session_id: "session-storage".to_string(),
        workflow_id: "workflow-storage".to_string(),
        kind: "user".to_string(),
        causal_id: "request-storage".to_string(),
        content: content.to_string(),
        content_hash: ledger::sha256_bytes(content.as_bytes()),
        source_pointers: vec![transcript::TranscriptSourcePointer {
            stable_ref: "src/lib.rs:1".to_string(),
            path: "src/lib.rs".to_string(),
            source_hash: "a".repeat(64),
        }],
        recorded_at_ms: 21,
        tool_output_artifact: None,
        artifact_hash: String::new(),
    };
    record.artifact_hash = ledger::sha256_bytes(record.artifact_payload().as_bytes());
    record
}

#[test]
fn transcript_install_is_byte_exact_idempotent_and_immutable() {
    let record = transcript_record("canonical content");
    let expected = format!(
        "{{\"schema_version\":2,\"record_id\":\"transcript-storage\",\"project_id\":\"project-storage\",\"session_id\":\"session-storage\",\"workflow_id\":\"workflow-storage\",\"kind\":\"user\",\"causal_id\":\"request-storage\",\"content\":\"canonical content\",\"content_hash\":\"{}\",\"source_pointers\":[{{\"stable_ref\":\"src/lib.rs:1\",\"path\":\"src/lib.rs\",\"source_hash\":\"{}\"}}],\"recorded_at_ms\":21,\"tool_output_artifact\":null,\"artifact_hash\":\"{}\"}}",
        record.content_hash,
        "a".repeat(64),
        record.artifact_hash
    );
    assert_eq!(record.to_json(), expected);
    assert_eq!(transcript::parse_record(&expected).unwrap(), record);

    assert_eq!(
        transcript::canonical_install_bytes(&record, None).unwrap(),
        Some(expected.clone())
    );
    assert_eq!(
        transcript::canonical_install_bytes(&record, Some(&expected)).unwrap(),
        None,
        "idempotent install must not rewrite"
    );

    let conflict = transcript_record("different canonical content");
    let error = transcript::canonical_install_bytes(&conflict, Some(&expected)).unwrap_err();
    assert_eq!(error.code, 3);
    assert!(error.message.contains("immutable conflict"));
}
