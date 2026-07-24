//! Source-backed model choices rendered for interactive setup.

use crate::runtime_core::inference::model::manifest::{
    source_backed_artifact, source_backed_vision_projector, CANDIDATES,
};
use crate::surfaces::tui::runtime_bridge::TuiModelOption;

use super::configured_model_id;

pub(super) fn setup_options() -> Vec<TuiModelOption> {
    let current = configured_model_id();
    CANDIDATES
        .iter()
        .filter(|candidate| source_backed_artifact(candidate).is_ok())
        .map(|candidate| TuiModelOption {
            id: candidate.id.to_string(),
            display_name: candidate.display_name.to_string(),
            quantization: candidate.quantization.unwrap_or("미확정").to_string(),
            download_bytes: candidate.size_bytes.unwrap_or(0).saturating_add(
                source_backed_vision_projector(candidate)
                    .map(|artifact| artifact.size_bytes)
                    .unwrap_or(0),
            ),
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
            note: if candidate.id == "gemma-4-e4b" {
                "vision 지원(mmproj 자동 준비); 로컬 adoption smoke 통과; 16 GB 적합성은 미확정"
                    .to_string()
            } else {
                "vision 지원(mmproj 자동 준비); 실험적 선택; exact-response adoption gate 미통과"
                    .to_string()
            },
            current: current.as_deref() == Some(candidate.id),
            recommended: candidate.id == "gemma-4-e4b",
        })
        .collect()
}
