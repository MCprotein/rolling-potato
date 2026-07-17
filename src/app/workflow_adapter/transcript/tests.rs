use std::fs;

use super::storage::load_tool_output_artifact;
use super::tool_turn::UNAVAILABLE_STREAM;
use super::*;
use crate::foundation::serialization as strict_json;

#[test]
fn sanitized_stream_limits_use_utf8_bytes_at_each_boundary() {
    for length in [MAX_SANITIZED_STREAM_BYTES - 1, MAX_SANITIZED_STREAM_BYTES] {
        let value = "x".repeat(length);
        let sanitized = sanitize_tool_stream(Some(&value)).unwrap();
        assert_eq!(sanitized.text.len(), length);
        assert!(!sanitized.truncated);
    }
    let over = "x".repeat(MAX_SANITIZED_STREAM_BYTES + 1);
    let sanitized = sanitize_tool_stream(Some(&over)).unwrap();
    assert_eq!(sanitized.text.len(), MAX_SANITIZED_STREAM_BYTES);
    assert!(sanitized.truncated);

    let multibyte = "가".repeat((MAX_SANITIZED_STREAM_BYTES / 3) + 1);
    let sanitized = sanitize_tool_stream(Some(&multibyte)).unwrap();
    assert!(sanitized.text.len() <= MAX_SANITIZED_STREAM_BYTES);
    assert!(sanitized.truncated);
    assert!(sanitized.text.is_char_boundary(sanitized.text.len()));
}

