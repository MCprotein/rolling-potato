use super::*;
use std::path::PathBuf;

fn with_temp_project(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("rpotato-ontology-{name}-{}", std::process::id()));
    let project = root.join("project");
    let data = root.join("data");
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
    std::env::set_var("RPOTATO_DATA_HOME", &data);
    fs::create_dir_all(project.join("src")).unwrap();
    fs::write(project.join("Cargo.toml"), "[package]\nname = \"demo\"\n").unwrap();
    fs::write(project.join("src").join("main.rs"), "fn main() {}\n").unwrap();
    fs::write(project.join(".gitignore"), "target/\n.rpotato/\n").unwrap();
    project
}

fn clear_env() {
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
}

#[test]
fn seed_creates_store_and_context_view() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let _project = with_temp_project("seed");

    let seed = ensure_seeded().unwrap();
    let context = context_report("main").unwrap();
    let status = status_report().unwrap();

    clear_env();

    assert!(seed.records_added >= 2);
    assert!(seed.store.exists());
    assert!(context.contains("source=src/main.rs:1"));
    assert!(status.contains("sourceless confirmed Layer B claims: 0"));
}

#[test]
fn seed_excludes_agent_and_runtime_state_directories() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let project = with_temp_project("seed-excludes-runtime-state");
    for directory in [".omx", ".omc", ".codex", ".agents"] {
        let state_dir = project.join(directory);
        fs::create_dir_all(&state_dir).unwrap();
        fs::write(state_dir.join("runtime.md"), "ephemeral runtime state\n").unwrap();
    }

    ensure_seeded().unwrap();
    let store = fs::read_to_string(paths::project_ontology_store_file()).unwrap();

    clear_env();

    for directory in [".omx", ".omc", ".codex", ".agents"] {
        assert!(
            !store.contains(directory),
            "{directory} must not be indexed as project knowledge"
        );
    }
}

#[test]
fn reread_rejects_parent_path_escape() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let _project = with_temp_project("reread-escape");

    let err = reread_report("../secret.txt:1").unwrap_err();

    clear_env();

    assert_eq!(err.code, 3);
}

#[test]
fn changed_layer_a_seed_appends_superseding_revision() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let project = with_temp_project("supersedes");

    ensure_seeded().unwrap();
    fs::write(
        project.join("src").join("main.rs"),
        "fn main() { println!(\"hi\"); }\n",
    )
    .unwrap();
    let seed = ensure_seeded().unwrap();
    let store = fs::read_to_string(paths::project_ontology_store_file()).unwrap();

    clear_env();

    assert_eq!(seed.records_added, 2);
    assert!(store.contains("\"supersedes\":\"a:file:"));
    assert!(store.contains("\"supersedes\":\"a:entrypoint:"));
}

#[test]
fn runtime_context_binds_reread_to_graph_hash() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let project = with_temp_project("runtime-context");
    ensure_seeded().unwrap();

    let selection = runtime_context("main", 4).unwrap();
    let record = selection
        .selected
        .iter()
        .find(|record| record.source_pointer == "src/main.rs:1")
        .unwrap();
    let source = reread_runtime_source(&record.source_pointer, &record.source_hash).unwrap();
    assert_eq!(source.relative_path, "src/main.rs");
    assert_eq!(source.contents, "fn main() {}\n");

    fs::write(project.join("src/main.rs"), "fn main() { panic!(); }\n").unwrap();
    let err = reread_runtime_source(&record.source_pointer, &record.source_hash).unwrap_err();
    clear_env();

    assert_eq!(err.code, 3);
    assert!(err.message.contains("graph source hash"));
}

#[test]
fn historical_reread_drops_a_missing_source_but_strict_reread_rejects_it() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let project = with_temp_project("missing-historical-source");
    let source = project.join("src/main.rs");
    let hash = crate::foundation::integrity::sha256_file(&source).unwrap();
    fs::remove_file(&source).unwrap();

    let historical = reread_historical_source("src/main.rs:1", &hash);
    let strict = reread_runtime_source("src/main.rs:1", &hash);
    clear_env();

    assert_eq!(historical.unwrap(), None);
    assert!(strict.is_err());
}

#[test]
fn import_blocks_confirmed_semantic_claim_without_source() {
    let text = r#"{"schemaVersion":1,"id":"b:one","layer":"B","kind":"invariant","label":"must be true","status":"confirmed","claimState":"confirmed","sourcePointer":"none","sourceHash":""}"#;

    let err = validate_import_text(text).unwrap_err();

    assert_eq!(err.code, 3);
    assert!(err.message.contains("confirmed Layer B"));
}

#[test]
fn import_accepts_source_backed_confirmed_semantic_claim() {
    let text = r#"{"schemaVersion":1,"id":"b:one","layer":"B","kind":"invariant","label":"must be true","status":"confirmed","claimState":"confirmed","sourcePointer":"docs/design.md:10","sourceHash":"abc"}"#;

    let validation = validate_import_text(text).unwrap();

    assert_eq!(validation.records, 1);
}
