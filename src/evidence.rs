use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::app::AppError;
use crate::paths;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceStoreStatus {
    pub runtime_evidence_file: PathBuf,
    pub runtime_evidence_records: usize,
    pub project_evidence_dir: PathBuf,
    pub project_artifacts: usize,
    pub stale_policy: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceValidation {
    pub artifact: PathBuf,
    pub project_root: PathBuf,
    pub stale_policy: &'static str,
}

pub fn store_status() -> Result<EvidenceStoreStatus, AppError> {
    let runtime_evidence_file = paths::runtime_evidence_file();
    let project_evidence_dir = paths::project_evidence_dir();

    Ok(EvidenceStoreStatus {
        runtime_evidence_records: count_jsonl_records(&runtime_evidence_file)?,
        project_artifacts: count_files(&project_evidence_dir)?,
        runtime_evidence_file,
        project_evidence_dir,
        stale_policy: stale_policy_summary(),
    })
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

fn count_jsonl_records(path: &Path) -> Result<usize, AppError> {
    if !path.exists() {
        return Ok(0);
    }

    let body = fs::read_to_string(path).map_err(|err| {
        AppError::runtime(format!(
            "runtime evidence store를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    Ok(body.lines().filter(|line| !line.trim().is_empty()).count())
}

fn count_files(path: &Path) -> Result<usize, AppError> {
    if !path.exists() {
        return Ok(0);
    }

    let mut count = 0;
    for entry in fs::read_dir(path).map_err(|err| {
        AppError::runtime(format!(
            "project evidence 디렉터리를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })? {
        let entry = entry.map_err(|err| {
            AppError::runtime(format!(
                "project evidence 항목을 읽지 못했습니다: {} ({err})",
                path.display()
            ))
        })?;
        let file_type = entry.file_type().map_err(|err| {
            AppError::runtime(format!(
                "project evidence 항목 타입을 읽지 못했습니다: {} ({err})",
                entry.path().display()
            ))
        })?;
        if file_type.is_file() {
            count += 1;
        } else if file_type.is_dir() {
            count += count_files(&entry.path())?;
        }
    }
    Ok(count)
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

    #[test]
    fn store_status_counts_runtime_records_and_project_artifacts() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-evidence-store-test-{}",
            std::process::id()
        ));
        let project = root.join("project");
        let data = root.join("data");
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
        std::env::set_var("RPOTATO_DATA_HOME", &data);

        fs::create_dir_all(paths::state_dir()).unwrap();
        fs::create_dir_all(paths::project_evidence_dir().join("nested")).unwrap();
        fs::write(
            paths::runtime_evidence_file(),
            "{\"evidence_id\":\"one\"}\n\n{\"evidence_id\":\"two\"}\n",
        )
        .unwrap();
        fs::write(paths::project_evidence_dir().join("one.txt"), "one").unwrap();
        fs::write(
            paths::project_evidence_dir().join("nested").join("two.txt"),
            "two",
        )
        .unwrap();

        let status = store_status().unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");

        assert_eq!(status.runtime_evidence_records, 2);
        assert_eq!(status.project_artifacts, 2);
        assert_eq!(status.stale_policy, stale_policy_summary());
    }
}
