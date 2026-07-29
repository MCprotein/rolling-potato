use super::*;

#[test]
fn parses_subagent_launch_status_and_cancel() {
    let command = parse([
        "subagent".to_string(),
        "launch".to_string(),
        "--role".to_string(),
        "executor".to_string(),
        "--task".to_string(),
        "bounded change".to_string(),
        "--tool".to_string(),
        "read_file".to_string(),
        "--tool".to_string(),
        "render_diff".to_string(),
        "--read".to_string(),
        "src/main.rs".to_string(),
        "--write".to_string(),
        "src/main.rs".to_string(),
        "--timeout-ms".to_string(),
        "1000".to_string(),
        "--max-tokens".to_string(),
        "128".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Subagent(SubagentCommand::Launch {
            role: "executor".to_string(),
            task: "bounded change".to_string(),
            tools: vec!["read_file".to_string(), "render_diff".to_string()],
            read_paths: vec!["src/main.rs".to_string()],
            write_paths: vec!["src/main.rs".to_string()],
            timeout_ms: Some(1000),
            max_tokens: Some(128),
        })
    );
    assert_eq!(
        parse(["subagent".to_string(), "status".to_string()]).unwrap(),
        Command::Subagent(SubagentCommand::Status { id: None })
    );
    assert_eq!(
        parse([
            "subagent".to_string(),
            "cancel".to_string(),
            "subagent-example".to_string(),
        ])
        .unwrap(),
        Command::Subagent(SubagentCommand::Cancel {
            id: "subagent-example".to_string()
        })
    );
}

#[test]
fn subagent_launch_rejects_missing_and_duplicate_singleton_options() {
    let missing = parse([
        "subagent".to_string(),
        "launch".to_string(),
        "--role".to_string(),
        "explore".to_string(),
    ])
    .unwrap_err();
    assert!(missing.message.contains("--task"));

    let duplicate = parse([
        "subagent".to_string(),
        "launch".to_string(),
        "--role".to_string(),
        "explore".to_string(),
        "--role".to_string(),
        "planner".to_string(),
        "--task".to_string(),
        "task".to_string(),
        "--tool".to_string(),
        "read_file".to_string(),
        "--read".to_string(),
        "src/main.rs".to_string(),
    ])
    .unwrap_err();
    assert!(duplicate.message.contains("한 번만"));
}

#[test]
fn parses_ontology_context_query() {
    let command = parse([
        "ontology".to_string(),
        "context".to_string(),
        "--query".to_string(),
        "runtime".to_string(),
        "entrypoint".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Ontology(OntologyCommand::Context {
            query: "runtime entrypoint".to_string()
        })
    );
}

#[test]
fn parses_ontology_reread() {
    let command = parse([
        "ontology".to_string(),
        "reread".to_string(),
        "src/main.rs:1".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Ontology(OntologyCommand::Reread {
            pointer: "src/main.rs:1".to_string()
        })
    );
}

#[test]
fn parses_ontology_export_jsonl() {
    let command = parse([
        "ontology".to_string(),
        "export".to_string(),
        "--format".to_string(),
        "jsonl".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Ontology(OntologyCommand::Export {
            format: OntologyExportFormat::Jsonl
        })
    );
}

#[test]
fn parses_ontology_import_dry_run() {
    let command = parse([
        "ontology".to_string(),
        "import".to_string(),
        "--file".to_string(),
        "ontology-view.jsonl".to_string(),
        "--dry-run".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Ontology(OntologyCommand::Import {
            path: "ontology-view.jsonl".to_string(),
            dry_run: true
        })
    );
}
