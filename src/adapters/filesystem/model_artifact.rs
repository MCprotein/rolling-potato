//! Model artifact filesystem facade.

mod cache;
mod download;
mod store;

pub(crate) use cache::{
    cleanup_failed_artifacts, local_artifact_candidate_present, local_artifact_state,
    model_artifact_part_path, model_artifact_path, sha256_for_file, vision_projector_artifact_path,
    vision_projector_part_path,
};
pub(crate) use download::{fetch_evaluation_artifact, fetch_managed_projector_artifact};
pub(crate) use store::{
    paths, promotion_evidence_path, read_default_selection, read_promotion_evidence,
    read_registry_entries, registry_path, write_promotion_evidence, write_registry_entry,
};

use crate::runtime_core::inference::model::manifest::ModelArtifactFetchStatus;
#[cfg(test)]
use download::remove_invalid_managed_projector;

impl ModelArtifactFetchStatus {
    pub(crate) fn label(self) -> &'static str {
        match self {
            ModelArtifactFetchStatus::Downloaded => "downloaded",
            ModelArtifactFetchStatus::Resumed => "resumed",
            ModelArtifactFetchStatus::CacheHit => "cache-hit",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_core::inference::model::manifest::{
        find_candidate, source_backed_artifact, source_backed_vision_projector,
        ModelArtifactDescriptor,
    };
    use std::fs;

    const SHA_ZERO: &str = "0000000000000000000000000000000000000000000000000000000000000000";
    const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn projector(sha256: &'static str) -> ModelArtifactDescriptor {
        ModelArtifactDescriptor {
            provider: "test",
            url: "https://example.com/projector.gguf",
            terms_url: "https://example.com/terms",
            file_name: "projector.gguf",
            sha256,
            size_bytes: 3,
        }
    }

    #[test]
    fn managed_projector_removes_a_corrupt_cached_file_before_recovery() {
        let root =
            std::env::temp_dir().join(format!("rpotato-projector-recovery-{}", std::process::id()));
        let path = root.join("projector.gguf");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(&path, b"bad").unwrap();

        remove_invalid_managed_projector(projector(SHA_ZERO), &path).unwrap();

        assert!(!path.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn model_upgrade_compatibility_verified_projector_cache_hit_never_redownloads() {
        let root =
            std::env::temp_dir().join(format!("rpotato-projector-cache-{}", std::process::id()));
        let path = root.join("projector.gguf");
        let part_path = root.join("projector.gguf.part");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(&path, b"abc").unwrap();
        let artifact =
            projector("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");

        let status = fetch_managed_projector_artifact(artifact, &path, &part_path).unwrap();

        assert_eq!(status, ModelArtifactFetchStatus::CacheHit);
        assert_eq!(fs::read(&path).unwrap(), b"abc");
        assert!(!part_path.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn model_upgrade_compatibility_verified_model_cache_hit_never_redownloads() {
        let root = std::env::temp_dir().join(format!("rpotato-model-cache-{}", std::process::id()));
        let path = root.join("model.gguf");
        let part_path = root.join("model.gguf.part");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(&path, b"abc").unwrap();
        let artifact = ModelArtifactDescriptor {
            provider: "test",
            url: "https://example.invalid/model.gguf",
            terms_url: "https://example.invalid/terms",
            file_name: "model.gguf",
            sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            size_bytes: 3,
        };

        let status = fetch_evaluation_artifact(artifact, &path, &part_path).unwrap();

        assert_eq!(status, ModelArtifactFetchStatus::CacheHit);
        assert_eq!(fs::read(&path).unwrap(), b"abc");
        assert!(!part_path.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn projector_partial_cache_is_scoped_to_the_expected_revision() {
        let candidate = find_candidate("gemma-4-e4b").unwrap();
        let first = vision_projector_part_path(candidate, projector(SHA_A));
        let second = vision_projector_part_path(candidate, projector(SHA_B));

        assert_ne!(first, second);
    }

    #[test]
    fn evaluation_fetch_paths_stay_under_app_data() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let data_root =
            std::env::temp_dir().join(format!("rpotato-fetch-path-test-{}", std::process::id()));
        std::env::set_var("RPOTATO_DATA_HOME", &data_root);
        std::env::set_var("RPOTATO_PROJECT_ROOT", data_root.join("project"));

        let candidate = find_candidate("gemma-4-e4b").unwrap();
        let artifact = source_backed_artifact(candidate).unwrap();
        let projector = source_backed_vision_projector(candidate).unwrap();
        let final_path = model_artifact_path(artifact);
        let part_path = model_artifact_part_path(candidate);
        let projector_path = vision_projector_artifact_path(candidate, projector);
        let projector_part_path = vision_projector_part_path(candidate, projector);

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");

        assert!(final_path.starts_with(data_root.join("models")));
        assert!(part_path.starts_with(data_root.join("downloads")));
        assert!(part_path.ends_with("gemma-4-e4b--model--gemma-4-E4B_q4_0-it.gguf.part"));
        assert!(projector_path.starts_with(data_root.join("models")));
        assert!(projector_path.ends_with("gemma-4-e4b--vision--gemma-4-E4B-it-mmproj.gguf"));
        assert!(projector_part_path.starts_with(data_root.join("downloads")));
        assert!(projector_part_path.ends_with(format!(
            "gemma-4-e4b--vision--gemma-4-E4B-it-mmproj.gguf--{}.part",
            &projector.sha256[..12]
        )));
    }
}
