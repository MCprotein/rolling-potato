use crate::foundation::error::AppError;
use crate::foundation::integrity as checksum;

use super::{
    CandidateStatus, InstallValidation, ModelArtifactDescriptor, ModelManifestEntry, CANDIDATES,
};

pub(crate) fn find_candidate(id: &str) -> Result<&'static ModelManifestEntry, AppError> {
    CANDIDATES
        .iter()
        .find(|candidate| candidate.id == id)
        .ok_or_else(|| {
            AppError::usage(format!(
                "알 수 없는 모델 id입니다: {id}\n사용 가능 후보는 `rpotato model list`로 확인하세요."
            ))
        })
}

pub(crate) fn quantization_for_artifact_hash(hash: &str) -> Option<&'static str> {
    CANDIDATES
        .iter()
        .find(|candidate| candidate.sha256 == Some(hash))
        .and_then(|candidate| candidate.quantization)
}

pub(crate) fn validate_install_ready(candidate: &ModelManifestEntry) -> InstallValidation {
    let mut blockers = Vec::new();

    if candidate.status != CandidateStatus::Verified {
        push_unique(
            &mut blockers,
            "manifest status가 verified가 아니므로 설치할 수 없습니다.",
        );
    }

    for blocker in candidate.install_blockers {
        push_unique(&mut blockers, blocker);
    }

    if candidate.artifact_provider.is_none() {
        push_unique(&mut blockers, "artifact provider 미확정");
    }
    if candidate.artifact_url.is_none() {
        push_unique(&mut blockers, "GGUF artifact URL 미확정");
    }
    if candidate.artifact_terms_url.is_none() {
        push_unique(&mut blockers, "artifact terms URL 미확정");
    }
    if candidate.artifact_name.is_none() {
        push_unique(&mut blockers, "artifact file name 미확정");
    }
    if candidate.quantization.is_none() {
        push_unique(&mut blockers, "quantization 미확정");
    }
    if candidate.size_bytes.is_none() {
        push_unique(&mut blockers, "file size 미확정");
    }
    if candidate.backend_compatibility.is_none() {
        push_unique(&mut blockers, "backend compatibility source 미확정");
    }
    if candidate.recommended_ram_gb.is_none() {
        push_unique(&mut blockers, "recommended RAM source 미확정");
    }

    match candidate.sha256 {
        Some(hash) if checksum::is_valid_sha256(hash) => {}
        Some(_) => push_unique(&mut blockers, "SHA-256 형식 오류"),
        None => push_unique(&mut blockers, "SHA-256 미확정"),
    }

    InstallValidation {
        ready: blockers.is_empty(),
        blockers,
    }
}

pub(crate) fn source_backed_artifact(
    candidate: &'static ModelManifestEntry,
) -> Result<ModelArtifactDescriptor, AppError> {
    let blockers = source_backed_artifact_blockers(candidate);
    if !blockers.is_empty() {
        return Err(fetch_blocked(candidate, blockers));
    }

    Ok(ModelArtifactDescriptor {
        provider: candidate
            .artifact_provider
            .expect("validated artifact provider"),
        url: candidate.artifact_url.expect("validated artifact url"),
        terms_url: candidate
            .artifact_terms_url
            .expect("validated artifact terms url"),
        file_name: candidate.artifact_name.expect("validated artifact name"),
        sha256: candidate.sha256.expect("validated artifact sha256"),
        size_bytes: candidate.size_bytes.expect("validated artifact size"),
    })
}

pub(crate) fn source_backed_vision_projector(
    candidate: &ModelManifestEntry,
) -> Option<ModelArtifactDescriptor> {
    candidate.vision_projector.filter(|artifact| {
        !artifact.provider.trim().is_empty()
            && artifact.url.starts_with("https://")
            && artifact.terms_url.starts_with("https://")
            && !artifact.file_name.trim().is_empty()
            && checksum::is_valid_sha256(artifact.sha256)
            && artifact.size_bytes > 0
    })
}

pub(crate) fn source_backed_artifact_blockers(candidate: &ModelManifestEntry) -> Vec<&'static str> {
    let mut blockers = Vec::new();

    if candidate.artifact_provider.is_none() {
        blockers.push("artifact provider 미확정");
    }
    if candidate.artifact_url.is_none() {
        blockers.push("GGUF artifact URL 미확정");
    }
    if candidate.artifact_terms_url.is_none() {
        blockers.push("artifact terms URL 미확정");
    }
    if candidate.artifact_name.is_none() {
        blockers.push("artifact file name 미확정");
    }
    match candidate.sha256 {
        Some(hash) if checksum::is_valid_sha256(hash) => {}
        Some(_) => blockers.push("SHA-256 형식 오류"),
        None => blockers.push("SHA-256 미확정"),
    }
    if candidate.size_bytes.is_none() {
        blockers.push("file size 미확정");
    }
    if candidate.format != "gguf" {
        blockers.push("GGUF format이 아닙니다");
    }
    if candidate.backend != "llama.cpp" {
        blockers.push("llama.cpp backend 후보가 아닙니다");
    }

    blockers
}

fn fetch_blocked(candidate: &ModelManifestEntry, blockers: Vec<&str>) -> AppError {
    AppError::blocked(format!(
        "검증용 model artifact fetch 차단\n- id: {}\n- status: {}\n- blockers: {}\n- 동작: source-backed artifact URL, terms, size, SHA-256이 모두 있어야 검증용 fetch를 실행합니다.",
        candidate.id,
        candidate.status.label(),
        blockers.join(", ")
    ))
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_string());
    }
}
