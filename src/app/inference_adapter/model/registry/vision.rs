use std::path::{Path, PathBuf};

use crate::adapters::filesystem::model_artifact::{
    local_artifact_state, model_artifact_path, read_registry_entries,
    vision_projector_artifact_path,
};
use crate::runtime_core::inference::model::manifest::{
    source_backed_artifact, source_backed_vision_projector, ModelArtifactDescriptor,
    ModelManifestEntry, RegistryVisionState,
};

mod preparation;

pub(crate) use preparation::prepare_bound_vision_projector;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VerifiedVisionProjector {
    pub(crate) path: PathBuf,
    pub(crate) sha256: String,
    pub(crate) size_bytes: u64,
}

pub(crate) fn verified_vision_projector(
    model_path: &Path,
    model_sha256: &str,
) -> Option<VerifiedVisionProjector> {
    let candidate = crate::runtime_core::inference::model::manifest::CANDIDATES
        .iter()
        .find(|candidate| candidate.sha256 == Some(model_sha256))?;
    let artifact = source_backed_artifact(candidate).ok()?;
    if model_artifact_path(artifact) != model_path {
        return None;
    }
    let projector = source_backed_vision_projector(candidate)?;
    let expected_path = vision_projector_artifact_path(candidate, projector);
    verified_vision_projector_binding(
        candidate.id,
        model_path,
        model_sha256,
        projector,
        expected_path,
    )
}

pub(super) fn verified_vision_projector_binding(
    model_id: &str,
    model_path: &Path,
    model_sha256: &str,
    projector: ModelArtifactDescriptor,
    expected_path: PathBuf,
) -> Option<VerifiedVisionProjector> {
    let entry = read_registry_entries()
        .ok()?
        .into_iter()
        .find(|entry| entry.id == model_id)?;
    if entry.vision_status != "ready"
        || entry.artifact_sha256 != model_sha256
        || Path::new(&entry.artifact_path) != model_path
    {
        return None;
    }
    if entry
        .mmproj_path
        .as_deref()
        .is_none_or(|path| Path::new(path) != expected_path)
        || entry.mmproj_sha256.as_deref() != Some(projector.sha256)
        || entry.mmproj_size_bytes != Some(projector.size_bytes)
    {
        return None;
    }
    let local = local_artifact_state(projector, &expected_path).ok()?;
    local.verified.then(|| VerifiedVisionProjector {
        path: expected_path,
        sha256: projector.sha256.to_string(),
        size_bytes: projector.size_bytes,
    })
}

pub(super) fn local_registry_vision(candidate: &ModelManifestEntry) -> RegistryVisionState {
    let Some(projector) = source_backed_vision_projector(candidate) else {
        return RegistryVisionState {
            status: "unavailable".to_string(),
            mmproj_path: None,
            mmproj_sha256: None,
            mmproj_size_bytes: None,
        };
    };
    let path = vision_projector_artifact_path(candidate, projector);
    match local_artifact_state(projector, &path) {
        Ok(state) if state.verified => RegistryVisionState {
            status: "ready".to_string(),
            mmproj_path: Some(path.display().to_string()),
            mmproj_sha256: Some(projector.sha256.to_string()),
            mmproj_size_bytes: Some(projector.size_bytes),
        },
        _ => RegistryVisionState {
            status: "unavailable".to_string(),
            mmproj_path: None,
            mmproj_sha256: None,
            mmproj_size_bytes: None,
        },
    }
}
