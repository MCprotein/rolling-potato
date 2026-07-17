use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BackendInstallResult {
    pub(super) download_status: ArchiveDownloadStatus,
    pub(super) archive_path: PathBuf,
    pub(super) extracted_binary: PathBuf,
    pub(super) managed_binary: PathBuf,
    binary_sha256: String,
    pub(super) ledger_event: String,
}

pub fn install_plan_report() -> String {
    let discovery = llama_backend::discover();
    let artifact = selected_backend_release_artifact(&LLAMA_CPP_RELEASE);
    let blockers = backend_install_blockers(&LLAMA_CPP_RELEASE, artifact);
    let install_status = if blockers.is_empty() {
        "ready"
    } else {
        "blocked"
    };
    let archive_name = artifact
        .map(|artifact| artifact.archive_name)
        .unwrap_or("미확정");
    let download_path = paths::downloads_dir().join(if archive_name == "미확정" {
        "llama.cpp.archive.part"
    } else {
        archive_name
    });

    format!(
        "backend install plan\n- id: {}\n- status: {}\n- upstream source: {}\n- license: {}\n- license source: {}\n- license checked-at: {}\n- release tag: {}\n- release URL: {}\n- release API source: {}\n- release checked-at: {}\n- platform: {}/{}\n- archive URL: {}\n- archive name: {}\n- archive kind: {}\n- archive size bytes: {}\n- archive sha256: {}\n- binary in archive: {}\n- managed binary: {}\n- selected binary: {}\n- selected source: {}\n- download path: {}\n- blockers: {}\n- 동작: 실제 backend 다운로드 전 release URL, checksum, size, license를 사용자에게 표시해야 합니다.",
        LLAMA_CPP_RELEASE.id,
        install_status,
        LLAMA_CPP_RELEASE.upstream_source,
        LLAMA_CPP_RELEASE.license,
        LLAMA_CPP_RELEASE.license_source,
        LLAMA_CPP_RELEASE.license_checked_at,
        LLAMA_CPP_RELEASE.release_tag,
        LLAMA_CPP_RELEASE.release_url,
        LLAMA_CPP_RELEASE.release_api_source,
        LLAMA_CPP_RELEASE.release_checked_at,
        env::consts::OS,
        env::consts::ARCH,
        artifact
            .map(|artifact| artifact.archive_url)
            .unwrap_or("미확정"),
        archive_name,
        artifact
            .map(|artifact| artifact.archive_kind.as_str())
            .unwrap_or("미확정"),
        artifact
            .map(|artifact| artifact.archive_size_bytes)
            .map(|value| value.to_string())
            .unwrap_or_else(|| "미확정".to_string()),
        artifact
            .map(|artifact| artifact.archive_sha256)
            .unwrap_or("미확정"),
        artifact
            .map(|artifact| artifact.binary_relative_path)
            .unwrap_or("미확정"),
        discovery.managed_path.display(),
        discovery.selected_path.display(),
        discovery.selected_source,
        download_path.display(),
        display_vec(&blockers)
    )
}

