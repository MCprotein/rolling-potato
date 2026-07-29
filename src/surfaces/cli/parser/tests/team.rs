use super::*;

#[test]
fn parses_team_status() {
    let command = parse(["team".to_string(), "status".to_string()]).unwrap();
    assert_eq!(command, Command::Team(TeamCommand::Status));
}

#[test]
fn parses_team_plan_manifest() {
    let command = parse([
        "team".to_string(),
        "plan".to_string(),
        "--manifest".to_string(),
        "plans/team.json".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Team(TeamCommand::Plan {
            manifest_path: "plans/team.json".to_string()
        })
    );
}

#[test]
fn team_plan_requires_exactly_one_manifest() {
    for args in [
        vec!["team", "plan"],
        vec!["team", "plan", "--manifest"],
        vec![
            "team",
            "plan",
            "--manifest",
            "one.json",
            "--manifest",
            "two.json",
        ],
    ] {
        assert_eq!(
            parse(args.into_iter().map(str::to_string))
                .unwrap_err()
                .code,
            2
        );
    }
}

#[test]
fn parses_team_execute_id() {
    let command = parse([
        "team".to_string(),
        "execute".to_string(),
        "--team".to_string(),
        "team-execution".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Team(TeamCommand::Execute {
            team_id: "team-execution".to_string()
        })
    );
}

#[test]
fn team_execute_requires_exactly_one_id() {
    for args in [
        vec!["team", "execute"],
        vec!["team", "execute", "--team"],
        vec!["team", "execute", "--team", "one", "--team", "two"],
    ] {
        assert_eq!(
            parse(args.into_iter().map(str::to_string))
                .unwrap_err()
                .code,
            2
        );
    }
}

#[test]
fn parses_team_reconcile_id() {
    let command = parse([
        "team".to_string(),
        "reconcile".to_string(),
        "--team".to_string(),
        "team-execution".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Team(TeamCommand::Reconcile {
            team_id: "team-execution".to_string()
        })
    );
}

#[test]
fn team_reconcile_requires_exactly_one_id() {
    for args in [
        vec!["team", "reconcile"],
        vec!["team", "reconcile", "--team"],
        vec!["team", "reconcile", "--team", "one", "--team", "two"],
    ] {
        assert_eq!(
            parse(args.into_iter().map(str::to_string))
                .unwrap_err()
                .code,
            2
        );
    }
}

#[test]
fn parses_team_cancel_id() {
    let command = parse([
        "team".to_string(),
        "cancel".to_string(),
        "--team".to_string(),
        "team-execution".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Team(TeamCommand::Cancel {
            team_id: "team-execution".to_string()
        })
    );
}

#[test]
fn team_cancel_requires_exactly_one_id() {
    for args in [
        vec!["team", "cancel"],
        vec!["team", "cancel", "--team"],
        vec!["team", "cancel", "--team", "one", "--team", "two"],
    ] {
        assert_eq!(
            parse(args.into_iter().map(str::to_string))
                .unwrap_err()
                .code,
            2
        );
    }
}

#[test]
fn parses_team_admit_with_lanes() {
    let command = parse([
        "team".to_string(),
        "admit".to_string(),
        "--lanes".to_string(),
        "3".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Team(TeamCommand::Admit {
            lanes: 3,
            write_paths: Vec::new(),
            owned_write_paths: Vec::new(),
            commands: Vec::new()
        })
    );
}

#[test]
fn parses_team_admit_policy_preflight() {
    let command = parse([
        "team".to_string(),
        "admit".to_string(),
        "--lanes".to_string(),
        "2".to_string(),
        "--write".to_string(),
        "README.md".to_string(),
        "--command".to_string(),
        "cargo".to_string(),
        "test".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Team(TeamCommand::Admit {
            lanes: 2,
            write_paths: vec!["README.md".to_string()],
            owned_write_paths: Vec::new(),
            commands: vec!["cargo test".to_string()]
        })
    );
}

#[test]
fn parses_team_admit_file_ownership_preflight() {
    let command = parse([
        "team".to_string(),
        "admit".to_string(),
        "--lanes".to_string(),
        "2".to_string(),
        "--write-owner".to_string(),
        "1:src/app.rs".to_string(),
        "--write-owner".to_string(),
        "2:src/cli.rs".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Team(TeamCommand::Admit {
            lanes: 2,
            write_paths: Vec::new(),
            owned_write_paths: vec![(1, "src/app.rs".to_string()), (2, "src/cli.rs".to_string())],
            commands: Vec::new()
        })
    );
}

#[test]
fn parses_team_dispatch_file_ownership_preflight() {
    let command = parse([
        "team".to_string(),
        "dispatch".to_string(),
        "--lanes".to_string(),
        "2".to_string(),
        "--write-owner".to_string(),
        "1:src/app.rs".to_string(),
        "--write-owner".to_string(),
        "2:src/cli.rs".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Team(TeamCommand::Dispatch {
            lanes: 2,
            owned_write_paths: vec![(1, "src/app.rs".to_string()), (2, "src/cli.rs".to_string())],
            failed_lane: None,
            failure_reason: None,
        })
    );
}

#[test]
fn parses_team_dispatch_failed_lane_continuation() {
    let command = parse([
        "team".to_string(),
        "dispatch".to_string(),
        "--lanes".to_string(),
        "3".to_string(),
        "--write-owner".to_string(),
        "1:src/app.rs".to_string(),
        "--write-owner".to_string(),
        "2:src/cli.rs".to_string(),
        "--failed-lane".to_string(),
        "2".to_string(),
        "--failure".to_string(),
        "worker".to_string(),
        "timed".to_string(),
        "out".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Team(TeamCommand::Dispatch {
            lanes: 3,
            owned_write_paths: vec![(1, "src/app.rs".to_string()), (2, "src/cli.rs".to_string())],
            failed_lane: Some(2),
            failure_reason: Some("worker timed out".to_string()),
        })
    );
}

#[test]
fn parses_team_governor() {
    let command = parse([
        "team".to_string(),
        "governor".to_string(),
        "--lanes".to_string(),
        "2".to_string(),
        "--context-tokens".to_string(),
        "6000".to_string(),
        "--context-limit".to_string(),
        "8192".to_string(),
        "--model-tier".to_string(),
        "standard".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Team(TeamCommand::Governor {
            lanes: 2,
            context_tokens: 6000,
            context_limit: Some(8192),
            model_tier: ModelTier::Standard
        })
    );
}

#[test]
fn rejects_unknown_team_governor_model_tier() {
    let err = parse([
        "team".to_string(),
        "governor".to_string(),
        "--lanes".to_string(),
        "2".to_string(),
        "--context-tokens".to_string(),
        "6000".to_string(),
        "--model-tier".to_string(),
        "frontier".to_string(),
    ])
    .unwrap_err();
    assert_eq!(err.code, 2);
    assert!(err.message.contains("small, standard, large"));
}

#[test]
fn rejects_team_admit_write_owner_outside_requested_lanes() {
    let err = parse([
        "team".to_string(),
        "admit".to_string(),
        "--lanes".to_string(),
        "2".to_string(),
        "--write-owner".to_string(),
        "3:src/app.rs".to_string(),
    ])
    .unwrap_err();
    assert_eq!(err.code, 2);
    assert!(err.message.contains("--lanes 2"));
}

#[test]
fn rejects_team_dispatch_without_write_owner() {
    let err = parse([
        "team".to_string(),
        "dispatch".to_string(),
        "--lanes".to_string(),
        "2".to_string(),
    ])
    .unwrap_err();
    assert_eq!(err.code, 2);
    assert!(err.message.contains("--write-owner"));
}

#[test]
fn rejects_team_dispatch_failure_without_failed_lane() {
    let err = parse([
        "team".to_string(),
        "dispatch".to_string(),
        "--lanes".to_string(),
        "2".to_string(),
        "--write-owner".to_string(),
        "1:src/app.rs".to_string(),
        "--failure".to_string(),
        "worker".to_string(),
        "timed".to_string(),
        "out".to_string(),
    ])
    .unwrap_err();
    assert_eq!(err.code, 2);
    assert!(err.message.contains("--failed-lane"));
}

#[test]
fn rejects_zero_team_admit_lanes() {
    let err = parse([
        "team".to_string(),
        "admit".to_string(),
        "--lanes".to_string(),
        "0".to_string(),
    ])
    .unwrap_err();
    assert_eq!(err.code, 2);
    assert!(err.message.contains("1 이상"));
}
