#[test]
fn fixture_retries_backend_start_after_ephemeral_port_collision() {
    let fixture = fixture("backend-port-retry");
    let occupied = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let occupied_port = occupied.local_addr().unwrap().port();
    fixture.port.store(occupied_port, Ordering::Relaxed);

    fixture.start();

    assert_ne!(fixture.port.load(Ordering::Relaxed), occupied_port);
}

#[test]
fn happy_path_is_restart_safe_and_reports_korean() {
    let fixture = fixture("happy-subprocess");
    fixture.start();
    let run = fixture.command(&["run", "src/lib.rs 테스트 값을 고쳐줘"]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let run_out = String::from_utf8(run.stdout).unwrap();
    assert!(!run_out.contains("MODEL ACTION"));
    assert!(!run_out.contains("- response:"));
    assert!(run_out.contains("raw response는 표시하지 않음"));
    let proposal = field(&run_out, "proposal id");
    let token = run_out
        .lines()
        .find_map(|line| line.strip_prefix("- approval command: rpotato patch approve "))
        .unwrap()
        .split(" --token ")
        .nth(1)
        .unwrap()
        .to_string();
    assert_eq!(
        fs::read_to_string(fixture.project.join("src/lib.rs")).unwrap(),
        "pub const VALUE: i32 = 1;\n"
    );

    let resume = fixture.command(&["state", "resume"]);
    assert!(resume.status.success());
    let resume_out = String::from_utf8_lossy(&resume.stdout);
    assert!(resume_out.contains("backend 호출: 없음"));
    assert!(resume_out.contains("token 재표시: 불가"));
    assert!(!resume_out.contains(&token));
    let tui = fixture.command(&["tui", "diff", &proposal]);
    assert!(tui.status.success());
    assert!(!String::from_utf8_lossy(&tui.stdout).contains(&token));
    assert_eq!(
        fs::read_to_string(&fixture.calls).unwrap().lines().count(),
        1
    );

    let approve = fixture.command(&["patch", "approve", &proposal, "--token", &token]);
    assert!(
        approve.status.success(),
        "{}",
        String::from_utf8_lossy(&approve.stderr)
    );
    let approve_report = String::from_utf8(approve.stdout).unwrap();
    assert!(approve_report.starts_with("patch approve\n- status: applied-awaiting-verification"));
    assert!(approve_report.contains("verification approval: required"));
    assert!(approve_report.contains("verification command는 아직 실행하지 않았습니다"));
    assert!(!approve_report.contains("패치 작업 완료"));
    assert!(!approve_report.contains("MODEL ACTION"));
    assert_eq!(
        fs::read_to_string(fixture.project.join("src/lib.rs")).unwrap(),
        "pub const VALUE: i32 = 2;\n"
    );
    let verify_token = verification_token(&approve_report);

    let resumed = fixture.command(&["state", "resume"]);
    assert!(resumed.status.success());
    let resumed_out = String::from_utf8_lossy(&resumed.stdout);
    assert!(resumed_out.contains("verification 승인 대기"));
    assert!(resumed_out.contains("verification 실행: 없음"));
    assert!(!resumed_out.contains(&verify_token));

    let verify = fixture.command(&["patch", "verify", &proposal, "--token", &verify_token]);
    assert!(
        verify.status.success(),
        "{}",
        String::from_utf8_lossy(&verify.stderr)
    );
    let report = String::from_utf8(verify.stdout).unwrap();
    assert!(report.starts_with("패치 작업 완료\n- 결과: 성공"));
    assert!(report.contains("stop gate: 통과"));
    assert!(!report.contains("MODEL ACTION"));

    let ledger_path = fixture.data.join("state/runtime-ledger.jsonl");
    let event_count = fs::read_to_string(&ledger_path).unwrap().lines().count();
    let repeated = fixture.command(&["patch", "verify", &proposal, "--token", &verify_token]);
    assert!(
        repeated.status.success(),
        "status={:?}\nstderr={}\nstdout={}",
        repeated.status.code(),
        String::from_utf8_lossy(&repeated.stderr),
        String::from_utf8_lossy(&repeated.stdout)
    );
    assert_eq!(
        fs::read_to_string(&ledger_path).unwrap().lines().count(),
        event_count,
        "complete resume must not duplicate ledger events"
    );
    assert_eq!(
        fs::read_to_string(&fixture.calls).unwrap().lines().count(),
        1
    );
}
