use super::*;

#[test]
fn parses_model_install() {
    let command = parse([
        "model".to_string(),
        "install".to_string(),
        "gemma-4-e4b".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Model(ModelCommand::Install {
            id: "gemma-4-e4b".to_string()
        })
    );
}

#[test]
fn parses_model_manifest() {
    let command = parse(["model".to_string(), "manifest".to_string()]).unwrap();
    assert_eq!(command, Command::Model(ModelCommand::Manifest));
}

#[test]
fn parses_model_inspect() {
    let command = parse([
        "model".to_string(),
        "inspect".to_string(),
        "qwen3.5-4b".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Model(ModelCommand::Inspect {
            id: "qwen3.5-4b".to_string()
        })
    );
}

#[test]
fn parses_model_registry() {
    let command = parse(["model".to_string(), "registry".to_string()]).unwrap();
    assert_eq!(command, Command::Model(ModelCommand::Registry));
}

#[test]
fn parses_model_default_show_and_select() {
    assert_eq!(
        parse(["model".to_string(), "default".to_string()]).unwrap(),
        Command::Model(ModelCommand::Default)
    );
    assert_eq!(
        parse([
            "model".to_string(),
            "default".to_string(),
            "qwen3.5-4b".to_string(),
        ])
        .unwrap(),
        Command::Model(ModelCommand::SetDefault {
            id: "qwen3.5-4b".to_string()
        })
    );
}

#[test]
fn parses_model_download_plan() {
    let command = parse([
        "model".to_string(),
        "download-plan".to_string(),
        "qwen3.5-4b".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Model(ModelCommand::DownloadPlan {
            id: "qwen3.5-4b".to_string()
        })
    );
}

#[test]
fn parses_model_eval_plan() {
    let command = parse([
        "model".to_string(),
        "eval-plan".to_string(),
        "qwen3.5-4b".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Model(ModelCommand::EvalPlan {
            id: "qwen3.5-4b".to_string()
        })
    );
}

#[test]
fn parses_model_benchmark_plan() {
    let command = parse([
        "model".to_string(),
        "benchmark-plan".to_string(),
        "qwen3.5-4b".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Model(ModelCommand::BenchmarkPlan {
            id: "qwen3.5-4b".to_string()
        })
    );
}

#[test]
fn parses_model_fetch_candidate_for_evaluation() {
    let command = parse([
        "model".to_string(),
        "fetch-candidate".to_string(),
        "qwen3.5-4b".to_string(),
        "--for-evaluation".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Model(ModelCommand::FetchCandidate {
            id: "qwen3.5-4b".to_string()
        })
    );
}

#[test]
fn model_fetch_candidate_requires_evaluation_flag() {
    let err = parse([
        "model".to_string(),
        "fetch-candidate".to_string(),
        "qwen3.5-4b".to_string(),
    ])
    .unwrap_err();

    assert_eq!(err.code, 2);
    assert!(err.message.contains("--for-evaluation"));
}

#[test]
fn parses_model_verify_file() {
    let command = parse([
        "model".to_string(),
        "verify-file".to_string(),
        "model.gguf".to_string(),
        "--sha256".to_string(),
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Model(ModelCommand::VerifyFile {
            path: "model.gguf".to_string(),
            sha256: "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".to_string()
        })
    );
}

#[test]
fn parses_model_promote_with_evidence_file() {
    let command = parse([
        "model".to_string(),
        "promote".to_string(),
        "qwen3.5-4b".to_string(),
        "--evidence".to_string(),
        "evidence/qwen3.5-4b-local.json".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Model(ModelCommand::Promote {
            id: "qwen3.5-4b".to_string(),
            evidence: "evidence/qwen3.5-4b-local.json".to_string()
        })
    );
}

#[test]
fn model_promote_requires_evidence_file() {
    let err = parse([
        "model".to_string(),
        "promote".to_string(),
        "qwen3.5-4b".to_string(),
    ])
    .unwrap_err();

    assert_eq!(err.code, 2);
    assert!(err.message.contains("--evidence"));
}

#[test]
fn parses_model_cleanup_failed_dry_run() {
    let command = parse([
        "model".to_string(),
        "cleanup-failed".to_string(),
        "qwen3.5-4b".to_string(),
        "--dry-run".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Model(ModelCommand::CleanupFailed {
            id: "qwen3.5-4b".to_string(),
            dry_run: true
        })
    );
}
