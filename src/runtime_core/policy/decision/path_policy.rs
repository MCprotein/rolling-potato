//! Project path-boundary policy classification.

use std::path::{Component, Path};

use crate::foundation::error::AppError;

use super::types::{ActionKind, Decision, PathMode, PathPolicyPort, PolicyDecision};

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
