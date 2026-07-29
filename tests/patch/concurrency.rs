use super::*;

#[test]
fn token_rotate_recovers_lost_delivery_and_invalidates_old_token_across_processes() {
    let fixture = fixture("token-rotate-subprocess");
    fixture.start();
    let run = fixture.command(&["run", "src/lib.rs 테스트 값을 고쳐줘"]);
    assert!(run.status.success());
    let out = String::from_utf8(run.stdout).unwrap();
    let proposal = field(&out, "proposal id");
    let old_token = out
        .lines()
        .find_map(|line| line.strip_prefix("- approval command: rpotato patch approve "))
        .unwrap()
        .split(" --token ")
        .nth(1)
        .unwrap()
        .to_string();

    let rotate = fixture.command(&["patch", "token-rotate", &proposal]);
    assert!(rotate.status.success());
    let rotate_out = String::from_utf8(rotate.stdout).unwrap();
    let new_token = field(&rotate_out, "새 approval token");
    let old = fixture.command(&[
        "patch",
        "approve",
        &proposal,
        "--token",
        &old_token,
        "--dry-run",
    ]);
    let new = fixture.command(&[
        "patch",
        "approve",
        &proposal,
        "--token",
        &new_token,
        "--dry-run",
    ]);

    assert_eq!(old.status.code(), Some(3));
    assert!(new.status.success());
    assert!(
        !fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl"))
            .unwrap()
            .contains(&old_token)
    );
}

#[test]
fn concurrent_approve_processes_create_one_apply_receipt() {
    let fixture = fixture("concurrent-approve");
    fixture.start();
    let run = fixture.command(&["run", "src/lib.rs 테스트 값을 고쳐줘"]);
    assert!(run.status.success());
    let out = String::from_utf8(run.stdout).unwrap();
    let proposal = field(&out, "proposal id");
    let token = out
        .lines()
        .find_map(|line| line.strip_prefix("- approval command: rpotato patch approve "))
        .unwrap()
        .split(" --token ")
        .nth(1)
        .unwrap()
        .to_string();

    let args = [
        "patch",
        "approve",
        proposal.as_str(),
        "--token",
        token.as_str(),
    ];
    let mut first_command = fixture.command_builder(&args);
    let mut second_command = fixture.command_builder(&args);
    let first = spawn_captured(&mut first_command).unwrap();
    let second = spawn_captured(&mut second_command).unwrap();
    let first = wait_bounded(first, &args);
    let second = wait_bounded(second, &args);
    assert!(first.status.success() || second.status.success());
    let successful_output = if first.status.success() {
        String::from_utf8(first.stdout).unwrap()
    } else {
        String::from_utf8(second.stdout).unwrap()
    };
    let verify_token = verification_token(&successful_output);
    assert_eq!(
        fs::read_to_string(fixture.project.join("src/lib.rs")).unwrap(),
        "pub const VALUE: i32 = 2;\n"
    );
    let ledger = fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl")).unwrap();
    assert_eq!(
        ledger
            .lines()
            .filter(|line| line.contains("\"event_type\":\"patch.applied\""))
            .count(),
        1
    );
    assert_eq!(
        ledger
            .lines()
            .filter(|line| line.contains("\"event_type\":\"verification.evidence.recorded\""))
            .count(),
        0
    );

    let verify = fixture.command(&["patch", "verify", &proposal, "--token", &verify_token]);
    assert!(verify.status.success());
    let ledger = fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl")).unwrap();
    assert_eq!(
        ledger
            .lines()
            .filter(|line| line.contains("\"event_type\":\"verification.evidence.recorded\""))
            .count(),
        1
    );
}
