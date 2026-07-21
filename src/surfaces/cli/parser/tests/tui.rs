use super::*;

#[test]
fn parses_no_arguments_as_default_tui() {
    let command = parse(Vec::<String>::new()).unwrap();
    assert_eq!(command, Command::Tui(TuiCommand::Auto));
}

#[test]
fn parses_tui_overview() {
    let command = parse(["tui".to_string()]).unwrap();
    assert_eq!(command, Command::Tui(TuiCommand::Auto));
}

#[test]
fn parses_explicit_interactive_tui() {
    let command = parse(["tui".to_string(), "interactive".to_string()]).unwrap();
    assert_eq!(command, Command::Tui(TuiCommand::Interactive));
}

#[test]
fn parses_tui_monitor() {
    let command = parse(["tui".to_string(), "monitor".to_string()]).unwrap();
    assert_eq!(command, Command::Tui(TuiCommand::Monitor));
}

#[test]
fn parses_tui_sessions() {
    let command = parse(["tui".to_string(), "sessions".to_string()]).unwrap();
    assert_eq!(command, Command::Tui(TuiCommand::Sessions));
}

#[test]
fn parses_tui_transcript() {
    let command = parse([
        "tui".to_string(),
        "transcript".to_string(),
        "session-1".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Tui(TuiCommand::Transcript {
            session_id: "session-1".to_string()
        })
    );
}

#[test]
fn parses_tui_approvals() {
    let command = parse(["tui".to_string(), "approvals".to_string()]).unwrap();
    assert_eq!(command, Command::Tui(TuiCommand::Approvals));
}

#[test]
fn parses_tui_diff() {
    let command = parse([
        "tui".to_string(),
        "diff".to_string(),
        "patch-proposal-abc123".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Tui(TuiCommand::Diff {
            proposal_id: "patch-proposal-abc123".to_string()
        })
    );
}

#[test]
fn parses_tui_evidence() {
    let command = parse(["tui".to_string(), "evidence".to_string()]).unwrap();
    assert_eq!(command, Command::Tui(TuiCommand::Evidence));
}
