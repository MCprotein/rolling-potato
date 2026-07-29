use std::fs;
use std::path::{Path, PathBuf};

use crate::adapters::filesystem::layout as paths;
use crate::foundation::error::AppError;
use crate::runtime_core::knowledge::evidence::{
    stale_policy_summary, validate_artifact_pointer_syntax, EvidenceValidation,
};

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
    validate_artifact_pointer_syntax(pointer)?;
    let pointer_path = Path::new(pointer);
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
