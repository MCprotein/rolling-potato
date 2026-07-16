//! Surface-neutral tool and capability policy decisions.

use std::path::{Component, Path, PathBuf};

use crate::foundation::error::AppError;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathMode {
    Read,
    Write,
}

pub(crate) trait PathPolicyPort {
    fn canonical_project_root(&self) -> Result<PathBuf, AppError>;

    fn normalize_existing_or_parent(&self, path: &Path) -> Result<PathBuf, AppError>;
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

pub(crate) fn classify_path(
    port: &dyn PathPolicyPort,
    mode: PathMode,
    raw_path: &str,
) -> Result<PolicyDecision, AppError> {
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

    let project_root = port.canonical_project_root()?;
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_root.join(path)
    };
    let normalized = port.normalize_existing_or_parent(&candidate)?;

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

    pub(crate) fn label(&self) -> &'static str {
        match self.decision {
            Decision::Allow => "allow",
            Decision::Ask => "ask",
            Decision::Deny => "deny",
        }
    }
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

fn action_for_mode(mode: PathMode) -> ActionKind {
    match mode {
        PathMode::Read => ActionKind::ReadFile,
        PathMode::Write => ActionKind::WriteFile,
    }
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
