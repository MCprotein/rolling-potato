use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::adapters::filesystem::{atomic_write, layout};
use crate::foundation::error::AppError;
use crate::foundation::integrity as checksum;
use crate::runtime_core::inference::model::codec::{parse_default_selection, parse_registry_entry};
use crate::runtime_core::inference::model::manifest::{
    DefaultSelection, LocalArtifactState, ModelArtifactDescriptor, ModelArtifactFetchStatus,
    ModelManifestEntry, RegistryEntry,
};
use crate::runtime_core::inference::model::ModelArtifactPaths;

const DOWNLOAD_BUFFER_BYTES: usize = 64 * 1024;

pub(crate) struct FailedArtifactCleanup {
    pub(crate) rows: Vec<String>,
    pub(crate) removed: usize,
    pub(crate) missing: usize,
}

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

pub(crate) fn registry_path(id: &str) -> PathBuf {
    paths().registry_entry(id)
}

pub(crate) fn promotion_evidence_path(id: &str) -> PathBuf {
    paths().promotion_evidence(id)
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

pub(crate) fn write_registry_entry(id: &str, contents: &str) -> Result<(), AppError> {
    let path = registry_path(id);
    atomic_write::atomic_replace_bytes(&path, contents.as_bytes())
}

pub(crate) fn read_registry_entries() -> Result<Vec<RegistryEntry>, AppError> {
    let dir = paths().registry_dir;
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|err| {
        AppError::runtime(format!(
            "model registry directory를 읽지 못했습니다: {} ({err})",
            dir.display()
        ))
    })? {
        let entry = entry.map_err(|err| {
            AppError::runtime(format!(
                "model registry entry를 읽지 못했습니다: {} ({err})",
                dir.display()
            ))
        })?;
        if !entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
        {
            continue;
        }
        let text = fs::read_to_string(entry.path()).map_err(|err| {
            AppError::runtime(format!(
                "model registry entry를 읽지 못했습니다: {} ({err})",
                entry.path().display()
            ))
        })?;
        entries.push(parse_registry_entry(&text)?);
    }
    entries.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(entries)
}

pub(crate) fn write_promotion_evidence(id: &str, contents: &str) -> Result<(), AppError> {
    let path = promotion_evidence_path(id);
    atomic_write::atomic_replace_bytes(&path, contents.as_bytes())
}

