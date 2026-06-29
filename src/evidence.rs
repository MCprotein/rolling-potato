use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::app::AppError;
use crate::paths;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceValidation {
    pub artifact: PathBuf,
    pub project_root: PathBuf,
    pub stale_policy: &'static str,
}

pub fn validate_report(pointer: &str) -> Result<String, AppError> {
    let validation = validate_artifact_pointer(pointer)?;
    Ok(format!(
        "evidence validate 결과\n- artifact: {}\n- project root: {}\n- boundary: project root 내부\n- stale policy: {}\n- 동작: artifact pointer가 존재하고 project boundary를 벗어나지 않는지 확인했습니다.",
        validation.artifact.display(),
        validation.project_root.display(),
        validation.stale_policy
    ))
}

pub fn validate_artifact_pointer(pointer: &str) -> Result<EvidenceValidation, AppError> {
    if pointer.trim().is_empty() {
        return Err(AppError::usage("evidence artifact pointer가 필요합니다."));
    }

    if pointer.contains("://") {
        return Err(AppError::blocked(
            "evidence artifact pointer는 local project path만 허용합니다.",
        ));
    }

    let pointer_path = Path::new(pointer);
    if pointer_path.is_absolute() {
        return Err(AppError::blocked(
            "evidence artifact pointer는 project-relative path만 허용합니다.",
        ));
    }

    if pointer_path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(AppError::blocked(
            "evidence artifact pointer는 상위 경로(..)를 포함할 수 없습니다.",
        ));
    }

    let project_root = canonical_project_root()?;
    let artifact = project_root.join(pointer_path);
    if !artifact.exists() {
        return Err(AppError::usage(format!(
            "evidence artifact가 존재하지 않습니다: {}",
            artifact.display()
        )));
    }

    let canonical_artifact = fs::canonicalize(&artifact).map_err(|err| {
        AppError::runtime(format!(
            "evidence artifact를 canonicalize하지 못했습니다: {} ({err})",
            artifact.display()
        ))
    })?;

    if !canonical_artifact.starts_with(&project_root) {
        return Err(AppError::blocked(format!(
            "evidence artifact가 project boundary를 벗어났습니다: {}",
            canonical_artifact.display()
        )));
    }

    Ok(EvidenceValidation {
        artifact: canonical_artifact,
        project_root,
        stale_policy: stale_policy_summary(),
    })
}

pub fn stale_policy_summary() -> &'static str {
    "artifact 누락, project boundary 이탈, stale_after_ms 만료 시 stale"
}

fn canonical_project_root() -> Result<PathBuf, AppError> {
    let root = paths::project_root();
    fs::create_dir_all(&root).map_err(|err| {
        AppError::runtime(format!(
            "project root를 만들지 못했습니다: {} ({err})",
            root.display()
        ))
    })?;
    fs::canonicalize(&root).map_err(|err| {
        AppError::runtime(format!(
            "project root를 canonicalize하지 못했습니다: {} ({err})",
            root.display()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_remote_artifact_pointer() {
        let err = validate_artifact_pointer("https://example.com/evidence.json")
            .expect_err("remote evidence pointers must be blocked");
        assert_eq!(err.code, 3);
    }

    #[test]
    fn rejects_parent_dir_artifact_pointer() {
        let err = validate_artifact_pointer("../outside.log")
            .expect_err("parent directory evidence pointers must be blocked");
        assert_eq!(err.code, 3);
    }
}
