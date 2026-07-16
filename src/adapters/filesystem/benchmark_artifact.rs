use std::fs;
use std::path::{Path, PathBuf};

use crate::adapters::filesystem::layout;
use crate::foundation::error::AppError;
use crate::foundation::integrity as checksum;
use crate::runtime_core::inference::benchmark::fixture::{
    parse_fixture, BenchmarkFixture, BenchmarkPromptArtifact,
};

pub(crate) fn read_fixture(path: &str) -> Result<BenchmarkFixture, AppError> {
    let path = project_local_file(path)?;
    let text = fs::read_to_string(&path).map_err(|err| {
        AppError::runtime(format!(
            "benchmark fixture를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    let sha256 = checksum::sha256_file(&path)?;

    parse_fixture(&text, path, sha256)
}

pub(crate) fn read_prompt_artifact(path: &str) -> Result<BenchmarkPromptArtifact, AppError> {
    let path = project_local_file(path)?;
    let text = fs::read_to_string(&path).map_err(|err| {
        AppError::runtime(format!(
            "benchmark prompt artifact를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    if text.trim().is_empty() {
        return Err(AppError::usage(
            "benchmark prompt artifact는 비어 있을 수 없습니다.",
        ));
    }
    let chars = u32::try_from(text.chars().count()).unwrap_or(u32::MAX);
    Ok(BenchmarkPromptArtifact {
        sha256: checksum::sha256_file(&path)?,
        path,
        text,
        chars,
    })
}

fn project_local_file(path: &str) -> Result<PathBuf, AppError> {
    if path.starts_with("http://") || path.starts_with("https://") {
        return Err(AppError::usage(
            "benchmark fixture path는 remote URL일 수 없습니다.",
        ));
    }

    let project_root = layout::project_root().canonicalize().map_err(|err| {
        AppError::runtime(format!(
            "project root를 확인하지 못했습니다: {} ({err})",
            layout::project_root().display()
        ))
    })?;
    let candidate = Path::new(path);
    let full_path = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        project_root.join(candidate)
    };
    let canonical = full_path.canonicalize().map_err(|err| {
        AppError::usage(format!(
            "benchmark fixture path를 찾지 못했습니다: {} ({err})",
            full_path.display()
        ))
    })?;
    if !canonical.starts_with(&project_root) {
        return Err(AppError::usage(
            "benchmark fixture는 project root 안의 파일이어야 합니다.",
        ));
    }
    let metadata = fs::metadata(&canonical).map_err(|err| {
        AppError::runtime(format!(
            "benchmark fixture metadata를 읽지 못했습니다: {} ({err})",
            canonical.display()
        ))
    })?;
    if !metadata.is_file() {
        return Err(AppError::usage(
            "benchmark fixture path는 파일이어야 합니다.",
        ));
    }
    Ok(canonical)
}
