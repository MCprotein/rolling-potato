use super::*;

#[test]
fn ontology_record_bytes_are_stable() {
    let record = OntologyRecord {
        id: "a:file:fixture".to_string(),
        layer: "A".to_string(),
        kind: "file".to_string(),
        label: "fixture".to_string(),
        status: "confirmed".to_string(),
        claim_state: "confirmed".to_string(),
        confidence: "1.00".to_string(),
        source_pointer: "src/main.rs:1".to_string(),
        source_hash: "source-hash".to_string(),
        evidence: "indexed-file".to_string(),
        supersedes: String::new(),
        current: true,
        event_id: "event-fixture".to_string(),
        created_at_ms: 42,
    };

    assert_eq!(
        record.to_json_line(),
        "{\"schemaVersion\":1,\"id\":\"a:file:fixture\",\"layer\":\"A\",\"kind\":\"file\",\"label\":\"fixture\",\"status\":\"confirmed\",\"claimState\":\"confirmed\",\"confidence\":\"1.00\",\"sourcePointer\":\"src/main.rs:1\",\"sourceHash\":\"source-hash\",\"evidence\":\"indexed-file\",\"supersedes\":\"\",\"current\":true,\"eventId\":\"event-fixture\",\"createdAtMs\":42}"
    );
}

#[test]
fn projection_keeps_latest_current_record_and_context_binding() {
    let mut first = layer_a_record("file", "first", "src/main.rs", "old", "main");
    first.created_at_ms = 1;
    let mut latest = layer_a_record("file", "latest", "src/main.rs", "new", "main");
    latest.created_at_ms = 2;
    let contents = format!("{}\n{}\n", first.to_json_line(), latest.to_json_line());

    let projection = parse_projection(&contents);
    let selected = runtime_context_selection(&projection, "main", 4, |_| false);

    assert_eq!(projection.total_records, 2);
    assert_eq!(projection.current_records.len(), 1);
    assert_eq!(projection.current_records[0].label, "latest");
    assert_eq!(selected.selected[0].source_hash, "new");
}
