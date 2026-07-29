#[test]
fn v03713_context_adapter_separates_filesystem_discovery() {
    let context_adapter = "src/app/context_adapter.rs";
    let context_compaction = "src/app/context_adapter/compaction.rs";
    let compaction_artifact_store = "src/app/context_adapter/compaction/artifact_store.rs";
    let filesystem_discovery = "src/app/context_adapter/discovery.rs";
    let context_tests = "src/app/context_adapter/tests.rs";
    assert!(Path::new(context_adapter).is_file());
    assert!(Path::new(context_compaction).is_file());
    assert!(Path::new(compaction_artifact_store).is_file());
    assert!(Path::new(filesystem_discovery).is_file());
    assert!(Path::new(context_tests).is_file());
    assert!(!Path::new("src/context.rs").exists());
    assert!(!Path::new("src/context").exists());
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(!main.lines().any(|line| line == "mod context;"));
    let app_root = fs::read_to_string("src/app.rs").unwrap();
    assert!(
        app_root
            .lines()
            .any(|line| line == "pub(crate) mod context_adapter;"),
        "application root does not register the context adapter"
    );

    let context = fs::read_to_string(context_adapter).unwrap();
    let compaction = fs::read_to_string(context_compaction).unwrap();
    let artifact_store = fs::read_to_string(compaction_artifact_store).unwrap();
    let discovery = fs::read_to_string(filesystem_discovery).unwrap();
    let tests = fs::read_to_string(context_tests).unwrap();
    assert!(
        context.lines().any(|line| line == "mod discovery;"),
        "context adapter does not register its filesystem discovery owner"
    );
    assert!(
        context.lines().any(|line| line == "mod compaction;"),
        "context adapter does not register its compaction owner"
    );
    assert!(
        compaction.lines().any(|line| line == "mod artifact_store;"),
        "context compaction does not register its artifact-store owner"
    );
    for responsibility in [
        "pub(super) fn install_artifact(",
        "pub(crate) fn load_current_artifact(",
        "fn validate_artifact_chain(",
        "fn load_artifact_pointer(",
    ] {
        assert!(
            artifact_store.contains(responsibility),
            "compaction artifact-store owner is missing responsibility: {responsibility}"
        );
        assert!(
            !compaction.contains(responsibility),
            "compaction orchestration still owns artifact storage: {responsibility}"
        );
    }
    for responsibility in [
        "pub(super) fn build_filesystem_fallback(",
        "pub(super) fn discover_candidate_files(",
        "fn should_skip_dir(",
        "fn is_context_file(",
        "pub(super) fn request_terms(",
        "pub(super) fn score_path(",
        "pub(super) fn relative_path(",
        "pub(super) fn content_fingerprint(",
    ] {
        assert!(
            discovery.contains(responsibility),
            "filesystem discovery owner is missing responsibility: {responsibility}"
        );
        assert!(
            !context.contains(responsibility),
            "context orchestration still owns filesystem discovery: {responsibility}"
        );
    }
    assert!(
        tests.contains("fn filesystem_discovery_skips_generated_dirs_and_ranks_request_matches(")
    );
    assert!(context.lines().count() < 600);
    assert!(compaction.lines().count() < 550);
    assert!(artifact_store.lines().count() < 350);
    assert!(discovery.lines().count() < 250);
}

