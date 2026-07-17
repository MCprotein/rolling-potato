//! Project path and audit adapter for policy decisions.

use std::path::{Path, PathBuf};

use crate::app::workflow_adapter::ledger;
use crate::foundation::error::AppError;
#[cfg(test)]
use crate::runtime_core::policy::decision::parse_patch_verification;
use crate::runtime_core::policy::decision::{self, PathPolicyPort};
pub use crate::runtime_core::policy::decision::{
    classify_command, schema_report, Decision, PathMode, PolicyDecision,
};
use crate::{adapters::filesystem::layout as paths, state};

struct ProjectPathPolicy;

impl PathPolicyPort for ProjectPathPolicy {
    fn canonical_project_root(&self) -> Result<PathBuf, AppError> {
        let root = paths::project_root();
        std::fs::create_dir_all(&root).map_err(|err| {
            AppError::runtime(format!(
                "project root를 만들지 못했습니다: {} ({err})",
                root.display()
            ))
        })?;
        std::fs::canonicalize(&root).map_err(|err| {
            AppError::runtime(format!(
                "project root를 canonicalize하지 못했습니다: {} ({err})",
                root.display()
            ))
        })
    }

    fn normalize_existing_or_parent(&self, path: &Path) -> Result<PathBuf, AppError> {
        if path.exists() {
            return std::fs::canonicalize(path).map_err(|err| {
                AppError::runtime(format!(
                    "path를 canonicalize하지 못했습니다: {} ({err})",
                    path.display()
                ))
            });
        }

        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let canonical_parent = std::fs::canonicalize(parent).map_err(|err| {
            AppError::runtime(format!(
                "path parent를 canonicalize하지 못했습니다: {} ({err})",
                parent.display()
            ))
        })?;
        Ok(canonical_parent.join(path.file_name().unwrap_or_default()))
    }
}

pub fn check_command_report(command: &str) -> Result<String, AppError> {
    let decision = classify_command(command)?;
    let event_id = state::record_event(
        "policy.command_decision",
        "command permission decision",
        &format!(
            "decision={:?} class={} command={}",
            decision.decision,
            decision.command_class,
            ledger::redact_text(command)
        ),
    )?;

    Ok(format!(
        "policy command 결과\n- command: {}\n- decision: {}\n- class: {}\n- action kind: {:?}\n- rule source: {:?}\n- approval prompt: {}\n- reason: {}\n- ledger event: {}",
        ledger::redact_text(command),
        decision.label(),
        decision.command_class,
        decision.action_kind,
        decision.rule_source,
        decision.approval_prompt,
        decision.reason,
        event_id
    ))
}

pub fn check_path_report(mode: PathMode, raw_path: &str) -> Result<String, AppError> {
    let decision = classify_path(mode, raw_path)?;
    let event_id = state::record_event(
        "policy.path_decision",
        "path permission decision",
        &format!(
            "decision={:?} mode={:?} path={}",
            decision.decision, mode, raw_path
        ),
    )?;

    Ok(format!(
        "policy path 결과\n- path: {}\n- mode: {:?}\n- decision: {}\n- action kind: {:?}\n- rule source: {:?}\n- approval prompt: {}\n- reason: {}\n- ledger event: {}",
        raw_path,
        mode,
        decision.label(),
        decision.action_kind,
        decision.rule_source,
        decision.approval_prompt,
        decision.reason,
        event_id
    ))
}

pub fn redact_report(text: &str) -> String {
    format!(
        "policy redact 결과\n- redacted: {}\n- 동작: credential-like token은 persistence 전에 치환합니다.",
        ledger::redact_text(text)
    )
}

pub fn classify_path(mode: PathMode, raw_path: &str) -> Result<PolicyDecision, AppError> {
    decision::classify_path(&ProjectPathPolicy, mode, raw_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_core::policy::decision::ActionKind;

    #[test]
    fn denies_destructive_command() {
        let decision = classify_command("git reset --hard HEAD").unwrap();
        assert_eq!(decision.decision, Decision::Deny);
    }

    #[test]
    fn asks_for_network_download() {
        let decision = classify_command("curl https://example.com/file").unwrap();
        assert_eq!(decision.decision, Decision::Ask);
        assert_eq!(decision.action_kind, ActionKind::NetworkDownload);
    }

    #[test]
    fn allows_read_only_verification() {
        let decision = classify_command("cargo test").unwrap();
        assert_eq!(decision.decision, Decision::Allow);
    }

    #[test]
    fn schema_report_names_diff_before_write() {
        assert!(schema_report().contains("diff-before-write"));
    }

    #[test]
    fn path_policy_preserves_project_boundary_decisions() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root =
            std::env::temp_dir().join(format!("rpotato-policy-path-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &root);

        let read = classify_path(PathMode::Read, "README.md").unwrap();
        let write = classify_path(PathMode::Write, "README.md").unwrap();
        let traversal = classify_path(PathMode::Read, "../secret.txt").unwrap();
        let excluded = classify_path(PathMode::Write, "target/output.bin").unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        let _ = std::fs::remove_dir_all(&root);

        assert_eq!(read.decision, Decision::Allow);
        assert_eq!(write.decision, Decision::Ask);
        assert_eq!(traversal.decision, Decision::Deny);
        assert_eq!(excluded.decision, Decision::Deny);
    }

    #[test]
    fn patch_verification_rejects_smuggling_and_external_selection() {
        for command in [
            "sh pwd",
            "rm ignored cargo test",
            "python cargo test",
            "/usr/bin/cargo test",
            "cargo test; pwd",
            "cargo test && pwd",
            "cargo test | pwd",
            "cargo test --manifest-path ../other/Cargo.toml",
            "cargo test --manifest-path=other/Cargo.toml",
            "cargo test --package other",
            "cargo test -p other",
            "cargo test --workspace",
            "cargo test --all",
            "cargo fmt",
            "cargo fmt --all",
            "cargo fmt --check",
            "cargo fmt --all -- --check",
        ] {
            assert!(
                parse_patch_verification(command).is_err(),
                "smuggled verification must be rejected: {command}"
            );
        }
        assert_eq!(
            parse_patch_verification("cargo fmt -- --check")
                .unwrap()
                .argv,
            ["cargo", "fmt", "--", "--check"]
        );
    }

    #[test]
    fn classification_uses_exact_first_argv_not_substrings() {
        assert_eq!(classify_command("sh pwd").unwrap().decision, Decision::Deny);
        assert_eq!(
            classify_command("rm ignored cargo test").unwrap().decision,
            Decision::Deny
        );
        assert_eq!(
            classify_command("cargo test").unwrap().decision,
            Decision::Allow
        );
        assert!(classify_command("cargo test; rm ignored").is_err());
    }
}
