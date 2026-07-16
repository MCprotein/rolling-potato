use crate::adapters::filesystem::layout;
use crate::runtime_core::inference::model::ModelArtifactPaths;

pub(crate) fn paths() -> ModelArtifactPaths {
    ModelArtifactPaths {
        downloads_dir: layout::downloads_dir(),
        models_dir: layout::models_dir(),
        registry_dir: layout::model_registry_dir(),
        evidence_dir: layout::model_evidence_dir(),
        default_file: layout::model_default_file(),
        observability_db_file: layout::observability_db_file(),
    }
}
