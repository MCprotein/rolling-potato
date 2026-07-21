//! Immutable compaction artifact storage and hash-chain validation.

use std::collections::BTreeSet;
#[cfg(test)]
use std::fs;

use crate::adapters::filesystem::{layout as paths, lease};
use crate::app::workflow_adapter::{ledger, state};
use crate::foundation::error::AppError;
use crate::runtime_core::knowledge::compaction::{
    parse_artifact, render_artifact, CompactionArtifact,
};
use crate::runtime_core::workflow::storage_compat::transcript::TranscriptRecord;

const MAX_COMPACTION_ARTIFACT_BYTES: u64 = 64 * 1024;
const MAX_COMPACTION_CHAIN_DEPTH: usize = 64;

pub(super) fn install_artifact(artifact: &CompactionArtifact) -> Result<String, AppError> {
    let identity = ledger::validated_current_identity()?;
    if artifact.project_id != identity.project_id || artifact.session_id != identity.session_id {
        return Err(AppError::blocked(
            "compaction artifact current project/session binding 불일치",
        ));
    }
    let body = render_artifact(artifact);
    if parse_artifact(&body, "compaction artifact install")? != *artifact {
        return Err(AppError::blocked(
            "compaction artifact canonical round-trip 불일치",
        ));
    }
    let path = paths::compaction_file(
        &artifact.project_id,
        &artifact.session_id,
        &artifact.artifact_id,
    );
    let _lease = lease::RecoverableLease::acquire(
        path.with_extension("checkpoint.lock"),
        "compaction artifact checkpoint",
    )?;
    if path.exists() {
        let existing = state::read_regular_file_bounded(
            &path,
            MAX_COMPACTION_ARTIFACT_BYTES,
            "compaction artifact",
        )?;
        if existing != body {
            return Err(AppError::blocked("compaction artifact immutable conflict"));
        }
    } else {
        crate::adapters::filesystem::atomic_write::atomic_replace_bytes(&path, body.as_bytes())?;
    }
    Ok(relative_artifact_path(artifact))
}

pub(crate) fn load_current_artifact(
    session_id: &str,
) -> Result<Option<CompactionArtifact>, AppError> {
    let records = crate::app::workflow_adapter::transcript::records_for_session(session_id)?;
    load_current_artifact_from_records(session_id, &records)
}

pub(super) fn load_current_artifact_from_records(
    session_id: &str,
    records: &[TranscriptRecord],
) -> Result<Option<CompactionArtifact>, AppError> {
    let Some(pointer) = state::current_compaction_boundary(session_id)? else {
        return Ok(None);
    };
    let head = load_artifact_pointer(&pointer, session_id)?;
    validate_artifact_chain(&pointer, &head, session_id, records)?;
    Ok(Some(head))
}

fn validate_artifact_chain(
    head_pointer: &str,
    head: &CompactionArtifact,
    session_id: &str,
    records: &[TranscriptRecord],
) -> Result<(), AppError> {
    let mut visited = BTreeSet::from([head_pointer.to_string()]);
    let mut child = head.clone();
    let mut child_boundary = boundary_index(records, &child.boundary_record_id)?;
    for _ in 0..MAX_COMPACTION_CHAIN_DEPTH {
        if child.previous_artifact_path == "none" {
            return Ok(());
        }
        if !visited.insert(child.previous_artifact_path.clone()) {
            return Err(AppError::blocked("compaction artifact chain cycle 감지"));
        }
        let previous = load_artifact_pointer(&child.previous_artifact_path, session_id)?;
        if previous.artifact_hash != child.previous_artifact_hash {
            return Err(AppError::blocked(
                "compaction artifact chain previous hash 불일치",
            ));
        }
        let previous_boundary = boundary_index(records, &previous.boundary_record_id)?;
        if previous_boundary >= child_boundary {
            return Err(AppError::blocked(
                "compaction artifact chain boundary 순서 불일치",
            ));
        }
        child = previous;
        child_boundary = previous_boundary;
    }
    Err(AppError::blocked(
        "compaction artifact chain depth 상한 초과",
    ))
}

fn boundary_index(records: &[TranscriptRecord], record_id: &str) -> Result<usize, AppError> {
    records
        .iter()
        .position(|record| record.record_id == record_id)
        .ok_or_else(|| AppError::blocked("compaction artifact boundary transcript 누락"))
}

fn load_artifact_pointer(pointer: &str, session_id: &str) -> Result<CompactionArtifact, AppError> {
    let identity = ledger::validated_current_identity()?;
    let expected_prefix = format!("state/compactions/{}/{session_id}/", identity.project_id);
    if !pointer.starts_with(&expected_prefix)
        || !pointer.ends_with(".json")
        || pointer
            .split('/')
            .any(|part| part.is_empty() || part == "..")
    {
        return Err(AppError::blocked(
            "compaction artifact pointer project/session boundary 불일치",
        ));
    }
    let path = paths::app_data_root().join(pointer);
    let body = state::read_regular_file_bounded(
        &path,
        MAX_COMPACTION_ARTIFACT_BYTES,
        "compaction resume artifact",
    )?;
    let artifact = parse_artifact(&body, "compaction resume artifact")?;
    if artifact.project_id != identity.project_id
        || artifact.session_id != session_id
        || relative_artifact_path(&artifact) != pointer
    {
        return Err(AppError::blocked(
            "compaction resume artifact identity/path binding 불일치",
        ));
    }
    Ok(artifact)
}

