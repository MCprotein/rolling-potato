use std::path::{Component, Path, PathBuf};

use crate::foundation::error::AppError;
use crate::{ledger, paths, state};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Ask,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleSource {
    User,
    Project,
    Local,
    Session,
    Policy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    ReadFile,
    WriteFile,
    RunCommand,
    ApplyPatch,
    NetworkDownload,
    PluginCapability,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionStatus {
    Create,
    Update,
    Noop,
    UserModified,
    Blocked,
}

const ALL_RULE_SOURCES: &[RuleSource] = &[
    RuleSource::User,
    RuleSource::Project,
    RuleSource::Local,
    RuleSource::Session,
    RuleSource::Policy,
];

const ALL_ACTION_KINDS: &[ActionKind] = &[
    ActionKind::ReadFile,
    ActionKind::WriteFile,
    ActionKind::RunCommand,
    ActionKind::ApplyPatch,
    ActionKind::NetworkDownload,
    ActionKind::PluginCapability,
];

const ALL_ACTION_STATUSES: &[ActionStatus] = &[
    ActionStatus::Create,
    ActionStatus::Update,
    ActionStatus::Noop,
    ActionStatus::UserModified,
    ActionStatus::Blocked,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyDecision {
    pub decision: Decision,
    pub action_kind: ActionKind,
    pub rule_source: RuleSource,
    pub command_class: &'static str,
    pub reason: String,
    pub approval_prompt: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCommand {
    pub display: String,
    pub argv: Vec<String>,
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

pub fn schema_report() -> String {
    format!(
        "policy schema\n- action kinds: {}\n- decisions: allow, ask, deny\n- rule sources: {}\n- action status: {}\n- write policy: diff-before-write + approval required\n- user-modified policy: owned region 외 변경은 blocked 또는 ask\n- managed artifact policy: manifest/hash tracking required before install/download\n- network policy: download and remote connector require ask\n- destructive command policy: deny or high-confirm by default",
        ALL_ACTION_KINDS
            .iter()
            .map(|kind| format!("{kind:?}"))
            .collect::<Vec<_>>()
            .join(", "),
        ALL_RULE_SOURCES
            .iter()
            .map(|source| format!("{source:?}"))
            .collect::<Vec<_>>()
            .join(", "),
        ALL_ACTION_STATUSES
            .iter()
            .map(|status| format!("{status:?}"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

pub fn classify_command(command: &str) -> Result<PolicyDecision, AppError> {
    let parsed = parse_exact_argv(command)?;
    let first = parsed.argv[0].as_str();
    if matches!(
        first,
        "rm" | "sh" | "bash" | "zsh" | "python" | "python3" | "mkfs" | "dd"
    ) || matches!(parsed.argv.as_slice(), [a, b, ..] if a == "git" && ((b == "reset" && parsed.argv.iter().any(|v| v == "--hard")) || b == "checkout"))
    {
        return Ok(PolicyDecision::new(
            Decision::Deny,
            ActionKind::RunCommand,
            "destructive-or-interpreter",
            "shell/interpreter/destructive command는 차단합니다.",
            "차단",
        ));
    }
    if matches!(first, "curl" | "wget")
        || matches!(parsed.argv.as_slice(), [a, b, ..] if (a == "git" && b == "clone") || (a == "cargo" && b == "add"))
    {
        return Ok(PolicyDecision::new(
            Decision::Ask,
            ActionKind::NetworkDownload,
            "network-or-dependency",
            "network/download/dependency 변경은 승인 prompt가 필요합니다.",
            "사용자 승인 필요",
        ));
    }
    if is_general_read_only(&parsed.argv) || validate_patch_verification_argv(&parsed.argv).is_ok()
    {
        return Ok(PolicyDecision::new(
            Decision::Allow,
            ActionKind::RunCommand,
            "read-only-or-verification",
            "읽기/검증 명령으로 분류되어 승인 없이 실행 가능",
            "불필요",
        ));
    }

    Ok(PolicyDecision::new(
        Decision::Ask,
        ActionKind::RunCommand,
        "unknown-side-effect",
        "side effect 여부가 확실하지 않아 승인 prompt가 필요합니다.",
        "사용자 승인 필요",
    ))
}

pub fn parse_patch_verification(command: &str) -> Result<ParsedCommand, AppError> {
    let parsed = parse_exact_argv(command)?;
    validate_patch_verification_argv(&parsed.argv)?;
    Ok(parsed)
}

fn parse_exact_argv(command: &str) -> Result<ParsedCommand, AppError> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Err(AppError::usage("검사할 command가 필요합니다."));
    }
    if trimmed.chars().any(|ch| {
        matches!(
            ch,
            ';' | '|' | '&' | '<' | '>' | '`' | '$' | '\n' | '\r' | '"' | '\'' | '(' | ')'
        )
    }) {
        return Err(AppError::blocked(
            "command 검증 차단\n- 이유: shell metacharacter 또는 chaining은 허용하지 않습니다.",
        ));
    }
    let argv = trimmed
        .split_ascii_whitespace()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if argv
        .first()
        .is_some_and(|arg| arg.contains('/') || arg.contains('\\'))
    {
        return Err(AppError::blocked(
            "command 검증 차단\n- 이유: path-like executable/argument는 허용하지 않습니다.",
        ));
    }
    Ok(ParsedCommand {
        display: argv.join(" "),
        argv,
    })
}

fn validate_patch_verification_argv(argv: &[String]) -> Result<(), AppError> {
    if argv == ["pwd"] {
        return Ok(());
    }
    if argv.first().map(String::as_str) != Some("cargo") || argv.len() < 2 {
        return Err(AppError::blocked(
            "patch verification 차단\n- 이유: pwd 또는 제한된 cargo verification만 허용합니다.",
        ));
    }
    let subcommand = argv[1].as_str();
    if !matches!(subcommand, "test" | "check" | "fmt" | "clippy") {
        return Err(AppError::blocked(
            "patch verification 차단\n- 이유: cargo test/check/fmt/clippy만 허용합니다.",
        ));
    }
    if subcommand == "fmt" {
        if argv != ["cargo", "fmt", "--", "--check"] {
            return Err(AppError::blocked(
                "patch verification 차단\n- 이유: cargo fmt는 정확히 `cargo fmt -- --check`만 허용합니다.",
            ));
        }
        return Ok(());
    }
    let mut index = 2;
    while index < argv.len() {
        let arg = argv[index].as_str();
        if matches!(arg, "--manifest-path" | "--package" | "-p")
            || arg.starts_with("--manifest-path=")
            || arg.starts_with("--package=")
        {
            return Err(AppError::blocked(
                "patch verification 차단\n- 이유: 외부 manifest/package 지정은 허용하지 않습니다.",
            ));
        }
        let takes_value = matches!(arg, "--bin" | "--test" | "--example" | "--features");
        let allowed = matches!(
            arg,
            "--locked"
                | "--all-targets"
                | "--tests"
                | "--bins"
                | "--lib"
                | "--examples"
                | "--release"
                | "--check"
                | "--no-default-features"
                | "--bin"
                | "--test"
                | "--example"
                | "--features"
        );
        if !allowed {
            return Err(AppError::blocked(format!(
                "patch verification 차단\n- 이유: 허용되지 않은 cargo argument: {arg}"
            )));
        }
        if takes_value {
            index += 1;
            let Some(value) = argv.get(index) else {
                return Err(AppError::blocked(
                    "patch verification 차단\n- 이유: cargo argument 값이 누락되었습니다.",
                ));
            };
            if value.is_empty()
                || !value
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | ','))
            {
                return Err(AppError::blocked("patch verification 차단\n- 이유: cargo argument 값이 안전한 identifier가 아닙니다."));
            }
        }
        index += 1;
    }
    Ok(())
}

fn is_general_read_only(argv: &[String]) -> bool {
    matches!(argv, [one] if one == "pwd" || one == "ls" || one == "git")
        || matches!(argv, [one, two] if (one == "git" && matches!(two.as_str(), "status" | "diff")) || (one == "cargo" && matches!(two.as_str(), "test" | "check" | "clippy")))
        || matches!(
            argv.first().map(String::as_str),
            Some("rg" | "head" | "tail" | "wc")
        )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathMode {
    Read,
    Write,
}

pub fn classify_path(mode: PathMode, raw_path: &str) -> Result<PolicyDecision, AppError> {
    if raw_path.trim().is_empty() {
        return Err(AppError::usage("검사할 path가 필요합니다."));
    }

    let path = Path::new(raw_path);
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Ok(PolicyDecision::new(
            Decision::Deny,
            action_for_mode(mode),
            "path-traversal",
            "상위 경로(..)는 project boundary 우회 위험 때문에 차단합니다.",
            "차단",
        ));
    }

    let project_root = canonical_project_root()?;
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_root.join(path)
    };
    let normalized = normalize_existing_or_parent(&candidate)?;

    if !normalized.starts_with(&project_root) {
        return Ok(PolicyDecision::new(
            Decision::Deny,
            action_for_mode(mode),
            "outside-project",
            "project boundary 밖 path는 기본 차단합니다.",
            "차단",
        ));
    }

    if is_excluded_path(&normalized) {
        return Ok(PolicyDecision::new(
            Decision::Deny,
            action_for_mode(mode),
            "excluded-path",
            ".git, target, build 산출물, credential/model file은 기본 제외합니다.",
            "차단",
        ));
    }

    match mode {
        PathMode::Read => Ok(PolicyDecision::new(
            Decision::Allow,
            ActionKind::ReadFile,
            "project-read",
            "project 내부 읽기 허용 path입니다.",
            "불필요",
        )),
        PathMode::Write => Ok(PolicyDecision::new(
            Decision::Ask,
            ActionKind::WriteFile,
            "project-write",
            "쓰기 전 diff 표시와 사용자 승인이 필요합니다.",
            "사용자 승인 필요",
        )),
    }
}

impl PolicyDecision {
    fn new(
        decision: Decision,
        action_kind: ActionKind,
        command_class: &'static str,
        reason: impl Into<String>,
        approval_prompt: &'static str,
    ) -> Self {
        Self {
            decision,
            action_kind,
            rule_source: RuleSource::Policy,
            command_class,
            reason: reason.into(),
            approval_prompt,
        }
    }

    fn label(&self) -> &'static str {
        match self.decision {
            Decision::Allow => "allow",
            Decision::Ask => "ask",
            Decision::Deny => "deny",
        }
    }
}

fn action_for_mode(mode: PathMode) -> ActionKind {
    match mode {
        PathMode::Read => ActionKind::ReadFile,
        PathMode::Write => ActionKind::WriteFile,
    }
}

fn canonical_project_root() -> Result<PathBuf, AppError> {
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

fn normalize_existing_or_parent(path: &Path) -> Result<PathBuf, AppError> {
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

fn is_excluded_path(path: &Path) -> bool {
    let lower = path.display().to_string().to_ascii_lowercase();
    contains_any(
        &lower,
        &[
            "/.git/",
            "/node_modules/",
            "/target/",
            "/dist/",
            "/build/",
            ".env",
            "id_rsa",
            ".gguf",
            ".safetensors",
            ".bin",
        ],
    )
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

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
