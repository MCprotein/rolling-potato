#[test]
fn ontology_application_adapter_delegates_to_bounded_owners() {
    let facade_path = "src/app/ontology_adapter.rs";
    let facade = fs::read_to_string(facade_path).unwrap();
    assert!(
        facade.contains("#[path = \"ontology_adapter/tests.rs\"]"),
        "ontology facade does not register its targeted-test owner"
    );
    for owner in [
        "exchange",
        "lifecycle",
        "project_paths",
        "projection",
        "reporting",
        "seeding",
        "source_reader",
    ] {
        assert!(
            facade.lines().any(|line| line == format!("mod {owner};")),
            "ontology facade does not register {owner}"
        );
    }

    for (owner, line_budget, responsibilities) in [
        (
            "exchange",
            90,
            &["fn export_report(", "fn import_report("][..],
        ),
        ("lifecycle", 100, &["fn ensure_seeded(", "fn now_ms("][..]),
        (
            "project_paths",
            50,
            &["fn canonical_project_root(", "fn relative_to_root("][..],
        ),
        (
            "projection",
            70,
            &[
                "fn load_projection(",
                "fn record_source_is_stale(",
                "fn runtime_context(",
            ][..],
        ),
        (
            "reporting",
            150,
            &[
                "fn seed_report(",
                "fn status_report(",
                "fn inspect_report(",
                "fn context_report(",
                "fn doctor_summary(",
            ][..],
        ),
        (
            "seeding",
            350,
            &[
                "fn ensure_layout(",
                "fn seed_candidates(",
                "fn collect_indexable_files(",
                "fn append_records(",
            ][..],
        ),
        (
            "source_reader",
            250,
            &[
                "fn reread_runtime_source(",
                "fn reread_historical_source(",
                "fn resolve_project_relative_file(",
            ][..],
        ),
    ] {
        let path = format!("src/app/ontology_adapter/{owner}.rs");
        let source = fs::read_to_string(&path).unwrap();
        assert!(
            source.lines().count() < line_budget,
            "ontology adapter owner {owner} exceeded its {line_budget}-line budget"
        );
        for responsibility in responsibilities {
            assert!(
                source.contains(responsibility),
                "ontology adapter owner {owner} is missing {responsibility}"
            );
            assert!(
                !facade.contains(responsibility),
                "ontology facade still owns {responsibility}"
            );
        }
    }

    let tests = fs::read_to_string("src/app/ontology_adapter/tests.rs").unwrap();
    assert!(tests.lines().count() < 200);
    for regression in [
        "fn seed_creates_store_and_context_view(",
        "fn seed_excludes_agent_and_runtime_state_directories(",
        "fn changed_layer_a_seed_appends_superseding_revision(",
        "fn runtime_context_binds_reread_to_graph_hash(",
        "fn historical_reread_drops_a_missing_source_but_strict_reread_rejects_it(",
        "fn import_blocks_confirmed_semantic_claim_without_source(",
    ] {
        assert!(
            tests.contains(regression),
            "ontology targeted-test owner is missing {regression}"
        );
    }
    assert!(
        facade.lines().count() < 50,
        "ontology application facade exceeded its 50-line budget"
    );
}