#[test]
fn transcript_content_limit_uses_utf8_bytes_at_each_boundary() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-transcript-byte-limit-{}",
        std::process::id()
    ));
    let project = root.join("project");
    let data = root.join("data");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&project).unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
    std::env::set_var("RPOTATO_DATA_HOME", &data);
    state::initialize().unwrap();
    let workflow = state::create_workflow("transcript byte boundary").unwrap();

    for length in [
        MAX_TRANSCRIPT_CONTENT_BYTES - 1,
        MAX_TRANSCRIPT_CONTENT_BYTES,
    ] {
        let content = "x".repeat(length);
        assert_eq!(content.len(), length);
        prepare_no_stream_tool_turn(&workflow, &format!("event-limit-{length}"), &content, &[])
            .unwrap();
    }
    let too_large = "x".repeat(MAX_TRANSCRIPT_CONTENT_BYTES + 1);
    assert!(
        prepare_no_stream_tool_turn(&workflow, "event-limit-over", &too_large, &[])
            .unwrap_err()
            .message
            .contains("content boundary")
    );
    let multibyte = "가".repeat((MAX_TRANSCRIPT_CONTENT_BYTES / 3) + 1);
    assert!(multibyte.chars().count() < MAX_TRANSCRIPT_CONTENT_BYTES);
    assert!(multibyte.len() > MAX_TRANSCRIPT_CONTENT_BYTES);
    assert!(
        prepare_no_stream_tool_turn(&workflow, "event-limit-utf8", &multibyte, &[])
            .unwrap_err()
            .message
            .contains("content boundary")
    );

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn prepared_no_stream_turn_installs_exact_artifacts_without_ledger_side_effect() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-prepared-transcript-test-{}",
        std::process::id()
    ));
    let project = root.join("project");
    let data = root.join("data");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&project).unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
    std::env::set_var("RPOTATO_DATA_HOME", &data);
    state::initialize().unwrap();
    let workflow = state::create_workflow("prepared transcript test").unwrap();
    let before_count = ledger::read_runtime_events().unwrap().len();

    let prepared = prepare_no_stream_tool_turn(
        &workflow,
        "event-patch-applied",
        "patch applied: proposal_id=proposal-a path=src/lib.rs",
        &[],
    )
    .unwrap();
    assert!(!prepared.tool_path.exists());
    assert!(!prepared.transcript_path.exists());
    assert_eq!(prepared.event.event_type, "transcript.recorded");
    assert_eq!(ledger::read_runtime_events().unwrap().len(), before_count);

    install_prepared_no_stream_tool_turn(&prepared).unwrap();
    install_prepared_no_stream_tool_turn(&prepared).unwrap();
    let artifact = load_tool_output_artifact(&prepared.tool_path).unwrap();
    assert_eq!(artifact.stdout, UNAVAILABLE_STREAM);
    assert_eq!(artifact.stderr, UNAVAILABLE_STREAM);
    assert_eq!(artifact.stdout_original_bytes, 0);
    assert_eq!(artifact.stderr_original_bytes, 0);
    assert_eq!(
        fs::read_to_string(&prepared.transcript_path).unwrap(),
        prepared.transcript_bytes
    );
    assert_eq!(
        load_record_path(&prepared.transcript_path).unwrap(),
        prepared.record
    );
    assert_eq!(ledger::read_runtime_events().unwrap().len(), before_count);

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn transcript_v2_tool_binding_strict_round_trip() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root =
        std::env::temp_dir().join(format!("rpotato-transcript-v2-test-{}", std::process::id()));
    let project = root.join("project");
    let data = root.join("data");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&project).unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
    std::env::set_var("RPOTATO_DATA_HOME", &data);

    state::initialize().unwrap();
    let workflow = state::create_workflow("transcript v2 strict test").unwrap();
    let record = record_workflow_turn_with_streams(
        &workflow,
        "tool",
        "tool-call-v2",
        "bounded tool result",
        &[],
        Some("ok api_key=SUPER_SECRET_SENTINEL\u{001b}[31m"),
        None,
    )
    .unwrap();
    assert_eq!(record.schema_version, TRANSCRIPT_SCHEMA_V2);
    let binding = record.tool_output_artifact.as_ref().unwrap();
    let transcript_path =
        paths::transcript_file(&record.project_id, &record.session_id, &record.record_id);
    let transcript_body = fs::read_to_string(&transcript_path).unwrap();
    assert!(!transcript_body.ends_with('\n'));
    strict_json::parse_canonical_object(
        &transcript_body,
        TRANSCRIPT_V2_KEYS,
        "test TranscriptRecord v2",
    )
    .unwrap();
    assert_eq!(load_record_path(&transcript_path).unwrap(), record);

    let tool_path = paths::tool_output_file(
        &record.project_id,
        &record.session_id,
        &record.workflow_id,
        &binding.id,
    );
    let tool_body = fs::read_to_string(&tool_path).unwrap();
    assert!(!tool_body.contains("SUPER_SECRET_SENTINEL"));
    assert!(!tool_body.contains('\u{001b}'));
    let artifact = load_tool_output_artifact(&tool_path).unwrap();
    assert_eq!(artifact.stderr, UNAVAILABLE_STREAM);
    assert_eq!(artifact.stderr_original_bytes, 0);
    assert!(!artifact.stderr_truncated);
    assert!(!artifact.stderr_redacted);
    assert_eq!(artifact.content_hash, binding.hash);

    let event = ledger::read_runtime_events()
        .unwrap()
        .into_iter()
        .find(|event| {
            event.event_type == "transcript.recorded" && event.details.contains(&record.record_id)
        })
        .unwrap();
    assert_eq!(record_from_event(&event).unwrap(), record);
    validate_event_details_for_schema(&event.details, TRANSCRIPT_SCHEMA_V2).unwrap();

    let reordered =
        transcript_body.replacen("{\"schema_version\":2,\"record_id\"", "{\"record_id\"", 1);
    fs::write(&transcript_path, reordered).unwrap();
    assert_eq!(load_record_path(&transcript_path).unwrap_err().code, 3);
    fs::write(&transcript_path, &transcript_body).unwrap();

    let non_tool =
        record_workflow_turn(&workflow, "user", "user-v2", "non-tool v2 record", &[]).unwrap();
    assert_eq!(non_tool.schema_version, TRANSCRIPT_SCHEMA_V2);
    assert!(non_tool.tool_output_artifact.is_none());

    let legacy_causal = "legacy-tool";
    let legacy_record_id = format!(
        "transcript-{}",
        &state::sha256_text(&format!(
            "{}\n{}\n{}\n{}\n{}",
            workflow.project_id, workflow.session_id, workflow.workflow_id, "tool", legacy_causal
        ))[..24]
    );
    let mut legacy = TranscriptRecord {
        schema_version: TRANSCRIPT_SCHEMA_V1,
        record_id: legacy_record_id.clone(),
        project_id: workflow.project_id.clone(),
        session_id: workflow.session_id.clone(),
        workflow_id: workflow.workflow_id.clone(),
        kind: "tool".to_string(),
        causal_id: legacy_causal.to_string(),
        content: "legacy result".to_string(),
        content_hash: state::sha256_text("legacy result"),
        source_pointers: Vec::new(),
        recorded_at_ms: 1,
        tool_output_artifact: None,
        artifact_hash: String::new(),
    };
    legacy.artifact_hash = state::sha256_text(&legacy.artifact_payload());
    let legacy_path = validated_transcript_path(
        &legacy.project_id,
        &legacy.session_id,
        &legacy.record_id,
        true,
    )
    .unwrap();
    let legacy_bytes = legacy.to_json();
    state::atomic_replace_bytes(&legacy_path, legacy_bytes.as_bytes()).unwrap();
    let retried =
        record_workflow_turn(&workflow, "tool", legacy_causal, "legacy result", &[]).unwrap();
    assert_eq!(retried.schema_version, TRANSCRIPT_SCHEMA_V1);
    assert!(retried.tool_output_artifact.is_none());
    assert_eq!(fs::read_to_string(&legacy_path).unwrap(), legacy_bytes);

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(root);
}

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
    let first = record_workflow_turn(&workflow, "user", "request", "값을 확인해줘", &[]).unwrap();
    let repeated =
        record_workflow_turn(&workflow, "user", "request", "값을 확인해줘", &[]).unwrap();
    let second =
        record_workflow_turn(&workflow, "tool", "context", "context prepared", &[]).unwrap();
    assert_eq!(first, repeated);
    assert_eq!(records_for_session(&workflow.session_id).unwrap().len(), 2);
    assert_eq!(
        crate::app::observability_adapter::status()
            .unwrap()
            .transcript_records,
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
        crate::app::observability_adapter::status()
            .unwrap()
            .transcript_records,
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
