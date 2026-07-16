use std::env;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::adapters::filesystem::layout as paths;
use crate::foundation::error::AppError;
use crate::foundation::integrity as checksum;

use super::backend::LLAMA_CPP_BACKEND_ID;

#[derive(Debug, Clone, Copy)]
pub(crate) struct BackendReleaseManifest {
    pub(crate) id: &'static str,
    pub(crate) upstream_source: &'static str,
    pub(crate) license: &'static str,
    pub(crate) license_source: &'static str,
    pub(crate) license_checked_at: &'static str,
    pub(crate) release_tag: &'static str,
    pub(crate) release_url: &'static str,
    pub(crate) release_api_source: &'static str,
    pub(crate) release_checked_at: &'static str,
    pub(crate) artifacts: &'static [BackendReleaseArtifact],
    pub(crate) install_blockers: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BackendReleaseArtifact {
    pub(crate) os: &'static str,
    pub(crate) arch: &'static str,
    pub(crate) archive_name: &'static str,
    pub(crate) archive_url: &'static str,
    pub(crate) archive_sha256: &'static str,
    pub(crate) archive_size_bytes: u64,
    pub(crate) archive_kind: BackendArchiveKind,
    pub(crate) binary_relative_path: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackendArchiveKind {
    TarGz,
    Zip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArchiveDownloadStatus {
    Downloaded,
    CacheHit,
}

impl ArchiveDownloadStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Downloaded => "downloaded",
            Self::CacheHit => "cache-hit",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InstalledPayload {
    pub(crate) archive_path: PathBuf,
    pub(crate) extracted_binary: PathBuf,
    pub(crate) managed_binary: PathBuf,
    pub(crate) binary_sha256: String,
}

impl BackendArchiveKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::TarGz => "tar.gz",
            Self::Zip => "zip",
        }
    }
}

pub(crate) const LLAMA_CPP_RELEASE: BackendReleaseManifest = BackendReleaseManifest {
    id: LLAMA_CPP_BACKEND_ID,
    upstream_source: "https://github.com/ggml-org/llama.cpp",
    license: "MIT",
    license_source: "https://github.com/ggml-org/llama.cpp/blob/b9982/LICENSE",
    license_checked_at: "2026-07-13",
    release_tag: "b9982",
    release_url: "https://github.com/ggml-org/llama.cpp/releases/tag/b9982",
    release_api_source: "https://api.github.com/repos/ggml-org/llama.cpp/releases/latest",
    release_checked_at: "2026-07-13",
    artifacts: &LLAMA_CPP_RELEASE_ARTIFACTS,
    install_blockers: &[],
};

