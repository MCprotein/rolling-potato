//! Local artifact cache state, cleanup, and revision-scoped paths.

use std::fs;
use std::path::{Path, PathBuf};

use super::paths;
use crate::foundation::error::AppError;
use crate::foundation::integrity as checksum;
use crate::runtime_core::inference::model::manifest::{
    LocalArtifactState, ModelArtifactDescriptor, ModelManifestEntry,
};

pub(crate) struct FailedArtifactCleanup {
    pub(crate) rows: Vec<String>,
    pub(crate) removed: usize,
    pub(crate) missing: usize,
}

pub(crate) fn failed_artifact_paths(candidate: &ModelManifestEntry) -> Vec<PathBuf> {
    let artifact_name = candidate.artifact_name.unwrap_or(candidate.id);
    let mut paths = vec![
        paths().partial(&artifact_download_key(candidate.id, "model", artifact_name)),
        paths().failed_download(candidate.id),
        paths().failed_model(artifact_name),
        paths().partial(candidate.id),
    ];
    if let Some(projector) = candidate.vision_projector {
        let projector_key = projector_download_key(candidate, projector);
        let legacy_key = artifact_download_key(candidate.id, "vision", projector.file_name);
        for key in [projector_key, legacy_key] {
            paths.push(self::paths().partial(&key));
            paths.push(self::paths().failed_download(&key));
            paths.push(self::paths().failed_model(&key));
        }
    }
    paths
}

pub(crate) fn sha256_for_file(path: &Path) -> Result<String, AppError> {
    if !path.is_file() {
        return Err(AppError::usage(format!(
            "검증 대상 파일을 찾지 못했습니다: {}",
            path.display()
        )));
    }
    checksum::sha256_file(path)
}

pub(crate) fn local_artifact_candidate_present(
    artifact: ModelArtifactDescriptor,
    path: &Path,
) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.file_type().is_file() && metadata.len() == artifact.size_bytes
    })
}

pub(crate) fn cleanup_failed_artifacts(
    candidate: &ModelManifestEntry,
    dry_run: bool,
) -> Result<FailedArtifactCleanup, AppError> {
    let mut rows = Vec::new();
    let mut removed = 0;
    let mut missing = 0;

    for path in failed_artifact_paths(candidate) {
        if !path.exists() {
            missing += 1;
            rows.push(format!("- {} | missing", path.display()));
            continue;
        }
        if !path.is_file() {
            return Err(AppError::blocked(format!(
                "failed artifact cleanup 대상은 file이어야 합니다: {}",
                path.display()
            )));
        }
        if dry_run {
            rows.push(format!("- {} | would delete", path.display()));
            continue;
        }
        fs::remove_file(&path).map_err(|err| {
            AppError::runtime(format!(
                "failed artifact를 삭제하지 못했습니다: {} ({err})",
                path.display()
            ))
        })?;
        removed += 1;
        rows.push(format!("- {} | deleted", path.display()));
    }

    Ok(FailedArtifactCleanup {
        rows,
        removed,
        missing,
    })
}

pub(crate) fn local_artifact_state(
    artifact: ModelArtifactDescriptor,
    final_path: &Path,
) -> Result<LocalArtifactState, AppError> {
    if !final_path.exists() {
        return Ok(LocalArtifactState {
            status: "missing",
            detail: "final artifact file is not present under app data models/".to_string(),
            verified: false,
        });
    }
    if !final_path.is_file() {
        return Ok(LocalArtifactState {
            status: "path-not-file",
            detail: format!(
                "final artifact path is not a file: {}",
                final_path.display()
            ),
            verified: false,
        });
    }

    let metadata = final_path.metadata().map_err(|err| {
        AppError::runtime(format!(
            "model artifact metadata를 읽지 못했습니다: {} ({err})",
            final_path.display()
        ))
    })?;
    if metadata.len() != artifact.size_bytes {
        return Ok(LocalArtifactState {
            status: "size-mismatch",
            detail: format!(
                "expected {} bytes but found {} bytes",
                artifact.size_bytes,
                metadata.len()
            ),
            verified: false,
        });
    }

    let actual_sha256 = checksum::sha256_file(final_path)?;
    if !actual_sha256.eq_ignore_ascii_case(artifact.sha256) {
        return Ok(LocalArtifactState {
            status: "sha256-mismatch",
            detail: format!("expected {} but found {}", artifact.sha256, actual_sha256),
            verified: false,
        });
    }

    Ok(LocalArtifactState {
        status: "verified-local-artifact",
        detail: "size and SHA-256 match the source-recorded manifest fields".to_string(),
        verified: true,
    })
}

pub(crate) fn model_artifact_path(artifact: ModelArtifactDescriptor) -> PathBuf {
    paths().artifact(artifact.file_name)
}

pub(crate) fn model_artifact_part_path(candidate: &ModelManifestEntry) -> PathBuf {
    paths().partial(&artifact_download_key(
        candidate.id,
        "model",
        candidate.artifact_name.unwrap_or(candidate.id),
    ))
}

pub(crate) fn vision_projector_artifact_path(
    candidate: &ModelManifestEntry,
    artifact: ModelArtifactDescriptor,
) -> PathBuf {
    paths().artifact(&artifact_download_key(
        candidate.id,
        "vision",
        artifact.file_name,
    ))
}

pub(crate) fn vision_projector_part_path(
    candidate: &ModelManifestEntry,
    artifact: ModelArtifactDescriptor,
) -> PathBuf {
    paths().partial(&projector_download_key(candidate, artifact))
}

fn projector_download_key(
    candidate: &ModelManifestEntry,
    artifact: ModelArtifactDescriptor,
) -> String {
    let revision = artifact.sha256.get(..12).unwrap_or(artifact.sha256);
    artifact_download_key(
        candidate.id,
        "vision",
        &format!("{}--{revision}", artifact.file_name),
    )
}

fn artifact_download_key(candidate_id: &str, kind: &str, file_name: &str) -> String {
    format!(
        "{}--{}--{}",
        safe_artifact_key(candidate_id),
        safe_artifact_key(kind),
        safe_artifact_key(file_name)
    )
}

fn safe_artifact_key(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                character
            } else {
                '_'
            }
        })
        .take(180)
        .collect()
}
