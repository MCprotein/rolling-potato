use super::*;

#[test]
fn parses_top_level_resume_as_history() {
    let command = parse(["resume".to_string()]).unwrap();
    assert_eq!(command, Command::Session(SessionCommand::List));
}

#[test]
fn parses_top_level_resume_with_id() {
    let command = parse(["resume".to_string(), "session-1".to_string()]).unwrap();
    assert_eq!(
        command,
        Command::Session(SessionCommand::Resume {
            id: "session-1".to_string()
        })
    );
}

#[test]
fn parses_top_level_continue_as_current_workflow_resume() {
    let command = parse(["continue".to_string()]).unwrap();
    assert_eq!(command, Command::State(StateCommand::Resume));
}

#[test]
fn parses_top_level_continue_with_session_id() {
    let command = parse(["continue".to_string(), "session-1".to_string()]).unwrap();
    assert_eq!(
        command,
        Command::Session(SessionCommand::Resume {
            id: "session-1".to_string()
        })
    );
}

#[test]
fn parses_debug_help_as_advanced_help() {
    assert_eq!(
        parse(["debug".to_string(), "--help".to_string()]).unwrap(),
        Command::AdvancedHelp
    );
}

#[test]
fn parses_existing_commands_beneath_debug_namespace() {
    assert_eq!(
        parse([
            "debug".to_string(),
            "backend".to_string(),
            "status".to_string(),
        ])
        .unwrap(),
        Command::Backend(BackendCommand::Status)
    );
}

#[test]
fn parses_evidence_validate() {
    let command = parse([
        "evidence".to_string(),
        "validate".to_string(),
        "logs/test.log".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Evidence(EvidenceCommand::Validate {
            pointer: "logs/test.log".to_string()
        })
    );
}

#[test]
fn parses_skill_run() {
    let command = parse([
        "skill".to_string(),
        "run".to_string(),
        "fix-test".to_string(),
        "테스트".to_string(),
        "고쳐줘".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Skill(SkillCommand::Run {
            id: "fix-test".to_string(),
            request: "테스트 고쳐줘".to_string()
        })
    );
}

#[test]
fn skill_run_requires_request() {
    let error = parse([
        "skill".to_string(),
        "run".to_string(),
        "fix-test".to_string(),
    ])
    .unwrap_err();

    assert_eq!(error.code, 2);
    assert!(error.message.contains("request 문자열"));
}

#[test]
fn parses_run_request() {
    let command = parse([
        "run".to_string(),
        "테스트".to_string(),
        "고쳐줘".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Run {
            request: "테스트 고쳐줘".to_string()
        }
    );
}

#[test]
fn parses_intent_classify_request() {
    let command = parse([
        "intent".to_string(),
        "classify".to_string(),
        "리뷰해줘".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Intent(IntentCommand::Classify {
            request: "리뷰해줘".to_string()
        })
    );
}

#[test]
fn parses_intent_routes() {
    let command = parse(["intent".to_string(), "routes".to_string()]).unwrap();
    assert_eq!(command, Command::Intent(IntentCommand::Routes));
}

#[test]
fn parses_policy_check_command() {
    let command = parse([
        "policy".to_string(),
        "check-command".to_string(),
        "cargo".to_string(),
        "test".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Policy(PolicyCommand::CheckCommand {
            command: "cargo test".to_string()
        })
    );
}

#[test]
fn parses_policy_check_path_write() {
    let command = parse([
        "policy".to_string(),
        "check-path".to_string(),
        "--write".to_string(),
        "src/main.rs".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Policy(PolicyCommand::CheckPath {
            mode: PolicyPathMode::Write,
            path: "src/main.rs".to_string()
        })
    );
}

#[test]
fn parses_hooks_list() {
    let command = parse(["hooks".to_string(), "list".to_string()]).unwrap();
    assert_eq!(command, Command::Hooks(HooksCommand::List));
}

#[test]
fn parses_monitor_export_jsonl() {
    let command = parse([
        "monitor".to_string(),
        "export".to_string(),
        "--format".to_string(),
        "jsonl".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Monitor(MonitorCommand::Export {
            format: MonitorExportFormat::Jsonl
        })
    );
}

#[test]
fn parses_monitor_export_html() {
    let command = parse([
        "monitor".to_string(),
        "export".to_string(),
        "--format".to_string(),
        "html".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Monitor(MonitorCommand::Export {
            format: MonitorExportFormat::Html
        })
    );
}

#[test]
fn parses_monitor_prune_dry_run() {
    let command = parse([
        "monitor".to_string(),
        "prune".to_string(),
        "--before".to_string(),
        "30d".to_string(),
        "--dry-run".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Monitor(MonitorCommand::Prune {
            before_days: 30,
            dry_run: true
        })
    );
}
