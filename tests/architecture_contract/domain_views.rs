use super::*;

#[test]
fn v0375_domain_views_replace_legacy_definitions() {
    let state_adapter = "src/app/workflow_adapter/state.rs";
    let transcript_adapter = "src/app/workflow_adapter/transcript.rs";
    let transcript_storage = "src/app/workflow_adapter/transcript/storage.rs";
    let transcript_storage_contract = "src/app/workflow_adapter/transcript/storage/contract.rs";
    let transcript_storage_paths = "src/app/workflow_adapter/transcript/storage/path_resolution.rs";
    let transcript_storage_records =
        "src/app/workflow_adapter/transcript/storage/record_repository.rs";
    let transcript_storage_tools = "src/app/workflow_adapter/transcript/storage/tool_artifact.rs";
    let transcript_tool_turn = "src/app/workflow_adapter/transcript/tool_turn.rs";
    let transcript_streams = "src/app/workflow_adapter/transcript/tool_turn/streams.rs";
    let transcript_tests = "src/app/workflow_adapter/transcript/tests.rs";
    for target in [
        "src/runtime_core/workflow/domain/mod.rs",
        "src/runtime_core/workflow/domain/snapshot.rs",
        "src/runtime_core/workflow/domain/transcript.rs",
    ] {
        assert!(
            Path::new(target).is_file(),
            "missing domain owner: {target}"
        );
    }

    let domain = fs::read_to_string("src/runtime_core/workflow/domain/mod.rs").unwrap();
    for owner in ["snapshot", "transcript"] {
        let expected = format!("pub(crate) mod {owner};");
        assert!(
            domain.lines().any(|line| line == expected),
            "workflow domain owner is not crate-private: {owner}"
        );
    }

    for (facade, moved_definition) in [
        (state_adapter, "struct CurrentStateSnapshot"),
        (state_adapter, "struct CurrentStateLeaseView"),
        (transcript_adapter, "struct ToolOutputView"),
    ] {
        let source = fs::read_to_string(facade).unwrap();
        assert!(
            !source.contains(moved_definition),
            "legacy facade still owns moved definition: {facade} -> {moved_definition}"
        );
    }

    let snapshot = fs::read_to_string("src/runtime_core/workflow/domain/snapshot.rs").unwrap();
    for rule in [
        "fn validate_session_resume_target",
        "fn validate_current_lease",
        "fn validate_read_only_workflow",
    ] {
        assert!(
            snapshot.contains(rule),
            "snapshot owner is missing domain rule: {rule}"
        );
    }

    assert!(
        !Path::new("src/state.rs").exists(),
        "legacy workflow root was restored: src/state.rs"
    );
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(
        !main.lines().any(|line| line == "mod state;"),
        "legacy workflow root remains registered: mod state;"
    );
    let adapter_mod = fs::read_to_string("src/app/workflow_adapter.rs").unwrap();
    assert!(
        adapter_mod
            .lines()
            .any(|line| line == "pub(crate) mod state;"),
        "state adapter is not registered under workflow_adapter"
    );

    let transcript = fs::read_to_string("src/runtime_core/workflow/domain/transcript.rs").unwrap();
    for rule in [
        "fn collect_session_records",
        "fn parse_event_binding",
        "fn validate_event_identity",
    ] {
        assert!(
            transcript.contains(rule),
            "transcript owner is missing domain rule: {rule}"
        );
    }

    assert!(
        !Path::new("src/transcript.rs").exists(),
        "legacy workflow root was restored: src/transcript.rs"
    );
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(
        !main.lines().any(|line| line == "mod transcript;"),
        "legacy workflow root remains registered: mod transcript;"
    );
    let adapter_mod = fs::read_to_string("src/app/workflow_adapter.rs").unwrap();
    assert!(
        adapter_mod
            .lines()
            .any(|line| line == "pub(crate) mod transcript;"),
        "transcript adapter is not registered under workflow_adapter"
    );
    assert!(Path::new(transcript_storage).is_file());
    assert!(Path::new(transcript_storage_contract).is_file());
    assert!(Path::new(transcript_storage_paths).is_file());
    assert!(Path::new(transcript_storage_records).is_file());
    assert!(Path::new(transcript_storage_tools).is_file());
    assert!(Path::new(transcript_tool_turn).is_file());
    assert!(Path::new(transcript_streams).is_file());
    assert!(Path::new(transcript_tests).is_file());
    let transcript_adapter_source = fs::read_to_string(transcript_adapter).unwrap();
    let transcript_storage_source = fs::read_to_string(transcript_storage).unwrap();
    let transcript_storage_contract_source =
        fs::read_to_string(transcript_storage_contract).unwrap();
    let transcript_storage_path_source = fs::read_to_string(transcript_storage_paths).unwrap();
    let transcript_storage_record_source = fs::read_to_string(transcript_storage_records).unwrap();
    let transcript_storage_tool_source = fs::read_to_string(transcript_storage_tools).unwrap();
    let transcript_tool_turn_source = fs::read_to_string(transcript_tool_turn).unwrap();
    let transcript_stream_source = fs::read_to_string(transcript_streams).unwrap();
    let transcript_test_source = fs::read_to_string(transcript_tests).unwrap();
    assert!(
        transcript_adapter_source
            .lines()
            .any(|line| line == "mod storage;"),
        "transcript adapter does not register its storage owner"
    );
    assert!(
        transcript_adapter_source
            .lines()
            .any(|line| line == "mod tool_turn;"),
        "transcript adapter does not register its tool-turn owner"
    );
    assert!(
        transcript_adapter_source.contains("#[path = \"transcript/tests.rs\"]"),
        "transcript adapter does not register its regression-test owner"
    );
    for regression in [
        "fn sanitized_stream_limits_use_utf8_bytes_at_each_boundary(",
        "fn prepared_no_stream_turn_installs_exact_artifacts_without_ledger_side_effect(",
        "fn transcript_v2_tool_binding_strict_round_trip(",
        "fn transcript_record_is_idempotent_and_sqlite_rebuilds_from_canonical_artifacts(",
    ] {
        assert!(
            transcript_test_source.contains(regression),
            "transcript regression owner is missing: {regression}"
        );
    }
    for owner in [
        "contract",
        "path_resolution",
        "record_repository",
        "tool_artifact",
    ] {
        assert!(
            transcript_storage_source
                .lines()
                .any(|line| line == format!("mod {owner};")),
            "transcript storage facade does not register {owner}"
        );
    }
    for (owner, responsibility) in [
        (&transcript_storage_record_source, "fn load_record_path("),
        (&transcript_storage_record_source, "fn install_record("),
        (
            &transcript_storage_record_source,
            "fn validate_expected_record(",
        ),
        (
            &transcript_storage_tool_source,
            "fn load_tool_output_artifact(",
        ),
        (
            &transcript_storage_tool_source,
            "fn parse_tool_output_artifact_body(",
        ),
        (
            &transcript_storage_tool_source,
            "fn validate_tool_binding_for_record(",
        ),
        (
            &transcript_storage_path_source,
            "fn validated_tool_output_path(",
        ),
        (
            &transcript_storage_path_source,
            "fn validated_transcript_path(",
        ),
        (
            &transcript_storage_path_source,
            "fn ensure_directory_boundary(",
        ),
        (
            &transcript_storage_contract_source,
            "fn validate_event_details_for_schema(",
        ),
    ] {
        assert!(
            owner.contains(responsibility),
            "transcript storage responsibility owner is missing: {responsibility}"
        );
        assert!(
            !transcript_storage_source.contains(responsibility),
            "transcript storage facade still owns: {responsibility}"
        );
        assert!(
            !transcript_adapter_source.contains(responsibility),
            "transcript adapter still owns storage validation: {responsibility}"
        );
    }
    for responsibility in [
        "pub(crate) struct PreparedTranscriptTurn",
        "pub(crate) fn prepare_no_stream_tool_turn(",
        "pub(crate) fn install_prepared_no_stream_tool_turn(",
        "pub(crate) fn decode_prepared_no_stream_tool_turn(",
        "pub(crate) fn tool_output_view_from_canonical_record(",
    ] {
        assert!(
            transcript_tool_turn_source.contains(responsibility),
            "transcript tool-turn owner is missing: {responsibility}"
        );
        assert!(
            !transcript_adapter_source.contains(responsibility),
            "transcript adapter still owns tool-turn behavior: {responsibility}"
        );
    }
    assert!(
        transcript_tool_turn_source
            .lines()
            .any(|line| line == "mod streams;"),
        "transcript tool-turn owner does not register its stream policy owner"
    );
    for responsibility in [
        "pub(in super::super) fn record_tool_output_artifact(",
        "pub(in super::super) fn sanitize_tool_stream(",
        "pub(in super::super) fn validate_requested_tool_streams(",
        "pub(in super::super) struct SanitizedStream",
    ] {
        assert!(
            transcript_stream_source.contains(responsibility),
            "transcript stream owner is missing: {responsibility}"
        );
        assert!(
            !transcript_tool_turn_source.contains(responsibility),
            "transcript tool-turn owner still owns stream policy: {responsibility}"
        );
    }
    assert!(
        transcript_adapter_source.lines().count() < 450,
        "transcript adapter regrew beyond its orchestration boundary"
    );
    for (owner, source, line_budget) in [
        ("facade", &transcript_storage_source, 30),
        ("contract", &transcript_storage_contract_source, 90),
        ("path resolution", &transcript_storage_path_source, 240),
        ("record repository", &transcript_storage_record_source, 100),
        ("tool artifact", &transcript_storage_tool_source, 250),
    ] {
        assert!(
            source.lines().count() < line_budget,
            "transcript storage {owner} exceeded its {line_budget}-line budget"
        );
    }
    assert!(
        transcript_tool_turn_source.lines().count() < 450,
        "transcript tool-turn module regrew beyond its ownership boundary"
    );
    assert!(
        transcript_stream_source.lines().count() < 275,
        "transcript stream module regrew beyond its ownership boundary"
    );
    assert!(
        transcript_test_source.lines().count() < 425,
        "transcript regression module regrew beyond its ownership boundary"
    );
}