pub(super) fn relative_artifact_path(artifact: &CompactionArtifact) -> String {
    format!(
        "state/compactions/{}/{}/{}.json",
        artifact.project_id, artifact.session_id, artifact.artifact_id
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::integrity::sha256_text;
    use crate::runtime_core::knowledge::compaction::{
        render_artifact_payload, CompactionCheckpoint, COMPACTION_SCHEMA_VERSION,
    };

    #[test]
    fn immutable_artifact_install_round_trips_and_rejects_conflict() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-compaction-artifact-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        fs::create_dir_all(root.join("project")).unwrap();
        state::initialize().unwrap();
        let identity = ledger::validated_current_identity().unwrap();
        let mut artifact = CompactionArtifact {
            schema_version: COMPACTION_SCHEMA_VERSION,
            artifact_id: "compaction-storage-test".to_string(),
            project_id: identity.project_id,
            session_id: identity.session_id,
            boundary_record_id: "transcript-boundary".to_string(),
            previous_artifact_path: "none".to_string(),
            previous_artifact_hash: "none".to_string(),
            post_compact_target_tokens: 1_638,
            source_record_count: 5,
            source_records_dropped: 0,
            recent_record_ids: vec!["transcript-recent".to_string()],
            checkpoint: CompactionCheckpoint {
                current_task: "resume compaction".to_string(),
                ..CompactionCheckpoint::default()
            },
            summary_model_id: "deterministic-fallback".to_string(),
            created_at_ms: 1,
            artifact_hash: String::new(),
        };
        artifact.artifact_hash = sha256_text(&render_artifact_payload(&artifact));

        let pointer = install_artifact(&artifact).unwrap();
        assert_eq!(
            load_artifact_pointer(&pointer, &artifact.session_id).unwrap(),
            artifact
        );
        assert_eq!(install_artifact(&artifact).unwrap(), pointer);

        let path = paths::app_data_root().join(&pointer);
        fs::write(&path, "corrupt").unwrap();
        assert!(install_artifact(&artifact).is_err());

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn current_artifact_load_validates_the_full_hash_chain_and_cas_pointer() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-compaction-chain-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        fs::create_dir_all(root.join("project")).unwrap();
        state::initialize().unwrap();
        let workflow = state::create_workflow("compaction chain test").unwrap();
        for index in 0..6 {
            crate::app::workflow_adapter::transcript::record_workflow_turn(
                &workflow,
                if index % 2 == 0 { "user" } else { "model" },
                &format!("turn-{index}"),
                &format!("turn {index}"),
                &[],
            )
            .unwrap();
        }
        let records =
            crate::app::workflow_adapter::transcript::records_for_session(&workflow.session_id)
                .unwrap();
        let make_artifact = |artifact_id: &str,
                             boundary_record_id: &str,
                             previous_artifact_path: String,
                             previous_artifact_hash: String,
                             created_at_ms: u128| {
            let mut artifact = CompactionArtifact {
                schema_version: COMPACTION_SCHEMA_VERSION,
                artifact_id: artifact_id.to_string(),
                project_id: workflow.project_id.clone(),
                session_id: workflow.session_id.clone(),
                boundary_record_id: boundary_record_id.to_string(),
                previous_artifact_path,
                previous_artifact_hash,
                post_compact_target_tokens: 1_638,
                source_record_count: 1,
                source_records_dropped: 0,
                recent_record_ids: Vec::new(),
                checkpoint: CompactionCheckpoint {
                    current_task: "validate chain".to_string(),
                    ..CompactionCheckpoint::default()
                },
                summary_model_id: "deterministic-fallback".to_string(),
                created_at_ms,
                artifact_hash: String::new(),
            };
            artifact.artifact_hash = sha256_text(&render_artifact_payload(&artifact));
            artifact
        };

        let first = make_artifact(
            "compaction-chain-first",
            &records[0].record_id,
            "none".to_string(),
            "none".to_string(),
            1,
        );
        let first_pointer = install_artifact(&first).unwrap();
        state::record_compaction_boundary(
            &first_pointer,
            &first.artifact_hash,
            &first.boundary_record_id,
            None,
        )
        .unwrap();

        let second = make_artifact(
            "compaction-chain-second",
            &records[1].record_id,
            first_pointer.clone(),
            first.artifact_hash.clone(),
            2,
        );
        let second_pointer = install_artifact(&second).unwrap();
        assert!(state::record_compaction_boundary(
            &second_pointer,
            &second.artifact_hash,
            &second.boundary_record_id,
            None,
        )
        .is_err());
        state::record_compaction_boundary(
            &second_pointer,
            &second.artifact_hash,
            &second.boundary_record_id,
            Some(first_pointer.clone()),
        )
        .unwrap();
        assert_eq!(
            load_current_artifact(&workflow.session_id)
                .unwrap()
                .unwrap(),
            second
        );

        let invalid = make_artifact(
            "compaction-chain-invalid",
            &records[2].record_id,
            second_pointer.clone(),
            "b".repeat(64),
            3,
        );
        let invalid_pointer = install_artifact(&invalid).unwrap();
        state::record_compaction_boundary(
            &invalid_pointer,
            &invalid.artifact_hash,
            &invalid.boundary_record_id,
            Some(second_pointer),
        )
        .unwrap();
        let error = load_current_artifact(&workflow.session_id).unwrap_err();
        assert!(error.message.contains("previous hash 불일치"));

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        let _ = fs::remove_dir_all(root);
    }
}
