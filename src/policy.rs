use std::path::{Component, Path, PathBuf};

use crate::app::AppError;
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
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Err(AppError::usage("검사할 command가 필요합니다."));
    }

    let lower = trimmed.to_ascii_lowercase();
    let first = lower.split_whitespace().next().unwrap_or_default();

    if contains_any(
        &lower,
        &[
            "rm -rf",
            "git reset --hard",
            "git checkout --",
            "mkfs",
            "dd if=",
            "chmod -r",
            "chown -r",
            "prod deploy",
            "production deploy",
        ],
    ) {
        return Ok(PolicyDecision::new(
            Decision::Deny,
            ActionKind::RunCommand,
            "destructive-command",
            "destructive command는 기본 차단합니다.",
            "차단",
        ));
    }

    if contains_any(
        &lower,
        &[
            "curl ",
            "wget ",
            "git clone",
            "cargo add",
            "npm install",
            "npm i ",
            "pip install",
            "docker run",
            "docker pull",
        ],
    ) || matches!(first, "curl" | "wget")
    {
        return Ok(PolicyDecision::new(
            Decision::Ask,
            ActionKind::NetworkDownload,
            "network-or-dependency",
            "network/download/dependency 변경은 승인 prompt가 필요합니다.",
            "사용자 승인 필요",
        ));
    }

    if contains_any(
        &lower,
        &[
            "cargo test",
            "cargo clippy",
            "cargo fmt --check",
            "git status",
            "git diff",
            "rg ",
            "ls ",
            "pwd",
            "head ",
            "tail ",
            "wc ",
        ],
    ) || matches!(first, "rg" | "ls" | "pwd")
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
}
