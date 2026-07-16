use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModelArtifactPaths {
    pub(crate) downloads_dir: PathBuf,
    pub(crate) models_dir: PathBuf,
    pub(crate) registry_dir: PathBuf,
    pub(crate) evidence_dir: PathBuf,
    pub(crate) default_file: PathBuf,
    pub(crate) observability_db_file: PathBuf,
}

impl ModelArtifactPaths {
    pub(crate) fn artifact(&self, file_name: &str) -> PathBuf {
        self.models_dir.join(file_name)
    }

    pub(crate) fn partial(&self, id: &str) -> PathBuf {
        self.downloads_dir.join(format!("{id}.part"))
    }

    pub(crate) fn failed_download(&self, id: &str) -> PathBuf {
        self.downloads_dir.join(format!("{id}.failed"))
    }

    pub(crate) fn failed_model(&self, artifact_name: &str) -> PathBuf {
        self.models_dir.join(format!("{artifact_name}.failed"))
    }

    pub(crate) fn registry_entry(&self, id: &str) -> PathBuf {
        self.registry_dir.join(format!("{id}.json"))
    }

    pub(crate) fn promotion_evidence(&self, id: &str) -> PathBuf {
        self.evidence_dir.join(format!("{id}.promotion.json"))
    }
}
