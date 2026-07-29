use super::*;

#[test]
fn complete_resume_revalidates_deleted_evidence() {
    let fixture = fixture("complete-evidence-delete");
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
    let approve = fixture.command(&["patch", "approve", &proposal, "--token", &token]);
    assert!(approve.status.success());
    let approve_report = String::from_utf8(approve.stdout).unwrap();
    let verify_token = verification_token(&approve_report);
    let verify = fixture.command(&["patch", "verify", &proposal, "--token", &verify_token]);
    assert!(verify.status.success());
    let report = String::from_utf8(verify.stdout).unwrap();
    let evidence_id = field(&report, "evidence id");
    fs::remove_file(
        fixture
            .project
            .join(".rpotato/evidence")
            .join(format!("{evidence_id}.json")),
    )
    .unwrap();

    let resumed = fixture.command(&["patch", "verify", &proposal, "--token", &verify_token]);
    assert_eq!(resumed.status.code(), Some(3));
    let error = String::from_utf8_lossy(&resumed.stderr);
    assert!(error.contains("verification evidence missing"));
    assert!(!error.contains("패치 작업 완료"));
}

#[test]
fn complete_resume_revalidates_changed_source() {
    let fixture = fixture("complete-source-change");
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
    let approve = fixture.command(&["patch", "approve", &proposal, "--token", &token]);
    assert!(approve.status.success());
    let approve_report = String::from_utf8(approve.stdout).unwrap();
    let verify_token = verification_token(&approve_report);
    let verify = fixture.command(&["patch", "verify", &proposal, "--token", &verify_token]);
    assert!(verify.status.success());
    fs::write(
        fixture.project.join("src/lib.rs"),
        "pub const VALUE: i32 = 9;\n",
    )
    .unwrap();

    let resumed = fixture.command(&["patch", "verify", &proposal, "--token", &verify_token]);
    assert_eq!(resumed.status.code(), Some(3));
    let error = String::from_utf8_lossy(&resumed.stderr);
    assert!(
        error.contains("대상 파일이 preview 이후 변경되었습니다"),
        "{error}"
    );
    assert!(!error.contains("패치 작업 완료"));
}

#[test]
fn hostile_model_side_effect_text_never_executes() {
    let fixture = fixture("hostile-model");
    fs::write(
        &fixture.response,
        "파일을 즉시 덮어쓰고 rm -rf를 실행했습니다.\nMODEL ACTION: kind=patch-proposal; source_pointers=src/lib.rs:1; path=src/lib.rs; find_hex=31; replace_hex=39; verification=pwd; next_gate=diff-before-write; side_effects=write-file",
    )
    .unwrap();
    fixture.start();
    let run = fixture.command(&["run", "src/lib.rs 테스트 값을 고쳐줘"]);
    assert_eq!(run.status.code(), Some(3));
    assert_eq!(
        fs::read_to_string(fixture.project.join("src/lib.rs")).unwrap(),
        "pub const VALUE: i32 = 1;\n"
    );
    assert!(String::from_utf8_lossy(&run.stderr).contains("model side effect 실행: 없음"));
}

#[test]
fn hostile_model_path_and_malformed_hex_fail_closed() {
    for (name, action) in [
        (
            "hostile-path",
            "MODEL ACTION: kind=patch-proposal; source_pointers=../outside:1; path=../outside; find_hex=31; replace_hex=32; verification=pwd; next_gate=diff-before-write; side_effects=none",
        ),
        (
            "hostile-hex",
            "MODEL ACTION: kind=patch-proposal; source_pointers=src/lib.rs:1; path=src/lib.rs; find_hex=zz; replace_hex=32; verification=pwd; next_gate=diff-before-write; side_effects=none",
        ),
    ] {
        let fixture = fixture(name);
        fs::write(fixture.root.join("outside"), "1\n").unwrap();
        fs::write(&fixture.response, action).unwrap();
        fixture.start();
        let run = fixture.command(&["run", "src/lib.rs 테스트 값을 고쳐줘"]);
        assert_eq!(run.status.code(), Some(3), "case: {name}");
        assert_eq!(
            fs::read_to_string(fixture.project.join("src/lib.rs")).unwrap(),
            "pub const VALUE: i32 = 1;\n"
        );
        assert_eq!(
            fs::read_to_string(&fixture.calls).unwrap().lines().count(),
            1,
            "case: {name}"
        );
    }
}

