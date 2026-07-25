use super::*;

#[test]
fn stale_historical_source_does_not_block_unrelated_next_request() {
    let fixture = fixture("resume-preflight-no-mutation");
    fs::write(
        &fixture.response,
        "src/lib.rs를 읽기 전용으로 확인했습니다.\nMODEL ACTION: kind=inspect-sources; source_pointers=src/lib.rs:1; next_gate=source-reread-before-claim; side_effects=none",
    )
    .unwrap();
    fixture.start();
    let first = fixture.command(&["run", "저장소 구조를 분석해줘"]);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );

    fs::write(
        fixture.project.join("src/lib.rs"),
        "pub const VALUE: i32 = 99;\n",
    )
    .unwrap();
    fs::write(
        &fixture.response,
        "5 곱하기 3은 15입니다.\nMODEL ACTION: kind=answer-only; source_pointers=none; next_gate=korean-output-guard; side_effects=none",
    )
    .unwrap();

    let continued = fixture.command(&["run", "5 곱하기 3은?"]);
    assert!(
        continued.status.success(),
        "{}",
        String::from_utf8_lossy(&continued.stderr)
    );
    assert!(!String::from_utf8_lossy(&continued.stderr).contains("source reread 차단"));
    assert_eq!(
        fs::read_to_string(&fixture.calls).unwrap().lines().count(),
        2
    );
}

#[test]
fn session_resume_validates_workflow_before_selecting_target_session() {
    let fixture = fixture("session-resume-preflight");
    fixture.start();
    let run = fixture.command(&["run", "src/lib.rs의 값을 2로 고쳐줘"]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let run_out = String::from_utf8(run.stdout).unwrap();
    let proposal_id = field(&run_out, "proposal id");
    let list = fixture.command(&["session", "list"]);
    let target_session = String::from_utf8(list.stdout)
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("- current session: "))
        .unwrap()
        .to_string();
    let new_session = fixture.command(&["session", "new"]);
    assert!(new_session.status.success());

    let proposal_path = fixture
        .project
        .join(".rpotato/patch-proposals")
        .join(format!("{proposal_id}.txt"));
    let proposal_body = fs::read_to_string(&proposal_path).unwrap();
    let workflow_field = proposal_body
        .lines()
        .find(|line| line.starts_with("workflow_id="))
        .unwrap();
    let tampered =
        proposal_body.replacen(workflow_field, "workflow_id=workflow-binding-tampered", 1);
    fs::write(&proposal_path, tampered).unwrap();
    let current_state_path = fixture.project.join(".rpotato/state/current-state.json");
    let ledger_path = fixture.data.join("state/runtime-ledger.jsonl");
    let state_before = fs::read(&current_state_path).unwrap();
    let ledger_before = fs::read(&ledger_path).unwrap();

    let blocked = fixture.command(&["resume", &target_session]);
    assert_eq!(blocked.status.code(), Some(3));
    assert_eq!(fs::read(&current_state_path).unwrap(), state_before);
    assert_eq!(fs::read(&ledger_path).unwrap(), ledger_before);
    assert_eq!(
        fs::read_to_string(&fixture.calls).unwrap().lines().count(),
        1
    );
}