pub fn install_report() -> Result<String, AppError> {
    let artifact = selected_backend_release_artifact(&LLAMA_CPP_RELEASE).ok_or_else(|| {
        AppError::blocked(format!(
            "backend install 차단\n- 이유: 지원 platform artifact 미확정 ({}/{})\n- 다음 단계: backend install-plan으로 현재 platform 상태를 확인하세요.",
            env::consts::OS,
            env::consts::ARCH
        ))
    })?;
    let blockers = backend_install_blockers(&LLAMA_CPP_RELEASE, Some(artifact));
    if !blockers.is_empty() {
        return Err(AppError::blocked(format!(
            "backend install 차단\n- blockers: {}\n- 다음 단계: backend install-plan으로 release URL, checksum, size, license source를 확인하세요.",
            display_vec(&blockers)
        )));
    }

    let archive_path = llama_install::archive_path(artifact);
    let download_status = llama_install::download_archive(artifact, &archive_path)?;
    llama_install::verify_archive_file(artifact, &archive_path)?;

    let managed_binary = LlamaCppAdapter.managed_binary_path();
    let staging_dir = llama_install::staging_dir(&LLAMA_CPP_RELEASE, artifact);
    let result = install_backend_from_archive(
        artifact,
        &archive_path,
        &managed_binary,
        &staging_dir,
        download_status,
    )?;

    Ok(format!(
        "backend install 완료\n- id: {}\n- release tag: {}\n- archive: {}\n- archive sha256: {}\n- archive source: {}\n- download status: {}\n- extracted binary: {}\n- managed binary: {}\n- managed binary sha256: {}\n- ledger event: {}\n- 다음 단계: rpotato backend doctor 또는 rpotato backend health-check로 상태를 확인하세요.",
        LLAMA_CPP_RELEASE.id,
        LLAMA_CPP_RELEASE.release_tag,
        result.archive_path.display(),
        artifact.archive_sha256,
        artifact.archive_url,
        result.download_status.as_str(),
        result.extracted_binary.display(),
        result.managed_binary.display(),
        result.binary_sha256,
        result.ledger_event
    ))
}

pub fn verify_archive_report(path: &str, expected_sha256: &str) -> Result<String, AppError> {
    if !checksum::is_valid_sha256(expected_sha256) {
        return Err(AppError::usage(
            "expected SHA-256은 64자리 hex string이어야 합니다.",
        ));
    }

    let path = PathBuf::from(path);
    if !path.is_file() {
        return Err(AppError::usage(format!(
            "검증 대상 backend archive를 찾지 못했습니다: {}",
            path.display()
        )));
    }

    let actual_sha256 = checksum::sha256_file(&path)?;
    let matched = actual_sha256.eq_ignore_ascii_case(expected_sha256);
    let event_id = state::record_event(
        if matched {
            "backend.archive.sha256.verified"
        } else {
            "backend.archive.sha256.rejected"
        },
        if matched {
            "backend archive SHA-256 검증 성공"
        } else {
            "backend archive SHA-256 검증 실패"
        },
        &format!(
            "path={} expected_sha256={} actual_sha256={}",
            path.display(),
            expected_sha256,
            actual_sha256
        ),
    )?;

    if !matched {
        return Err(AppError::blocked(format!(
            "backend archive SHA-256 검증 실패\n- path: {}\n- expected: {}\n- actual: {}\n- ledger event: {}\n- 동작: backend install과 extraction을 차단해야 합니다.",
            path.display(),
            expected_sha256,
            actual_sha256,
            event_id
        )));
    }

    Ok(format!(
        "backend archive SHA-256 검증 성공\n- path: {}\n- expected: {}\n- actual: {}\n- ledger event: {}",
        path.display(),
        expected_sha256,
        actual_sha256,
        event_id
    ))
}

pub(super) fn install_backend_from_archive(
    artifact: &BackendReleaseArtifact,
    archive_path: &Path,
    managed_binary: &Path,
    staging_dir: &Path,
    download_status: ArchiveDownloadStatus,
) -> Result<BackendInstallResult, AppError> {
    let payload =
        llama_install::prepare_install(artifact, archive_path, managed_binary, staging_dir)?;
    llama_install::write_install_record(artifact, &payload.binary_sha256)?;
    llama_install::cleanup_staging(staging_dir)?;

    let event_id = state::record_event(
        "backend.install.completed",
        "llama.cpp backend 설치 완료",
        &format!(
            "release_tag={} archive={} sha256={} managed_binary={} binary_sha256={} download_status={}",
            LLAMA_CPP_RELEASE.release_tag,
            archive_path.display(),
            artifact.archive_sha256,
            managed_binary.display(),
            payload.binary_sha256,
            download_status.as_str()
        ),
    )?;

    Ok(BackendInstallResult {
        download_status,
        archive_path: payload.archive_path,
        extracted_binary: payload.extracted_binary,
        managed_binary: payload.managed_binary,
        binary_sha256: payload.binary_sha256,
        ledger_event: event_id,
    })
}
