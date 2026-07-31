use super::*;

#[test]
fn backend_generation_cancel_keeps_sidecar_and_cleans_active_state() {
    let fixture = fixture("backend-generation-cancel");
    fixture.start();
    let mut command = fixture.command_builder(&[
        "backend",
        "chat",
        "--prompt",
        "RPOTATO_STALL",
        "--stream",
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
    let chat = wait_bounded(chat, &["backend", "chat", "--stream"]);
    assert!(!chat.status.success());
    assert!(
        String::from_utf8_lossy(&chat.stderr).contains("취소됨"),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&chat.stdout),
        String::from_utf8_lossy(&chat.stderr)
    );

    let status = fixture.command(&["backend", "status"]);
    assert!(status.status.success());
    assert!(String::from_utf8_lossy(&status.stdout).contains("status: running"));
    assert!(!active_record.exists());
    assert!(!fixture
        .data
        .join("state/backend-active-generation.lock")
        .exists());
    assert!(!fixture
        .data
        .join("state/backend-active-generation.cancel")
        .exists());
    let ledger = fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl")).unwrap();
    assert!(ledger.contains("backend.generation.cancelled"));
    assert!(ledger.contains("backend.resource.sampled"));
}

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

#[test]
fn backend_generation_timeout_records_terminal_evidence_and_cleans_state() {
    let fixture = fixture("backend-generation-timeout");
    fixture.start();

    let chat = fixture.command(&[
        "backend",
        "chat",
        "--prompt",
        "RPOTATO_STALL",
        "--timeout-ms",
        "150",
    ]);
    assert!(!chat.status.success());
    assert!(
        String::from_utf8_lossy(&chat.stderr).contains("시간 초과"),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&chat.stdout),
        String::from_utf8_lossy(&chat.stderr)
    );

    let status = fixture.command(&["backend", "status"]);
    assert!(status.status.success());
    assert!(String::from_utf8_lossy(&status.stdout).contains("status: running"));
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
    let ledger = fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl")).unwrap();
    assert!(ledger.contains("backend.generation.timeout"));
    assert!(ledger.contains("backend.resource.sampled"));
}

#[test]
fn streaming_guard_keeps_a_nonempty_fallback_visible_without_failed_ledger() {
    let fixture = fixture("backend-stream-language-guard");
    let forbidden = "This model response must never be emitted.";
    fs::write(&fixture.response, forbidden).unwrap();
    fixture.start();

    let chat = fixture.command(&[
        "backend",
        "chat",
        "--prompt",
        "언어 경계를 검증해줘",
        "--stream",
    ]);

    assert!(chat.status.success());
    assert!(String::from_utf8_lossy(&chat.stdout).contains(forbidden));
    assert!(!String::from_utf8_lossy(&chat.stderr).contains(forbidden));
    let ledger = fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl")).unwrap();
    assert!(!ledger.contains("backend.generation.failed"));
    assert!(ledger.contains("backend.chat.completed"));
    assert!(!ledger.contains(forbidden));
}

#[test]
fn upstream_stream_error_detail_is_redacted_from_output_and_persistent_state() {
    let fixture = fixture("backend-stream-error-redaction");
    fixture.start();
    let secret = b"RPOTATO_SECRET_UPSTREAM_DETAIL";

    let chat = fixture.command(&[
        "backend",
        "chat",
        "--prompt",
        "RPOTATO_UPSTREAM_ERROR",
        "--stream",
    ]);

    assert_eq!(chat.status.code(), Some(3));
    assert!(!chat
        .stdout
        .windows(secret.len())
        .any(|window| window == secret));
    assert!(!chat
        .stderr
        .windows(secret.len())
        .any(|window| window == secret));
    assert!(!tree_contains(&fixture.data, secret));
    let ledger = fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl")).unwrap();
    assert!(ledger.contains("error_detail=redacted"));
}

#[test]
fn upstream_http_reason_phrase_is_redacted_from_output_and_persistent_state() {
    let fixture = fixture("backend-http-error-redaction");
    fixture.start();
    let secret = b"RPOTATO_SECRET_REASON_PHRASE";

    let chat = fixture.command(&[
        "backend",
        "chat",
        "--prompt",
        "RPOTATO_HTTP_ERROR",
        "--stream",
    ]);

    assert_eq!(chat.status.code(), Some(3));
    assert!(!chat
        .stdout
        .windows(secret.len())
        .any(|window| window == secret));
    assert!(!chat
        .stderr
        .windows(secret.len())
        .any(|window| window == secret));
    assert!(String::from_utf8_lossy(&chat.stderr).contains("backend request 실패"));
    assert!(!tree_contains(&fixture.data, secret));
}

#[test]
fn streaming_guard_projects_mixed_output_without_hiding_the_answer() {
    let fixture = fixture("backend-stream-mixed-language-guard");
    fixture.start();
    let forbidden = "Forbidden English sentence.";

    let chat = fixture.command(&[
        "backend",
        "chat",
        "--prompt",
        "RPOTATO_MIXED_LANGUAGE",
        "--stream",
    ]);

    assert!(chat.status.success());
    let stdout = String::from_utf8_lossy(&chat.stdout);
    assert!(stdout.contains("정상 한국어 문장입니다."));
    assert!(!stdout.contains(forbidden));
    assert!(!String::from_utf8_lossy(&chat.stderr).contains(forbidden));
    let ledger = fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl")).unwrap();
    assert!(!ledger.contains("backend.generation.failed"));
    assert!(ledger.contains("backend.chat.completed"));
    assert!(!ledger.contains(forbidden));
}

#[test]
fn backend_stop_acknowledges_generation_cancellation_before_sidecar_shutdown() {
    let fixture = fixture("backend-stop-active-generation");
    fixture.start();
    let mut command = fixture.command_builder(&[
        "backend",
        "chat",
        "--prompt",
        "RPOTATO_STALL",
        "--stream",
        "--timeout-ms",
        "15000",
    ]);
    let chat = spawn_captured(&mut command).unwrap();
    wait_for_path(
        &fixture.data.join("state/backend-active-generation.txt"),
        Duration::from_secs(5),
    );
    wait_for_lines(&fixture.calls, 1, Duration::from_secs(5));

    let stop = fixture.command(&["backend", "stop"]);
    let chat = wait_bounded(chat, &["backend", "chat", "--stream"]);

    assert!(stop.status.success());
    assert!(
        String::from_utf8_lossy(&stop.stdout).contains("generation outcome: cancelled"),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&stop.stdout),
        String::from_utf8_lossy(&stop.stderr)
    );
    assert!(!chat.status.success());
    assert!(String::from_utf8_lossy(&chat.stderr).contains("취소됨"));
    for name in [
        "backend-active-generation.txt",
        "backend-active-generation.lock",
        "backend-active-generation.cancel",
    ] {
        assert!(!fixture.data.join("state").join(name).exists(), "{name}");
    }
    let status = fixture.command(&["backend", "status"]);
    assert!(String::from_utf8_lossy(&status.stdout).contains("status: stopped"));
}
