use std::env;
use std::fs;
use std::path::PathBuf;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BackendInstallRecord {
    pub(crate) release_tag: String,
    pub(crate) archive_sha256: String,
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

mod archive;
mod payload;

pub(crate) use archive::{download_archive, verify_archive_file};
#[cfg(test)]
pub(crate) use payload::set_executable_bit;
pub(crate) use payload::{cleanup_staging, prepare_install};

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

pub(crate) fn write_install_record(
    artifact: &BackendReleaseArtifact,
    binary_sha256: &str,
) -> Result<(), AppError> {
    let path = install_record_path();
    let parent = path.parent().ok_or_else(|| {
        AppError::runtime(format!(
            "backend install record parent path를 계산하지 못했습니다: {}",
            path.display()
        ))
    })?;
    fs::create_dir_all(parent).map_err(|err| {
        AppError::runtime(format!(
            "backend install record directory를 만들지 못했습니다: {} ({err})",
            parent.display()
        ))
    })?;

    let contents = format!(
        "release_tag={}\narchive_sha256={}\nbinary_sha256={}\n",
        LLAMA_CPP_RELEASE.release_tag, artifact.archive_sha256, binary_sha256
    );
    fs::write(&path, contents).map_err(|err| {
        AppError::runtime(format!(
            "backend install record를 쓰지 못했습니다: {} ({err})",
            path.display()
        ))
    })
}

pub(crate) fn read_install_record() -> Result<BackendInstallRecord, AppError> {
    let path = install_record_path();
    let contents = fs::read_to_string(&path).map_err(|err| {
        AppError::runtime(format!(
            "backend install record를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    parse_install_record(&contents).ok_or_else(|| {
        AppError::blocked(format!(
            "backend install record 형식이 유효하지 않습니다: {}",
            path.display()
        ))
    })
}

fn install_record_path() -> PathBuf {
    paths::backends_dir()
        .join("llama.cpp")
        .join("install-record.txt")
}

fn parse_install_record(contents: &str) -> Option<BackendInstallRecord> {
    let mut release_tag = None;
    let mut archive_sha256 = None;
    let mut binary_sha256 = None;

    for line in contents.lines() {
        let (key, value) = line.split_once('=')?;
        match key {
            "release_tag" => release_tag = Some(value.to_string()),
            "archive_sha256" => archive_sha256 = Some(value.to_string()),
            "binary_sha256" => binary_sha256 = Some(value.to_string()),
            _ => {}
        }
    }

    Some(BackendInstallRecord {
        release_tag: release_tag?,
        archive_sha256: archive_sha256?,
        binary_sha256: binary_sha256?,
    })
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
    use std::time::{SystemTime, UNIX_EPOCH};

    const TEST_ARCHIVE: BackendReleaseArtifact = BackendReleaseArtifact {
        os: "test-os",
        arch: "test-arch",
        archive_name: "archive.bin",
        archive_url: "https://invalid.example/archive.bin",
        archive_sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        archive_size_bytes: 3,
        archive_kind: BackendArchiveKind::Zip,
        binary_relative_path: "llama-server",
    };

    #[test]
    fn verified_archive_cache_is_reused_and_tamper_is_rejected() {
        let root = std::env::temp_dir().join(format!(
            "rpotato-llama-archive-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let archive = root.join(TEST_ARCHIVE.archive_name);
        fs::write(&archive, b"abc").unwrap();

        assert_eq!(
            download_archive(&TEST_ARCHIVE, &archive).unwrap(),
            ArchiveDownloadStatus::CacheHit
        );

        fs::write(&archive, b"abcd").unwrap();
        let error = verify_archive_file(&TEST_ARCHIVE, &archive).unwrap_err();
        assert!(error.message.contains("archive size"));

        let _ = fs::remove_dir_all(root);
    }

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