const LLAMA_CPP_RELEASE_ARTIFACTS: [BackendReleaseArtifact; 6] = [
    BackendReleaseArtifact {
        os: "macos",
        arch: "aarch64",
        archive_name: "llama-b9982-bin-macos-arm64.tar.gz",
        archive_url: "https://github.com/ggml-org/llama.cpp/releases/download/b9982/llama-b9982-bin-macos-arm64.tar.gz",
        archive_sha256: "9606e3a609bc9483730f50f17ce78c3d764df8eaec63fcbb47d2f8b235667c9c",
        archive_size_bytes: 10_746_432,
        archive_kind: BackendArchiveKind::TarGz,
        binary_relative_path: "llama-server",
    },
    BackendReleaseArtifact {
        os: "macos",
        arch: "x86_64",
        archive_name: "llama-b9982-bin-macos-x64.tar.gz",
        archive_url: "https://github.com/ggml-org/llama.cpp/releases/download/b9982/llama-b9982-bin-macos-x64.tar.gz",
        archive_sha256: "da109cc18574392ab88936de826ca00f8d196b9ef5a1c19da72fbfb06bea7cd0",
        archive_size_bytes: 11_022_427,
        archive_kind: BackendArchiveKind::TarGz,
        binary_relative_path: "llama-server",
    },
    BackendReleaseArtifact {
        os: "linux",
        arch: "aarch64",
        archive_name: "llama-b9982-bin-ubuntu-arm64.tar.gz",
        archive_url: "https://github.com/ggml-org/llama.cpp/releases/download/b9982/llama-b9982-bin-ubuntu-arm64.tar.gz",
        archive_sha256: "9468c0282c15e286216a63122e7471f7d14888d3858bdab61b72d14a2531cf60",
        archive_size_bytes: 12_782_598,
        archive_kind: BackendArchiveKind::TarGz,
        binary_relative_path: "llama-server",
    },
    BackendReleaseArtifact {
        os: "linux",
        arch: "x86_64",
        archive_name: "llama-b9982-bin-ubuntu-x64.tar.gz",
        archive_url: "https://github.com/ggml-org/llama.cpp/releases/download/b9982/llama-b9982-bin-ubuntu-x64.tar.gz",
        archive_sha256: "0c1f0445f6f86a0f049de3586b7eabdde7108d827d0a9b2c5c0dc2185506ffee",
        archive_size_bytes: 15_850_588,
        archive_kind: BackendArchiveKind::TarGz,
        binary_relative_path: "llama-server",
    },
    BackendReleaseArtifact {
        os: "windows",
        arch: "aarch64",
        archive_name: "llama-b9982-bin-win-cpu-arm64.zip",
        archive_url: "https://github.com/ggml-org/llama.cpp/releases/download/b9982/llama-b9982-bin-win-cpu-arm64.zip",
        archive_sha256: "11ad20d8df121d5760900b4e2fa9943a065856075ef44df52ed7a8dc58b08b2f",
        archive_size_bytes: 12_151_247,
        archive_kind: BackendArchiveKind::Zip,
        binary_relative_path: "llama-server.exe",
    },
    BackendReleaseArtifact {
        os: "windows",
        arch: "x86_64",
        archive_name: "llama-b9982-bin-win-cpu-x64.zip",
        archive_url: "https://github.com/ggml-org/llama.cpp/releases/download/b9982/llama-b9982-bin-win-cpu-x64.zip",
        archive_sha256: "69337038e8e56feb3c04d99588fa19f9241b294bae6f6c2e665a301605726e2a",
        archive_size_bytes: 18_247_652,
        archive_kind: BackendArchiveKind::Zip,
        binary_relative_path: "llama-server.exe",
    },
];

pub(crate) fn selected_release_artifact(
    manifest: &BackendReleaseManifest,
) -> Option<&'static BackendReleaseArtifact> {
    release_artifact_for(manifest, env::consts::OS, env::consts::ARCH)
}

pub(crate) fn release_artifact_for(
    manifest: &BackendReleaseManifest,
    os: &str,
    arch: &str,
) -> Option<&'static BackendReleaseArtifact> {
    manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.os == os && artifact.arch == arch)
}

pub(crate) fn install_blockers(
    manifest: &BackendReleaseManifest,
    artifact: Option<&BackendReleaseArtifact>,
) -> Vec<String> {
    let mut blockers = Vec::new();
    for blocker in manifest.install_blockers {
        push_unique(&mut blockers, *blocker);
    }
    if manifest.release_url.is_empty() {
        push_unique(&mut blockers, "release URL 미확정");
    }
    if manifest.release_api_source.is_empty() {
        push_unique(&mut blockers, "release API source 미확정");
    }
    if manifest.release_tag.is_empty() {
        push_unique(&mut blockers, "release tag 미확정");
    }
    let Some(artifact) = artifact else {
        push_unique(
            &mut blockers,
            format!(
                "지원 platform artifact 미확정 ({}/{})",
                env::consts::OS,
                env::consts::ARCH
            ),
        );
        return blockers;
    };
    if artifact.archive_url.is_empty() {
        push_unique(&mut blockers, "archive URL 미확정");
    }
    if artifact.archive_name.is_empty() {
        push_unique(&mut blockers, "archive name 미확정");
    }
    if !checksum::is_valid_sha256(artifact.archive_sha256) {
        push_unique(&mut blockers, "archive SHA-256 미확정");
    }
    if artifact.archive_size_bytes == 0 {
        push_unique(&mut blockers, "archive file size 미확정");
    }
    if artifact.binary_relative_path.is_empty() {
        push_unique(&mut blockers, "archive 내부 binary path 미확정");
    }
    blockers
}

pub(crate) fn archive_path(artifact: &BackendReleaseArtifact) -> PathBuf {
    paths::downloads_dir().join(artifact.archive_name)
}

pub(crate) fn staging_dir(
    manifest: &BackendReleaseManifest,
    artifact: &BackendReleaseArtifact,
) -> PathBuf {
    paths::backends_dir().join("llama.cpp").join(format!(
        ".staging-{}-{}-{}",
        manifest.release_tag, artifact.os, artifact.arch
    ))
}