pub(crate) fn read_promotion_evidence(path: &Path) -> Result<String, AppError> {
    fs::read_to_string(path).map_err(|err| {
        AppError::runtime(format!(
            "model promotion evidence를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })
}

pub(crate) fn read_default_selection() -> Result<DefaultSelection, AppError> {
    let path = paths().default_file;
    if !path.exists() {
        return Err(AppError::blocked(format!(
            "기본 모델이 선택되지 않았습니다. `rpotato model default <id>`를 실행하세요.\n- selection: {}",
            path.display()
        )));
    }
    let text = fs::read_to_string(&path).map_err(|err| {
        AppError::runtime(format!(
            "기본 모델 선택을 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    parse_default_selection(&text)
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

pub(crate) fn fetch_evaluation_artifact(
    artifact: ModelArtifactDescriptor,
    final_path: &Path,
    part_path: &Path,
) -> Result<ModelArtifactFetchStatus, AppError> {
    if final_path.exists() && !final_path.is_file() {
        return Err(AppError::blocked(format!(
            "model artifact final path가 file이 아닙니다: {}",
            final_path.display()
        )));
    }
    if final_path.is_file() {
        if model_artifact_matches(artifact, final_path)? {
            return Ok(ModelArtifactFetchStatus::CacheHit);
        }
        return Err(AppError::blocked(format!(
            "기존 model artifact가 manifest와 일치하지 않아 덮어쓰지 않습니다.\n- path: {}\n- expected size: {}\n- expected sha256: {}\n- 다음 단계: 파일을 수동으로 이동하거나 삭제한 뒤 다시 실행하세요.",
            final_path.display(),
            artifact.size_bytes,
            artifact.sha256
        )));
    }

    let final_parent = final_path.parent().ok_or_else(|| {
        AppError::runtime(format!(
            "model artifact final parent path를 계산하지 못했습니다: {}",
            final_path.display()
        ))
    })?;
    let part_parent = part_path.parent().ok_or_else(|| {
        AppError::runtime(format!(
            "model artifact partial parent path를 계산하지 못했습니다: {}",
            part_path.display()
        ))
    })?;
    fs::create_dir_all(final_parent).map_err(|err| {
        AppError::runtime(format!(
            "model artifact directory를 만들지 못했습니다: {} ({err})",
            final_parent.display()
        ))
    })?;
    fs::create_dir_all(part_parent).map_err(|err| {
        AppError::runtime(format!(
            "model artifact download directory를 만들지 못했습니다: {} ({err})",
            part_parent.display()
        ))
    })?;

    let existing_bytes = partial_artifact_size(part_path, artifact)?;
    if existing_bytes == artifact.size_bytes {
        verify_model_artifact_file(artifact, part_path)?;
        place_verified_artifact(part_path, final_path)?;
        return Ok(ModelArtifactFetchStatus::Resumed);
    }

    let (start_offset, resumed) =
        download_model_artifact_stream(artifact, part_path, existing_bytes)?;
    verify_partial_size(part_path, artifact, start_offset)?;
    verify_model_artifact_file(artifact, part_path)?;
    place_verified_artifact(part_path, final_path)?;

    if resumed {
        Ok(ModelArtifactFetchStatus::Resumed)
    } else {
        Ok(ModelArtifactFetchStatus::Downloaded)
    }
}

pub(crate) fn fetch_managed_projector_artifact(
    artifact: ModelArtifactDescriptor,
    final_path: &Path,
    part_path: &Path,
) -> Result<ModelArtifactFetchStatus, AppError> {
    remove_invalid_managed_projector(artifact, final_path)?;
    fetch_evaluation_artifact(artifact, final_path, part_path)
}

fn remove_invalid_managed_projector(
    artifact: ModelArtifactDescriptor,
    final_path: &Path,
) -> Result<(), AppError> {
    if final_path.exists() && !final_path.is_file() {
        return Err(AppError::blocked(format!(
            "vision projector final path가 file이 아닙니다: {}",
            final_path.display()
        )));
    }
    if final_path.is_file() && !model_artifact_matches(artifact, final_path)? {
        fs::remove_file(final_path).map_err(|err| {
            AppError::runtime(format!(
                "손상되었거나 revision이 바뀐 app-managed vision projector를 교체하지 못했습니다: {} ({err})",
                final_path.display()
            ))
        })?;
    }
    Ok(())
}

fn partial_artifact_size(
    part_path: &Path,
    artifact: ModelArtifactDescriptor,
) -> Result<u64, AppError> {
    if !part_path.exists() {
        return Ok(0);
    }
    if !part_path.is_file() {
        return Err(AppError::blocked(format!(
            "model artifact partial path가 file이 아닙니다: {}",
            part_path.display()
        )));
    }

    let size = part_path
        .metadata()
        .map_err(|err| {
            AppError::runtime(format!(
                "model artifact partial metadata를 읽지 못했습니다: {} ({err})",
                part_path.display()
            ))
        })?
        .len();
    if size > artifact.size_bytes {
        return Err(AppError::blocked(format!(
            "model artifact partial size가 manifest보다 큽니다.\n- expected: {}\n- actual: {}\n- path: {}\n- 다음 단계: rpotato model cleanup-failed <id> --delete 로 app-managed partial을 정리하세요.",
            artifact.size_bytes,
            size,
            part_path.display()
        )));
    }

    Ok(size)
}

fn download_model_artifact_stream(
    artifact: ModelArtifactDescriptor,
    part_path: &Path,
    existing_bytes: u64,
) -> Result<(u64, bool), AppError> {
    let mut request = ureq::get(artifact.url)
        .header("User-Agent", concat!("rpotato/", env!("CARGO_PKG_VERSION")));
    if existing_bytes > 0 {
        request = request.header("Range", &format!("bytes={existing_bytes}-"));
    }

    let response = request.call().map_err(|err| {
        AppError::runtime(format!(
            "model artifact 다운로드 실패\n- url: {}\n- error: {err}",
            artifact.url
        ))
    })?;
    let status_code = response.status().as_u16();
    let (start_offset, resumed) = match (existing_bytes, status_code) {
        (0, 200 | 206) => (0, false),
        (_, 206) => (existing_bytes, true),
        (_, 200) => (0, false),
        (_, status) => {
            return Err(AppError::blocked(format!(
                "model artifact 다운로드 HTTP status가 예상과 다릅니다.\n- url: {}\n- status: {}\n- expected: 200 또는 206",
                artifact.url, status
            )));
        }
    };

    let (_, body) = response.into_parts();
    let mut reader = body.into_reader();
    let mut file: Box<dyn Write> = if start_offset == 0 {
        Box::new(File::create(part_path).map_err(|err| {
            AppError::runtime(format!(
                "model artifact partial file을 만들지 못했습니다: {} ({err})",
                part_path.display()
            ))
        })?)
    } else {
        Box::new(
            OpenOptions::new()
                .append(true)
                .open(part_path)
                .map_err(|err| {
                    AppError::runtime(format!(
                        "model artifact partial file을 append로 열지 못했습니다: {} ({err})",
                        part_path.display()
                    ))
                })?,
        )
    };

    copy_model_reader_with_limit(&mut reader, &mut file, start_offset, artifact.size_bytes)?;
    Ok((start_offset, resumed))
}

fn verify_partial_size(
    part_path: &Path,
    artifact: ModelArtifactDescriptor,
    start_offset: u64,
) -> Result<(), AppError> {
    let actual_bytes = part_path
        .metadata()
        .map_err(|err| {
            AppError::runtime(format!(
                "model artifact partial metadata를 읽지 못했습니다: {} ({err})",
                part_path.display()
            ))
        })?
        .len();
    if actual_bytes != artifact.size_bytes {
        return Err(AppError::blocked(format!(
            "model artifact size 검증 실패\n- expected: {}\n- actual: {}\n- resumed from: {}\n- path: {}\n- 동작: partial은 보존되며 같은 명령으로 재시도하거나 cleanup-failed로 정리할 수 있습니다.",
            artifact.size_bytes,
            actual_bytes,
            start_offset,
            part_path.display()
        )));
    }

    Ok(())
}

fn copy_model_reader_with_limit<R: Read, W: Write + ?Sized>(
    reader: &mut R,
    writer: &mut W,
    existing_bytes: u64,
    expected_total_bytes: u64,
) -> Result<u64, AppError> {
    let mut copied_bytes = 0_u64;
    let mut buffer = [0_u8; DOWNLOAD_BUFFER_BYTES];

    loop {
        let bytes_read = reader
            .read(&mut buffer)
            .map_err(|err| AppError::runtime(format!("model artifact stream read 실패: {err}")))?;
        if bytes_read == 0 {
            break;
        }
        copied_bytes += bytes_read as u64;
        let total_bytes = existing_bytes + copied_bytes;
        if total_bytes > expected_total_bytes {
            return Err(AppError::blocked(format!(
                "model artifact size limit 초과\n- expected: {}\n- actual-at-least: {}",
                expected_total_bytes, total_bytes
            )));
        }
        writer.write_all(&buffer[..bytes_read]).map_err(|err| {
            AppError::runtime(format!("model artifact partial file write 실패: {err}"))
        })?;
    }

    writer
        .flush()
        .map_err(|err| AppError::runtime(format!("model artifact partial flush 실패: {err}")))?;
    Ok(copied_bytes)
}

fn model_artifact_matches(
    artifact: ModelArtifactDescriptor,
    path: &Path,
) -> Result<bool, AppError> {
    let metadata = path.metadata().map_err(|err| {
        AppError::runtime(format!(
            "model artifact metadata를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    if !metadata.is_file() {
        return Err(AppError::blocked(format!(
            "model artifact path가 file이 아닙니다: {}",
            path.display()
        )));
    }
    if metadata.len() != artifact.size_bytes {
        return Ok(false);
    }

    let actual_sha256 = checksum::sha256_file(path)?;
    Ok(actual_sha256.eq_ignore_ascii_case(artifact.sha256))
}

fn verify_model_artifact_file(
    artifact: ModelArtifactDescriptor,
    path: &Path,
) -> Result<(), AppError> {
    let metadata = path.metadata().map_err(|err| {
        AppError::runtime(format!(
            "model artifact metadata를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    if !metadata.is_file() {
        return Err(AppError::blocked(format!(
            "model artifact path가 file이 아닙니다: {}",
            path.display()
        )));
    }
    if metadata.len() != artifact.size_bytes {
        return Err(AppError::blocked(format!(
            "model artifact size 검증 실패\n- expected: {}\n- actual: {}\n- path: {}",
            artifact.size_bytes,
            metadata.len(),
            path.display()
        )));
    }

    let actual_sha256 = checksum::sha256_file(path)?;
    if !actual_sha256.eq_ignore_ascii_case(artifact.sha256) {
        return Err(AppError::blocked(format!(
            "model artifact SHA-256 검증 실패\n- expected: {}\n- actual: {}\n- path: {}\n- 동작: registry 등록은 수행하지 않으며 partial은 cleanup-failed 대상으로 남깁니다.",
            artifact.sha256,
            actual_sha256,
            path.display()
        )));
    }

    Ok(())
}

fn place_verified_artifact(part_path: &Path, final_path: &Path) -> Result<(), AppError> {
    if final_path.exists() {
        return Err(AppError::blocked(format!(
            "model artifact final path가 이미 존재해 partial을 배치하지 않습니다: {}",
            final_path.display()
        )));
    }

    fs::rename(part_path, final_path).map_err(|err| {
        AppError::runtime(format!(
            "model artifact 배치 실패: {} -> {} ({err})",
            part_path.display(),
            final_path.display()
        ))
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
    };

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
