use super::*;
use crate::adapters::sqlite::observability_projection::schema::migrate;
use rusqlite::{params, Connection};

#[test]
fn latest_model_run_is_scoped_to_the_requested_session() {
    let connection = Connection::open_in_memory().unwrap();
    migrate(&connection).unwrap();
    for (run_id, session_id, model_id, started_at, context_used) in [
        ("run-a-old", "session-a", "model-a-old", 10_i64, 100_i64),
        ("run-a-new", "session-a", "model-a-new", 20_i64, 200_i64),
        ("run-b-newest", "session-b", "model-b", 30_i64, 3_000_i64),
    ] {
        connection
            .execute(
                "INSERT INTO model_runs (model_run_id, session_id, model_id, context_limit_tokens, started_at_ms) VALUES (?1, ?2, ?3, 4096, ?4)",
                params![run_id, session_id, model_id, started_at],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO token_usage (token_usage_id, model_run_id, model_id, total_tokens, context_tokens_used) VALUES (?1, ?2, ?3, ?4, ?4)",
                params![format!("token-{run_id}"), run_id, model_id, context_used],
            )
            .unwrap();
    }

    let session_a = latest_model_run_for_session_from_connection(&connection, "session-a")
        .unwrap()
        .unwrap();
    let session_b = latest_model_run_for_session_from_connection(&connection, "session-b")
        .unwrap()
        .unwrap();

    assert_eq!(session_a.model_id, "model-a-new");
    assert_eq!(session_a.context_tokens_used, Some(200));
    assert_eq!(session_b.model_id, "model-b");
    assert_eq!(session_b.context_tokens_used, Some(3_000));
    assert!(
        latest_model_run_for_session_from_connection(&connection, "missing")
            .unwrap()
            .is_none()
    );
}