pub(crate) fn download_archive(
    artifact: &BackendReleaseArtifact,
    archive_path: &Path,
) -> Result<ArchiveDownloadStatus, AppError> {
    if archive_path.exists() && !archive_path.is_file() {
        return Err(AppError::blocked(format!(
            "backend archive cache path가 file이 아닙니다: {}",
            archive_path.display()
        )));
    }
    if archive_path.is_file() && verify_archive_file(artifact, archive_path).is_ok() {
        return Ok(ArchiveDownloadStatus::CacheHit);
    }

    let parent = archive_path.parent().ok_or_else(|| {
        AppError::runtime(format!(
            "backend archive parent path를 계산하지 못했습니다: {}",
            archive_path.display()
        ))
    })?;
    fs::create_dir_all(parent).map_err(|err| {
        AppError::runtime(format!(
            "backend archive download directory를 만들지 못했습니다: {} ({err})",
            parent.display()
        ))
    })?;

    let part_path = archive_path.with_file_name(format!("{}.part", artifact.archive_name));
    remove_file_if_exists(&part_path)?;

    let response = ureq::get(artifact.archive_url)
        .header("User-Agent", concat!("rpotato/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|err| {
            AppError::runtime(format!(
                "backend archive 다운로드 실패\n- url: {}\n- error: {err}",
                artifact.archive_url
            ))
        })?;
    let (_, body) = response.into_parts();
    let mut reader = body.into_reader();
    let mut file = File::create(&part_path).map_err(|err| {
        AppError::runtime(format!(
            "backend archive partial file을 만들지 못했습니다: {} ({err})",
            part_path.display()
        ))
    })?;

    let copied_bytes =
        match copy_reader_with_limit(&mut reader, &mut file, artifact.archive_size_bytes) {
            Ok(copied_bytes) => copied_bytes,
            Err(err) => {
                drop(file);
                let _ = fs::remove_file(&part_path);
                return Err(err);
            }
        };
    file.sync_all().map_err(|err| {
        AppError::runtime(format!(
            "backend archive partial file sync 실패: {} ({err})",
            part_path.display()
        ))
    })?;
    drop(file);

    if copied_bytes != artifact.archive_size_bytes {
        remove_file_if_exists(&part_path)?;
        return Err(AppError::blocked(format!(
            "backend archive size 검증 실패\n- expected: {}\n- actual: {}\n- path: {}",
            artifact.archive_size_bytes,
            copied_bytes,
            part_path.display()
        )));
    }
    if let Err(err) = verify_archive_file(artifact, &part_path) {
        remove_file_if_exists(&part_path)?;
        return Err(err);
    }

    remove_file_if_exists(archive_path)?;
    fs::rename(&part_path, archive_path).map_err(|err| {
        AppError::runtime(format!(
            "backend archive cache 배치 실패: {} -> {} ({err})",
            part_path.display(),
            archive_path.display()
        ))
    })?;

    Ok(ArchiveDownloadStatus::Downloaded)
}

pub(crate) fn verify_archive_file(
    artifact: &BackendReleaseArtifact,
    archive_path: &Path,
) -> Result<(), AppError> {
    let metadata = archive_path.metadata().map_err(|err| {
        AppError::runtime(format!(
            "backend archive metadata를 읽지 못했습니다: {} ({err})",
            archive_path.display()
        ))
    })?;
    if !metadata.is_file() {
        return Err(AppError::blocked(format!(
            "backend archive path가 file이 아닙니다: {}",
            archive_path.display()
        )));
    }
    if metadata.len() != artifact.archive_size_bytes {
        return Err(AppError::blocked(format!(
            "backend archive size 검증 실패\n- expected: {}\n- actual: {}\n- path: {}",
            artifact.archive_size_bytes,
            metadata.len(),
            archive_path.display()
        )));
    }

    let actual_sha256 = checksum::sha256_file(archive_path)?;
    if !actual_sha256.eq_ignore_ascii_case(artifact.archive_sha256) {
        return Err(AppError::blocked(format!(
            "backend archive SHA-256 검증 실패\n- expected: {}\n- actual: {}\n- path: {}",
            artifact.archive_sha256,
            actual_sha256,
            archive_path.display()
        )));
    }

    Ok(())
}

