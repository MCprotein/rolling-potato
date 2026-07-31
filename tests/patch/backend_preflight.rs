use super::*;

#[test]
fn backend_cancel_interrupts_stalled_input_token_preflight() {
    let fixture = fixture("backend-preflight-cancel");
    fixture.start();
    let mut command = fixture.command_builder(&[
        "backend",
        "chat",
        "--prompt",
        "RPOTATO_PREFLIGHT_STALL",
        "--timeout-ms",
        "5000",
    ]);
    let chat = spawn_captured(&mut command).unwrap();
    let active_record = fixture.data.join("state/backend-active-generation.txt");
    wait_for_path(&active_record, Duration::from_secs(2));

    let cancel = fixture.command(&["backend", "cancel"]);
    assert!(
        cancel.status.success(),
        "{}",
        String::from_utf8_lossy(&cancel.stderr)
    );
    assert!(
        String::from_utf8_lossy(&cancel.stdout).contains("status: acknowledged"),
        "{}",
        String::from_utf8_lossy(&cancel.stdout)
    );
    let chat = wait_bounded(chat, &["backend", "chat", "preflight-stall"]);
    assert!(!chat.status.success());
    assert!(String::from_utf8_lossy(&chat.stderr).contains("취소됨"));
    assert!(!active_record.exists());
    assert!(!fixture
        .data
        .join("state/backend-active-generation.lock")
        .exists());
    assert!(!fixture
        .data
        .join("state/backend-active-generation.cancel")
        .exists());
    assert!(!fixture.calls.exists());
    assert_eq!(
        fs::read_to_string(&fixture.preflight_calls)
            .unwrap()
            .lines()
            .count(),
        1
    );
}

#[test]
fn stalled_input_token_preflight_uses_total_chat_timeout_and_cleans_state() {
    let fixture = fixture("backend-preflight-timeout");
    fixture.start();

    let chat = fixture.command(&[
        "backend",
        "chat",
        "--prompt",
        "RPOTATO_PREFLIGHT_STALL",
        "--timeout-ms",
        "1000",
    ]);

    assert!(!chat.status.success());
    assert!(
        String::from_utf8_lossy(&chat.stderr).contains("시간 초과"),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&chat.stdout),
        String::from_utf8_lossy(&chat.stderr)
    );
    assert!(!fixture
        .data
        .join("state/backend-active-generation.txt")
        .exists());
    assert!(!fixture
        .data
        .join("state/backend-active-generation.lock")
        .exists());
    assert!(!fixture
        .data
        .join("state/backend-active-generation.cancel")
        .exists());
    assert!(!fixture.calls.exists());
    assert_eq!(
        fs::read_to_string(&fixture.preflight_calls)
            .unwrap()
            .lines()
            .count(),
        1
    );
    let ledger = fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl")).unwrap();
    assert!(ledger.contains("backend.generation.timeout"));
    assert!(ledger.contains("phase=input-tokens"));
}

#[test]
fn fake_llama_routes_input_token_preflight_without_counting_it_as_chat() {
    let fixture = fixture("backend-input-token-route");
    fixture.start();

    let chat = fixture.command(&["backend", "chat", "--prompt", "route preflight"]);

    assert!(
        chat.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&chat.stdout),
        String::from_utf8_lossy(&chat.stderr)
    );
    assert_eq!(
        fs::read_to_string(&fixture.calls)
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        ["chat"]
    );
    assert_eq!(
        fs::read_to_string(&fixture.preflight_calls)
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        ["input_tokens"]
    );
}

#[test]
fn concurrent_chat_is_rejected_before_a_second_input_token_preflight() {
    let fixture = fixture("backend-preflight-single-flight");
    fixture.start();
    let mut first_command = fixture.command_builder(&[
        "backend",
        "chat",
        "--prompt",
        "RPOTATO_STALL",
        "--timeout-ms",
        "5000",
    ]);
    let first = spawn_captured(&mut first_command).unwrap();
    wait_for_lines(&fixture.preflight_calls, 1, Duration::from_secs(2));
    wait_for_lines(&fixture.calls, 1, Duration::from_secs(2));

    let second = fixture.command(&["backend", "chat", "--prompt", "second contender"]);

    assert!(!second.status.success());
    assert!(
        String::from_utf8_lossy(&second.stderr).contains("이미 active generation"),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&second.stdout),
        String::from_utf8_lossy(&second.stderr)
    );
    assert_eq!(
        fs::read_to_string(&fixture.preflight_calls)
            .unwrap()
            .lines()
            .count(),
        1
    );

    let cancel = fixture.command(&["backend", "cancel"]);
    assert!(cancel.status.success());
    let first = wait_bounded(first, &["backend", "chat", "single-flight"]);
    assert!(!first.status.success());
}
