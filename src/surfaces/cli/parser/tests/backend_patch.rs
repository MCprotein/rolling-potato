use super::*;

#[test]
fn parses_backend_doctor() {
    let command = parse(["backend".to_string(), "doctor".to_string()]).unwrap();
    assert_eq!(command, Command::Backend(BackendCommand::Doctor));
}

#[test]
fn parses_backend_install_plan() {
    let command = parse(["backend".to_string(), "install-plan".to_string()]).unwrap();
    assert_eq!(command, Command::Backend(BackendCommand::InstallPlan));
}

#[test]
fn parses_backend_install() {
    let command = parse(["backend".to_string(), "install".to_string()]).unwrap();
    assert_eq!(command, Command::Backend(BackendCommand::Install));
}

#[test]
fn parses_backend_start() {
    let command = parse([
        "backend".to_string(),
        "start".to_string(),
        "--model".to_string(),
        "model.gguf".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Backend(BackendCommand::Start {
            model_path: Some("model.gguf".to_string()),
            ctx_size: None
        })
    );
}

#[test]
fn parses_backend_start_without_model_for_default_resolution() {
    let command = parse(["backend".to_string(), "start".to_string()]).unwrap();
    assert_eq!(
        command,
        Command::Backend(BackendCommand::Start {
            model_path: None,
            ctx_size: None
        })
    );
}

#[test]
fn parses_backend_start_with_ctx_size() {
    let command = parse([
        "backend".to_string(),
        "start".to_string(),
        "--model".to_string(),
        "model.gguf".to_string(),
        "--ctx-size".to_string(),
        "4096".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Backend(BackendCommand::Start {
            model_path: Some("model.gguf".to_string()),
            ctx_size: Some(4096)
        })
    );
}

#[test]
fn rejects_zero_backend_ctx_size() {
    let err = parse([
        "backend".to_string(),
        "start".to_string(),
        "--model".to_string(),
        "model.gguf".to_string(),
        "--ctx-size".to_string(),
        "0".to_string(),
    ])
    .unwrap_err();

    assert_eq!(err.code, 2);
    assert!(err.message.contains("1 이상"));
}

#[test]
fn parses_backend_status() {
    let command = parse(["backend".to_string(), "status".to_string()]).unwrap();
    assert_eq!(command, Command::Backend(BackendCommand::Status));
}

#[test]
fn parses_backend_stop() {
    let command = parse(["backend".to_string(), "stop".to_string()]).unwrap();
    assert_eq!(command, Command::Backend(BackendCommand::Stop));
}

#[test]
fn parses_backend_verify_archive() {
    let command = parse([
        "backend".to_string(),
        "verify-archive".to_string(),
        "llama.zip".to_string(),
        "--sha256".to_string(),
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Backend(BackendCommand::VerifyArchive {
            path: "llama.zip".to_string(),
            sha256: "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".to_string()
        })
    );
}

#[test]
fn parses_backend_health_check() {
    let command = parse(["backend".to_string(), "health-check".to_string()]).unwrap();
    assert_eq!(command, Command::Backend(BackendCommand::HealthCheck));
}

#[test]
fn parses_backend_chat() {
    let command = parse([
        "backend".to_string(),
        "chat".to_string(),
        "--prompt".to_string(),
        "감자는 무엇인가?".to_string(),
        "--max-tokens".to_string(),
        "64".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Backend(BackendCommand::Chat {
            prompt: "감자는 무엇인가?".to_string(),
            max_tokens: Some(64),
            stream: false,
            timeout_ms: None,
        })
    );
}

#[test]
fn parses_backend_stream_chat_timeout() {
    let command = parse([
        "backend".to_string(),
        "chat".to_string(),
        "--prompt".to_string(),
        "감자".to_string(),
        "--stream".to_string(),
        "--timeout-ms".to_string(),
        "1500".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Backend(BackendCommand::Chat {
            prompt: "감자".to_string(),
            max_tokens: None,
            stream: true,
            timeout_ms: Some(1500),
        })
    );
}

#[test]
fn parses_backend_generation_cancel() {
    let command = parse(["backend".to_string(), "cancel".to_string()]).unwrap();

    assert_eq!(command, Command::Backend(BackendCommand::Cancel));
}

#[test]
fn unknown_backend_command_guidance_includes_cancel() {
    let error = parse(["backend".to_string(), "unknown".to_string()]).unwrap_err();

    assert!(error.message.contains("stop, cancel, verify-archive"));
}

#[test]
fn backend_chat_requires_prompt() {
    let err = parse(["backend".to_string(), "chat".to_string()]).unwrap_err();

    assert_eq!(err.code, 2);
    assert!(err.message.contains("--prompt"));
}

#[test]
fn parses_patch_preview() {
    let command = parse([
        "patch".to_string(),
        "preview".to_string(),
        "--path".to_string(),
        "src/lib.rs".to_string(),
        "--find".to_string(),
        "old".to_string(),
        "--replace".to_string(),
        "new".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Patch(PatchCommand::Preview {
            path: "src/lib.rs".to_string(),
            find: "old".to_string(),
            replace: "new".to_string()
        })
    );
}

#[test]
fn parses_patch_approve_dry_run() {
    let command = parse([
        "patch".to_string(),
        "approve".to_string(),
        "patch-proposal-abc123".to_string(),
        "--token".to_string(),
        "token123".to_string(),
        "--dry-run".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Patch(PatchCommand::Approve {
            proposal_id: "patch-proposal-abc123".to_string(),
            token: "token123".to_string(),
            dry_run: true
        })
    );
}

#[test]
fn parses_patch_token_rotate() {
    let command = parse([
        "patch".to_string(),
        "token-rotate".to_string(),
        "patch-proposal-wf-example".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Patch(PatchCommand::TokenRotate {
            proposal_id: "patch-proposal-wf-example".to_string()
        })
    );
}

#[test]
fn rejects_patch_approve_with_verify_command() {
    let error = parse([
        "patch".to_string(),
        "approve".to_string(),
        "patch-proposal-abc123".to_string(),
        "--token".to_string(),
        "token123".to_string(),
        "--verify-command".to_string(),
        "cargo fmt --check".to_string(),
    ])
    .unwrap_err();

    assert!(error.message.contains("알 수 없는 patch approve 옵션"));
}

#[test]
fn parses_patch_verify() {
    let command = parse([
        "patch".to_string(),
        "verify".to_string(),
        "patch-proposal-abc123".to_string(),
        "--token".to_string(),
        "token123".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Patch(PatchCommand::Verify {
            proposal_id: "patch-proposal-abc123".to_string(),
            token: "token123".to_string()
        })
    );
}