fn copy_reader_with_limit<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    expected_bytes: u64,
) -> Result<u64, AppError> {
    const DOWNLOAD_BUFFER_BYTES: usize = 64 * 1024;

    let mut copied_bytes = 0_u64;
    let mut buffer = [0_u8; DOWNLOAD_BUFFER_BYTES];
    loop {
        let bytes_read = reader.read(&mut buffer).map_err(|err| {
            AppError::runtime(format!("backend archive download stream read 실패: {err}"))
        })?;
        if bytes_read == 0 {
            break;
        }
        copied_bytes += bytes_read as u64;
        if copied_bytes > expected_bytes {
            return Err(AppError::blocked(format!(
                "backend archive size limit 초과\n- expected: {}\n- actual-at-least: {}",
                expected_bytes, copied_bytes
            )));
        }
        writer.write_all(&buffer[..bytes_read]).map_err(|err| {
            AppError::runtime(format!("backend archive partial file write 실패: {err}"))
        })?;
    }
    writer
        .flush()
        .map_err(|err| AppError::runtime(format!("backend archive flush 실패: {err}")))?;
    Ok(copied_bytes)
}

pub(crate) fn prepare_install(
    artifact: &BackendReleaseArtifact,
    archive_path: &Path,
    managed_binary: &Path,
    staging_dir: &Path,
) -> Result<InstalledPayload, AppError> {
    remove_dir_if_exists(staging_dir)?;
    fs::create_dir_all(staging_dir).map_err(|err| {
        AppError::runtime(format!(
            "backend staging directory를 만들지 못했습니다: {} ({err})",
            staging_dir.display()
        ))
    })?;

    if let Err(err) = extract_archive(artifact, archive_path, staging_dir) {
        let _ = fs::remove_dir_all(staging_dir);
        return Err(err);
    }
    let extracted_binary = match find_extracted_binary(artifact, staging_dir) {
        Ok(path) => path,
        Err(err) => {
            let _ = fs::remove_dir_all(staging_dir);
            return Err(err);
        }
    };
    if let Err(err) = place_managed_payload(&extracted_binary, staging_dir, managed_binary) {
        let _ = fs::remove_dir_all(staging_dir);
        return Err(err);
    }
    let binary_sha256 = checksum::sha256_file(managed_binary)?;

    Ok(InstalledPayload {
        archive_path: archive_path.to_path_buf(),
        extracted_binary,
        managed_binary: managed_binary.to_path_buf(),
        binary_sha256,
    })
}

pub(crate) fn cleanup_staging(staging_dir: &Path) -> Result<(), AppError> {
    remove_dir_if_exists(staging_dir)
}

fn extract_archive(
    artifact: &BackendReleaseArtifact,
    archive_path: &Path,
    staging_dir: &Path,
) -> Result<(), AppError> {
    match artifact.archive_kind {
        BackendArchiveKind::TarGz => extract_tar_gz_archive(archive_path, staging_dir),
        BackendArchiveKind::Zip => extract_zip_archive(archive_path, staging_dir),
    }
}

fn extract_tar_gz_archive(archive_path: &Path, staging_dir: &Path) -> Result<(), AppError> {
    let file = File::open(archive_path).map_err(|err| {
        AppError::runtime(format!(
            "backend tar.gz archive를 열지 못했습니다: {} ({err})",
            archive_path.display()
        ))
    })?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive.unpack(staging_dir).map_err(|err| {
        AppError::runtime(format!(
            "backend tar.gz archive extraction 실패: {} -> {} ({err})",
            archive_path.display(),
            staging_dir.display()
        ))
    })
}

fn extract_zip_archive(archive_path: &Path, staging_dir: &Path) -> Result<(), AppError> {
    let file = File::open(archive_path).map_err(|err| {
        AppError::runtime(format!(
            "backend zip archive를 열지 못했습니다: {} ({err})",
            archive_path.display()
        ))
    })?;
    let mut archive = zip::ZipArchive::new(file).map_err(|err| {
        AppError::runtime(format!(
            "backend zip archive metadata를 읽지 못했습니다: {} ({err})",
            archive_path.display()
        ))
    })?;
    archive.extract(staging_dir).map_err(|err| {
        AppError::runtime(format!(
            "backend zip archive extraction 실패: {} -> {} ({err})",
            archive_path.display(),
            staging_dir.display()
        ))
    })
}

