use super::*;

#[test]
fn v0375_domain_views_replace_legacy_definitions() {
    let state_adapter = "src/app/workflow_adapter/state.rs";
    let transcript_adapter = "src/app/workflow_adapter/transcript.rs";
    let transcript_ledger_projection = "src/app/workflow_adapter/transcript/ledger_projection.rs";
    let transcript_read_model = "src/app/workflow_adapter/transcript/read_model.rs";
    let transcript_recording = "src/app/workflow_adapter/transcript/recording.rs";
    let transcript_storage = "src/app/workflow_adapter/transcript/storage.rs";
    let transcript_storage_contract = "src/app/workflow_adapter/transcript/storage/contract.rs";
    let transcript_storage_paths = "src/app/workflow_adapter/transcript/storage/path_resolution.rs";
    let transcript_storage_records =
        "src/app/workflow_adapter/transcript/storage/record_repository.rs";
    let transcript_storage_tools = "src/app/workflow_adapter/transcript/storage/tool_artifact.rs";
    let transcript_tool_turn = "src/app/workflow_adapter/transcript/tool_turn.rs";
    let transcript_tool_decoding = "src/app/workflow_adapter/transcript/tool_turn/decoding.rs";
    let transcript_tool_installation =
        "src/app/workflow_adapter/transcript/tool_turn/installation.rs";
    let transcript_tool_preparation =
        "src/app/workflow_adapter/transcript/tool_turn/preparation.rs";
    let transcript_streams = "src/app/workflow_adapter/transcript/tool_turn/streams.rs";
    let transcript_tool_types = "src/app/workflow_adapter/transcript/tool_turn/types.rs";
    let transcript_tool_view = "src/app/workflow_adapter/transcript/tool_turn/view.rs";
    let transcript_tests = "src/app/workflow_adapter/transcript/tests.rs";
    for target in [
        "src/runtime_core/workflow/domain/mod.rs",
        "src/runtime_core/workflow/domain/snapshot.rs",
        "src/runtime_core/workflow/domain/snapshot/lease.rs",
        "src/runtime_core/workflow/domain/snapshot/session.rs",
        "src/runtime_core/workflow/domain/snapshot/tui_read.rs",
        "src/runtime_core/workflow/domain/snapshot/types.rs",
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
    let snapshot_lease =
        fs::read_to_string("src/runtime_core/workflow/domain/snapshot/lease.rs").unwrap();
    let snapshot_session =
        fs::read_to_string("src/runtime_core/workflow/domain/snapshot/session.rs").unwrap();
    let snapshot_tui_read =
        fs::read_to_string("src/runtime_core/workflow/domain/snapshot/tui_read.rs").unwrap();
    let snapshot_types =
        fs::read_to_string("src/runtime_core/workflow/domain/snapshot/types.rs").unwrap();
    for owner in ["lease", "session", "tui_read", "types"] {
        let declaration = format!("mod {owner};");
        assert!(
            snapshot.lines().any(|line| line == declaration),
            "snapshot facade is missing child owner: {owner}"
        );
    }
    for (owner, rule) in [
        (&snapshot_session, "fn validate_session_resume_target"),
        (&snapshot_lease, "fn validate_current_lease"),
        (&snapshot_tui_read, "fn validate_read_only_workflow"),
    ] {
        assert!(
            owner.contains(rule),
            "snapshot owner is missing domain rule: {rule}"
        );
        assert!(
            !snapshot.contains(rule),
            "snapshot facade still owns domain rule: {rule}"
        );
    }
    for value_object in [
        "struct CurrentWorkflowBinding",
        "struct CurrentStateSnapshot",
        "struct CurrentStateLeaseView",
        "struct TuiStateSnapshot",
    ] {
        assert!(
            snapshot_types.contains(value_object),
            "snapshot types owner is missing: {value_object}"
        );
        assert!(!snapshot.contains(value_object));
    }
    assert!(
        snapshot
            .split("#[cfg(test)]")
            .next()
            .unwrap()
            .lines()
            .count()
            < 40
    );
    assert!(snapshot_lease.lines().count() < 75);
    assert!(snapshot_session.lines().count() < 75);
    assert!(snapshot_tui_read.lines().count() < 175);
    assert!(snapshot_types.lines().count() < 75);

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
    assert!(Path::new(transcript_ledger_projection).is_file());
    assert!(Path::new(transcript_read_model).is_file());
    assert!(Path::new(transcript_recording).is_file());
    assert!(Path::new(transcript_storage_contract).is_file());
    assert!(Path::new(transcript_storage_paths).is_file());
    assert!(Path::new(transcript_storage_records).is_file());
    assert!(Path::new(transcript_storage_tools).is_file());
    assert!(Path::new(transcript_tool_turn).is_file());
    assert!(Path::new(transcript_tool_decoding).is_file());
    assert!(Path::new(transcript_tool_installation).is_file());
    assert!(Path::new(transcript_tool_preparation).is_file());
    assert!(Path::new(transcript_streams).is_file());
    assert!(Path::new(transcript_tool_types).is_file());
    assert!(Path::new(transcript_tool_view).is_file());
    assert!(Path::new(transcript_tests).is_file());
    let transcript_adapter_source = fs::read_to_string(transcript_adapter).unwrap();
    let transcript_ledger_projection_source =
        fs::read_to_string(transcript_ledger_projection).unwrap();
    let transcript_read_model_source = fs::read_to_string(transcript_read_model).unwrap();
    let transcript_recording_source = fs::read_to_string(transcript_recording).unwrap();
    let transcript_storage_source = fs::read_to_string(transcript_storage).unwrap();
    let transcript_storage_contract_source =
        fs::read_to_string(transcript_storage_contract).unwrap();
    let transcript_storage_path_source = fs::read_to_string(transcript_storage_paths).unwrap();
    let transcript_storage_record_source = fs::read_to_string(transcript_storage_records).unwrap();
    let transcript_storage_tool_source = fs::read_to_string(transcript_storage_tools).unwrap();
    let transcript_tool_turn_source = fs::read_to_string(transcript_tool_turn).unwrap();
    let transcript_tool_decoding_source = fs::read_to_string(transcript_tool_decoding).unwrap();
    let transcript_tool_installation_source =
        fs::read_to_string(transcript_tool_installation).unwrap();
    let transcript_tool_preparation_source =
        fs::read_to_string(transcript_tool_preparation).unwrap();
    let transcript_stream_source = fs::read_to_string(transcript_streams).unwrap();
    let transcript_tool_types_source = fs::read_to_string(transcript_tool_types).unwrap();
    let transcript_tool_view_source = fs::read_to_string(transcript_tool_view).unwrap();
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
    for owner in ["ledger_projection", "read_model", "recording"] {
        assert!(
            transcript_adapter_source
                .lines()
                .any(|line| line == format!("mod {owner};")),
            "transcript adapter does not register its {owner} owner"
        );
    }
    for (owner, responsibilities) in [
        (
            transcript_recording_source.as_str(),
            &["pub fn record_workflow_turn(", "pub(super) fn record_turn("][..],
        ),
        (
            transcript_read_model_source.as_str(),
            &[
                "pub fn records_for_session(",
                "pub fn record_from_event(",
                "pub fn record_from_binding(",
            ][..],
        ),
        (
            transcript_ledger_projection_source.as_str(),
            &[
                "pub(super) fn ensure_ledger_event_under_guard(",
                "pub(super) fn transcript_ledger_event(",
            ][..],
        ),
    ] {
        for responsibility in responsibilities {
            assert!(
                owner.contains(responsibility),
                "transcript responsibility owner is missing: {responsibility}"
            );
            assert!(
                !transcript_adapter_source.contains(responsibility),
                "transcript facade still owns: {responsibility}"
            );
        }
    }
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
    for owner in [
        "decoding",
        "installation",
        "preparation",
        "streams",
        "types",
        "view",
    ] {
        assert!(
            transcript_tool_turn_source
                .lines()
                .any(|line| line == format!("mod {owner};")),
            "transcript tool-turn facade does not register {owner}"
        );
    }
    for (owner, responsibility) in [
        (
            &transcript_tool_types_source,
            "pub(crate) struct PreparedTranscriptTurn",
        ),
        (
            &transcript_tool_preparation_source,
            "pub(crate) fn prepare_no_stream_tool_turn(",
        ),
        (
            &transcript_tool_installation_source,
            "pub(crate) fn install_prepared_no_stream_tool_turn(",
        ),
        (
            &transcript_tool_decoding_source,
            "pub(crate) fn decode_prepared_no_stream_tool_turn(",
        ),
        (
            &transcript_tool_view_source,
            "pub(crate) fn tool_output_view_from_canonical_record(",
        ),
    ] {
        assert!(
            owner.contains(responsibility),
            "transcript tool-turn responsibility owner is missing: {responsibility}"
        );
        assert!(
            !transcript_tool_turn_source.contains(responsibility),
            "transcript tool-turn facade still owns: {responsibility}"
        );
        assert!(
            !transcript_adapter_source.contains(responsibility),
            "transcript adapter still owns tool-turn behavior: {responsibility}"
        );
    }
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
    for (owner, source, line_budget) in [
        ("facade", &transcript_adapter_source, 60),
        ("recording", &transcript_recording_source, 200),
        ("read model", &transcript_read_model_source, 75),
        (
            "ledger projection",
            &transcript_ledger_projection_source,
            250,
        ),
    ] {
        assert!(
            source.lines().count() < line_budget,
            "transcript {owner} exceeded its {line_budget}-line budget"
        );
    }
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
    for (owner, source, line_budget) in [
        ("facade", &transcript_tool_turn_source, 30),
        ("decoding", &transcript_tool_decoding_source, 125),
        ("installation", &transcript_tool_installation_source, 100),
        ("preparation", &transcript_tool_preparation_source, 175),
        ("stream policy", &transcript_stream_source, 250),
        ("types", &transcript_tool_types_source, 150),
        ("view", &transcript_tool_view_source, 100),
    ] {
        assert!(
            source.lines().count() < line_budget,
            "transcript tool-turn {owner} exceeded its {line_budget}-line budget"
        );
    }
    assert!(
        transcript_test_source.lines().count() < 425,
        "transcript regression module regrew beyond its ownership boundary"
    );
}
