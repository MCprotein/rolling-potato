//! Source-backed model choices rendered for interactive setup.

use crate::runtime_core::inference::model::manifest::{
    source_backed_artifact, source_backed_vision_projector, ModelManifestEntry, CANDIDATES,
};
use crate::surfaces::tui::runtime_bridge::TuiModelOption;

use super::configured_model_id;

pub(super) fn setup_options() -> Vec<TuiModelOption> {
    let current = configured_model_id();
    CANDIDATES
        .iter()
        .filter(|candidate| source_backed_artifact(candidate).is_ok())
        .map(|candidate| {
            let artifact =
                source_backed_artifact(candidate).expect("source-backed candidates were filtered");
            let artifact_path =
                crate::adapters::filesystem::model_artifact::model_artifact_path(artifact);
            let projector = source_backed_vision_projector(candidate);
            let projector_cached = projector.is_some_and(|projector| {
                let path =
                    crate::adapters::filesystem::model_artifact::vision_projector_artifact_path(
                        candidate, projector,
                    );
                crate::adapters::filesystem::model_artifact::local_artifact_candidate_present(
                    projector, &path,
                )
            });
            TuiModelOption {
                id: candidate.id.to_string(),
                display_name: candidate.display_name.to_string(),
                quantization: candidate.quantization.unwrap_or("미확정").to_string(),
                download_bytes: artifact.size_bytes,
                model_cached:
                    crate::adapters::filesystem::model_artifact::local_artifact_candidate_present(
                        artifact,
                        &artifact_path,
                    ),
                vision_projector_bytes: projector.map(|artifact| artifact.size_bytes),
                vision_projector_cached: projector_cached,
                context_length: candidate.context_length,
                ram: candidate
                    .recommended_ram_gb
                    .map(|value| format!("{value} GiB"))
                    .unwrap_or_else(|| "미확정".to_string()),
                license: if candidate
                    .license
                    .claim
                    .to_ascii_lowercase()
                    .contains("apache-2.0")
                {
                    "Apache-2.0".to_string()
                } else {
                    candidate.license.status.to_string()
                },
                note: model_note(candidate, projector, projector_cached),
                current: current.as_deref() == Some(candidate.id),
                recommended: candidate
                    .setup_profile
                    .is_some_and(|profile| profile.recommended),
            }
        })
        .collect()
}

fn model_note(
    candidate: &ModelManifestEntry,
    projector: Option<crate::runtime_core::inference::model::manifest::ModelArtifactDescriptor>,
    projector_cached: bool,
) -> String {
    let vision = match projector {
        Some(_) if projector_cached => "vision 지원(projector cache 준비됨)",
        Some(projector) => {
            return format!(
                "vision 지원(첫 이미지에서 projector {:.1} GiB 자동 준비); {}",
                projector.size_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
                adoption_note(candidate)
            );
        }
        None => "vision 미지원",
    };
    format!("{vision}; {}", adoption_note(candidate))
}

fn adoption_note(candidate: &ModelManifestEntry) -> &'static str {
    candidate
        .setup_profile
        .map_or("local adoption evidence 미확정", |profile| {
            profile.adoption.claim
        })
}