fn find_extracted_binary(
    artifact: &BackendReleaseArtifact,
    staging_dir: &Path,
) -> Result<PathBuf, AppError> {
    let hinted_path = staging_dir.join(artifact.binary_relative_path);
    if is_regular_file_no_symlink(&hinted_path) {
        return Ok(hinted_path);
    }

    let binary_name = Path::new(artifact.binary_relative_path)
        .file_name()
        .ok_or_else(|| {
            AppError::blocked(format!(
                "archive 내부 binary path가 유효하지 않습니다: {}",
                artifact.binary_relative_path
            ))
        })?;
    let mut matches = Vec::new();
    collect_binary_matches(staging_dir, binary_name, &mut matches)?;
    matches.sort();

    match matches.len() {
        0 => Err(AppError::blocked(format!(
            "backend archive에서 binary를 찾지 못했습니다\n- expected: {}\n- staging: {}",
            artifact.binary_relative_path,
            staging_dir.display()
        ))),
        1 => Ok(matches.remove(0)),
        _ => Err(AppError::blocked(format!(
            "backend archive에서 binary 후보가 여러 개입니다\n- expected: {}\n- count: {}\n- staging: {}",
            artifact.binary_relative_path,
            matches.len(),
            staging_dir.display()
        ))),
    }
}

fn collect_binary_matches(
    directory: &Path,
    binary_name: &std::ffi::OsStr,
    matches: &mut Vec<PathBuf>,
) -> Result<(), AppError> {
    for entry in fs::read_dir(directory).map_err(|err| {
        AppError::runtime(format!(
            "backend extraction directory를 읽지 못했습니다: {} ({err})",
            directory.display()
        ))
    })? {
        let entry =
            entry.map_err(|err| AppError::runtime(format!("directory entry read 실패: {err}")))?;
        let path = entry.path();
        let file_type = fs::symlink_metadata(&path)
            .map_err(|err| {
                AppError::runtime(format!(
                    "backend extracted path metadata를 읽지 못했습니다: {} ({err})",
                    path.display()
                ))
            })?
            .file_type();
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_binary_matches(&path, binary_name, matches)?;
        } else if file_type.is_file() && path.file_name() == Some(binary_name) {
            matches.push(path);
        }
    }
    Ok(())
}

fn is_regular_file_no_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_file() && !metadata.file_type().is_symlink())
        .unwrap_or(false)
}

fn place_managed_payload(
    extracted_binary: &Path,
    staging_dir: &Path,
    managed_binary: &Path,
) -> Result<(), AppError> {
    let parent = managed_binary.parent().ok_or_else(|| {
        AppError::runtime(format!(
            "managed backend binary parent path를 계산하지 못했습니다: {}",
            managed_binary.display()
        ))
    })?;
    fs::create_dir_all(parent).map_err(|err| {
        AppError::runtime(format!(
            "managed backend directory를 만들지 못했습니다: {} ({err})",
            parent.display()
        ))
    })?;

    if managed_binary.exists() && !managed_binary.is_file() {
        return Err(AppError::blocked(format!(
            "managed backend path가 file이 아닙니다: {}",
            managed_binary.display()
        )));
    }
    if parent.exists() && !parent.is_dir() {
        return Err(AppError::blocked(format!(
            "managed backend directory path가 directory가 아닙니다: {}",
            parent.display()
        )));
    }

    let file_name = managed_binary
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("llama-server");
    let next_dir = parent.with_file_name("llama.cpp.next");
    let backup_dir = parent.with_file_name("llama.cpp.previous");
    remove_dir_if_exists(&next_dir)?;
    remove_dir_if_exists(&backup_dir)?;

    let payload_root = payload_root_for(extracted_binary, staging_dir)?;
    copy_release_tree(&payload_root, &next_dir)?;
    let next_binary = next_dir.join(file_name);
    if !next_binary.is_file() {
        fs::copy(extracted_binary, &next_binary).map_err(|err| {
            AppError::runtime(format!(
                "managed backend binary copy 실패: {} -> {} ({err})",
                extracted_binary.display(),
                next_binary.display()
            ))
        })?;
    }
    set_executable_bit(&next_binary)?;

    let had_existing = parent.exists();
    if had_existing {
        fs::rename(parent, &backup_dir).map_err(|err| {
            AppError::runtime(format!(
                "기존 managed backend directory backup 실패: {} -> {} ({err})",
                parent.display(),
                backup_dir.display()
            ))
        })?;
    }

    if let Err(err) = fs::rename(&next_dir, parent) {
        if had_existing && backup_dir.is_dir() {
            let _ = fs::rename(&backup_dir, parent);
        }
        let _ = fs::remove_dir_all(&next_dir);
        return Err(AppError::runtime(format!(
            "managed backend directory 배치 실패: {} -> {} ({err})",
            next_dir.display(),
            parent.display()
        )));
    }
    remove_dir_if_exists(&backup_dir)?;
    Ok(())
}