#[test]
fn v03713_evidence_adapter_separates_responsibility_owners() {
    let evidence_adapter = "src/app/evidence_adapter.rs";
    let evidence_recording = "src/app/evidence_adapter/recording.rs";
    let evidence_stop_gate = "src/app/evidence_adapter/stop_gate.rs";
    let evidence_pointer = "src/app/evidence_adapter/artifact_pointer.rs";
    let evidence_store = "src/app/evidence_adapter/store.rs";
    let evidence_tests = "src/app/evidence_adapter/tests.rs";
    assert!(Path::new(evidence_adapter).is_file());
    assert!(Path::new(evidence_recording).is_file());
    assert!(Path::new(evidence_stop_gate).is_file());
    assert!(Path::new(evidence_pointer).is_file());
    assert!(Path::new(evidence_store).is_file());
    assert!(Path::new(evidence_tests).is_file());
    assert!(!Path::new("src/evidence.rs").exists());
    assert!(!Path::new("src/evidence").exists());
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(!main.lines().any(|line| line == "mod evidence;"));
    let app_root = fs::read_to_string("src/app.rs").unwrap();
    assert!(
        app_root
            .lines()
            .any(|line| line == "pub(crate) mod evidence_adapter;"),
        "application root does not register the evidence adapter"
    );

    let evidence = fs::read_to_string(evidence_adapter).unwrap();
    let recording = fs::read_to_string(evidence_recording).unwrap();
    let stop_gate = fs::read_to_string(evidence_stop_gate).unwrap();
    let pointer = fs::read_to_string(evidence_pointer).unwrap();
    let store = fs::read_to_string(evidence_store).unwrap();
    let tests = fs::read_to_string(evidence_tests).unwrap();
    for owner in [
        "mod artifact_pointer;",
        "mod recording;",
        "mod stop_gate;",
        "mod store;",
    ] {
        assert!(
            evidence.lines().any(|line| line == owner),
            "evidence adapter does not register owner: {owner}"
        );
    }
    for responsibility in [
        "pub fn store_status(",
        "pub(crate) fn store_status_bounded(",
        "fn count_jsonl_records(",
        "fn count_jsonl_records_bounded(",
        "fn count_top_level_files_bounded(",
        "fn count_files(",
    ] {
        assert!(
            store.contains(responsibility),
            "evidence store owner is missing responsibility: {responsibility}"
        );
        assert!(
            !evidence.contains(responsibility),
            "evidence orchestration still owns store inspection: {responsibility}"
        );
    }
    assert!(
        tests.contains("fn bounded_store_status_reports_scan_truncation_and_rejects_zero_budget(")
    );
    for (owner, responsibility) in [
        (&recording, "pub fn record_patch_verification("),
        (&stop_gate, "pub fn evaluate_patch_stop_gate("),
        (&stop_gate, "pub fn validate_patch_stop_gate("),
        (&pointer, "pub fn validate_artifact_pointer("),
        (&pointer, "pub fn validate_report("),
    ] {
        assert!(
            owner.contains(responsibility),
            "evidence owner is missing responsibility: {responsibility}"
        );
        assert!(
            !evidence.contains(responsibility),
            "evidence facade still owns responsibility: {responsibility}"
        );
    }
    assert!(evidence.lines().count() < 80);
    assert!(recording.lines().count() < 250);
    assert!(stop_gate.lines().count() < 250);
    assert!(pointer.lines().count() < 150);
    assert!(store.lines().count() < 200);
    assert!(tests.lines().count() < 300);
}

#[test]
fn v03713_benchmark_adapter_separates_regression_tests() {
    let adapter_path = "src/app/inference_adapter/benchmark.rs";
    let tests_path = "src/app/inference_adapter/benchmark/tests.rs";
    assert!(Path::new(adapter_path).is_file());
    assert!(Path::new(tests_path).is_file());

    let adapter = fs::read_to_string(adapter_path).unwrap();
    let tests = fs::read_to_string(tests_path).unwrap();
    assert!(
        adapter.contains("#[path = \"benchmark/tests.rs\"]"),
        "benchmark adapter does not register its regression test owner"
    );
    for regression in [
        "fn validates_fixture_metadata(",
        "fn executable_run_records_local_score_without_prompt_text(",
        "fn rejects_raw_prompt_field(",
        "fn canonical_model_adoption_fixture_is_valid(",
    ] {
        assert!(
            tests.contains(regression),
            "benchmark test owner is missing regression: {regression}"
        );
        assert!(
            !adapter.contains(regression),
            "benchmark production adapter still owns regression: {regression}"
        );
    }
    assert!(adapter.lines().count() < 350);
    assert!(tests.lines().count() < 450);
}
