use std::env;

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