#[test]
fn stale_target_and_bad_token_fail_closed_without_token_leak() {
    let fixture = fixture("stale-token");
    fixture.start();
    let run = fixture.command(&["run", "src/lib.rs 테스트 값을 고쳐줘"]);
    assert!(run.status.success());
    let run_out = String::from_utf8(run.stdout).unwrap();
    let proposal = field(&run_out, "proposal id");
    let bad = "plaintext-secret-token-never-ledger";
    let rejected = fixture.command(&["patch", "approve", &proposal, "--token", bad]);
    assert_eq!(rejected.status.code(), Some(3));
    let ledger = fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl")).unwrap();
    assert!(!ledger.contains(bad));
    assert_eq!(
        fs::read_to_string(fixture.project.join("src/lib.rs")).unwrap(),
        "pub const VALUE: i32 = 1;\n"
    );

    let token = run_out
        .lines()
        .find_map(|line| line.strip_prefix("- approval command: rpotato patch approve "))
        .unwrap()
        .split(" --token ")
        .nth(1)
        .unwrap();
    fs::write(
        fixture.project.join("src/lib.rs"),
        "pub const VALUE: i32 = 7;\n",
    )
    .unwrap();
    let stale = fixture.command(&["patch", "approve", &proposal, "--token", token]);
    assert_eq!(stale.status.code(), Some(3));
    assert_eq!(
        fs::read_to_string(fixture.project.join("src/lib.rs")).unwrap(),
        "pub const VALUE: i32 = 7;\n"
    );
}

#[test]
fn denied_verification_never_spawns_command() {
    let fixture = fixture("denied-verification");
    let marker = fixture.project.join("must-not-exist");
    fs::write(
        &fixture.response,
        "MODEL ACTION: kind=patch-proposal; source_pointers=src/lib.rs:1; path=src/lib.rs; find_hex=31; replace_hex=32; verification=touch must-not-exist; next_gate=diff-before-write; side_effects=none",
    )
    .unwrap();
    fixture.start();
    let run = fixture.command(&["run", "src/lib.rs 테스트 값을 고쳐줘"]);
    assert_eq!(run.status.code(), Some(3));
    assert!(!marker.exists());
    assert_eq!(
        fs::read_to_string(fixture.project.join("src/lib.rs")).unwrap(),
        "pub const VALUE: i32 = 1;\n"
    );
}

#[test]
fn verification_failure_restores_original_and_blocks_success() {
    let fixture = fixture("verification-rollback");
    fs::write(
        &fixture.response,
        "MODEL ACTION: kind=patch-proposal; source_pointers=src/lib.rs:1; path=src/lib.rs; find_hex=31; replace_hex=32; verification=cargo test; next_gate=diff-before-write; side_effects=none",
    )
    .unwrap();
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
        .unwrap();
    let approve = fixture.command(&["patch", "approve", &proposal, "--token", token]);
    assert!(approve.status.success());
    let approve_report = String::from_utf8(approve.stdout).unwrap();
    assert_eq!(
        fs::read_to_string(fixture.project.join("src/lib.rs")).unwrap(),
        "pub const VALUE: i32 = 2;\n"
    );
    let verify_token = verification_token(&approve_report);
    let verify = fixture.command(&["patch", "verify", &proposal, "--token", &verify_token]);
    assert_eq!(verify.status.code(), Some(3));
    let error = String::from_utf8_lossy(&verify.stderr);
    assert!(
        error.contains("verification-failed-rolled-back"),
        "status={:?}\nstderr={}\nstdout={}",
        verify.status.code(),
        error,
        String::from_utf8_lossy(&verify.stdout)
    );
    assert!(!error.contains("패치 작업 완료"));
    assert_eq!(
        fs::read_to_string(fixture.project.join("src/lib.rs")).unwrap(),
        "pub const VALUE: i32 = 1;\n"
    );
}

#[test]
fn corrupt_workflow_blocks_resume_without_backend_reentry() {
    let fixture = fixture("corrupt-workflow");
    fixture.start();
    let run = fixture.command(&["run", "src/lib.rs 테스트 값을 고쳐줘"]);
    assert!(run.status.success());
    let out = String::from_utf8(run.stdout).unwrap();
    let workflow = field(&out, "workflow id");
    fs::write(
        fixture
            .project
            .join(".rpotato/workflows")
            .join(format!("{workflow}.json")),
        "{corrupt",
    )
    .unwrap();
    let resume = fixture.command(&["state", "resume"]);
    assert_eq!(resume.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&resume.stderr).contains("fail-closed"));
    assert_eq!(
        fs::read_to_string(&fixture.calls).unwrap().lines().count(),
        1
    );
}