fn payload_root_for(extracted_binary: &Path, staging_dir: &Path) -> Result<PathBuf, AppError> {
    if !extracted_binary.starts_with(staging_dir) {
        return Err(AppError::runtime(format!(
            "extracted backend binary relative path 계산 실패: {} under {}",
            extracted_binary.display(),
            staging_dir.display()
        )));
    }
    extracted_binary
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            AppError::runtime(format!(
                "extracted backend binary parent path를 계산하지 못했습니다: {}",
                extracted_binary.display()
            ))
        })
}

fn copy_release_tree(source: &Path, destination: &Path) -> Result<(), AppError> {
    fs::create_dir_all(destination).map_err(|err| {
        AppError::runtime(format!(
            "managed backend payload directory를 만들지 못했습니다: {} ({err})",
            destination.display()
        ))
    })?;
    for entry in fs::read_dir(source).map_err(|err| {
        AppError::runtime(format!(
            "backend payload source directory를 읽지 못했습니다: {} ({err})",
            source.display()
        ))
    })? {
        let entry = entry
            .map_err(|err| AppError::runtime(format!("backend payload entry read 실패: {err}")))?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = fs::symlink_metadata(&source_path)
            .map_err(|err| {
                AppError::runtime(format!(
                    "backend payload metadata를 읽지 못했습니다: {} ({err})",
                    source_path.display()
                ))
            })?
            .file_type();

        if file_type.is_dir() {
            copy_release_tree(&source_path, &destination_path)?;
        } else if file_type.is_file() || file_type.is_symlink() {
            fs::copy(&source_path, &destination_path).map_err(|err| {
                AppError::runtime(format!(
                    "backend payload file copy 실패: {} -> {} ({err})",
                    source_path.display(),
                    destination_path.display()
                ))
            })?;
        }
    }
    Ok(())
}

#[cfg(unix)]
pub(crate) fn set_executable_bit(path: &Path) -> Result<(), AppError> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = path
        .metadata()
        .map_err(|err| {
            AppError::runtime(format!(
                "managed backend binary metadata를 읽지 못했습니다: {} ({err})",
                path.display()
            ))
        })?
        .permissions();
    permissions.set_mode(permissions.mode() | 0o755);
    fs::set_permissions(path, permissions).map_err(|err| {
        AppError::runtime(format!(
            "managed backend binary 실행 권한 설정 실패: {} ({err})",
            path.display()
        ))
    })
}

#[cfg(not(unix))]
pub(crate) fn set_executable_bit(_path: &Path) -> Result<(), AppError> {
    Ok(())
}

fn remove_file_if_exists(path: &Path) -> Result<(), AppError> {
    if path.exists() {
        fs::remove_file(path).map_err(|err| {
            AppError::runtime(format!("file 삭제 실패: {} ({err})", path.display()))
        })?;
    }
    Ok(())
}

fn remove_dir_if_exists(path: &Path) -> Result<(), AppError> {
    if path.exists() {
        fs::remove_dir_all(path).map_err(|err| {
            AppError::runtime(format!("directory 삭제 실패: {} ({err})", path.display()))
        })?;
    }
    Ok(())
}

fn push_unique(values: &mut Vec<String>, value: impl Into<String>) {
    let value = value.into();
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_manifest_records_supported_artifacts() {
        assert_eq!(LLAMA_CPP_RELEASE.artifacts.len(), 6);
        assert!(LLAMA_CPP_RELEASE.artifacts.iter().all(|artifact| {
            checksum::is_valid_sha256(artifact.archive_sha256)
                && artifact.archive_size_bytes > 0
                && artifact.archive_url.ends_with(artifact.archive_name)
                && install_blockers(&LLAMA_CPP_RELEASE, Some(artifact)).is_empty()
        }));
    }

    #[test]
    fn selection_rejects_an_unknown_platform() {
        assert!(release_artifact_for(&LLAMA_CPP_RELEASE, "freebsd", "riscv64").is_none());
    }
}
