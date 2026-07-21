//! Stable-release comparison and official release-asset naming.

use std::cmp::Ordering;

use crate::foundation::error::AppError;
use crate::foundation::integrity;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ReleaseVersion {
    pub(crate) major: u64,
    pub(crate) minor: u64,
    pub(crate) patch: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AvailableRelease {
    pub(crate) tag: String,
    pub(crate) version: ReleaseVersion,
    pub(crate) release_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UpdateAvailability {
    Available(AvailableRelease),
    Current {
        current: ReleaseVersion,
        latest: ReleaseVersion,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReleaseArchiveKind {
    TarGz,
    Zip,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReleaseAssetPlan {
    pub(crate) target: String,
    pub(crate) archive_name: String,
    pub(crate) checksum_name: String,
    pub(crate) binary_name: String,
    pub(crate) archive_kind: ReleaseArchiveKind,
}

pub(crate) fn classify_update(
    current: &str,
    latest_tag: &str,
    release_url: &str,
) -> Result<UpdateAvailability, AppError> {
    let current = parse_version(current)?;
    let latest = parse_release_tag(latest_tag)?;
    match latest.cmp(&current) {
        Ordering::Greater => Ok(UpdateAvailability::Available(AvailableRelease {
            tag: latest_tag.to_string(),
            version: latest,
            release_url: release_url.to_string(),
        })),
        Ordering::Equal | Ordering::Less => Ok(UpdateAvailability::Current { current, latest }),
    }
}

pub(crate) fn parse_release_tag(tag: &str) -> Result<ReleaseVersion, AppError> {
    let version = tag.strip_prefix('v').ok_or_else(|| {
        AppError::blocked(format!(
            "release tag 형식이 유효하지 않습니다: {tag} (expected vMAJOR.MINOR.PATCH)"
        ))
    })?;
    parse_version(version)
}

pub(crate) fn release_asset_plan(
    tag: &str,
    os: &str,
    arch: &str,
) -> Result<ReleaseAssetPlan, AppError> {
    parse_release_tag(tag)?;
    let (target, archive_kind, extension, binary_name) = match (os, arch) {
        ("macos", "aarch64") => (
            "aarch64-apple-darwin",
            ReleaseArchiveKind::TarGz,
            "tar.gz",
            "rpotato",
        ),
        ("macos", "x86_64") => (
            "x86_64-apple-darwin",
            ReleaseArchiveKind::TarGz,
            "tar.gz",
            "rpotato",
        ),
        ("linux", "aarch64") => (
            "aarch64-unknown-linux-gnu",
            ReleaseArchiveKind::TarGz,
            "tar.gz",
            "rpotato",
        ),
        ("linux", "x86_64") => (
            "x86_64-unknown-linux-gnu",
            ReleaseArchiveKind::TarGz,
            "tar.gz",
            "rpotato",
        ),
        ("windows", "x86_64") => (
            "x86_64-pc-windows-msvc",
            ReleaseArchiveKind::Zip,
            "zip",
            "rpotato.exe",
        ),
        _ => {
            return Err(AppError::blocked(format!(
                "자동 업데이트를 지원하지 않는 platform입니다: {os}/{arch}"
            )))
        }
    };
    let base = format!("rpotato-{tag}-{target}");
    let archive_name = format!("{base}.{extension}");
    Ok(ReleaseAssetPlan {
        target: target.to_string(),
        checksum_name: format!("{archive_name}.sha256"),
        archive_name,
        binary_name: binary_name.to_string(),
        archive_kind,
    })
}

pub(crate) fn parse_checksum_line(body: &str, expected_archive: &str) -> Result<String, AppError> {
    let mut lines = body.lines();
    let line = lines
        .next()
        .ok_or_else(|| AppError::blocked("release checksum 응답이 비어 있습니다."))?;
    if lines.any(|line| !line.trim().is_empty()) {
        return Err(AppError::blocked(
            "release checksum 응답에는 정확히 한 줄만 있어야 합니다.",
        ));
    }
    let mut fields = line.split_whitespace();
    let checksum = fields
        .next()
        .ok_or_else(|| AppError::blocked("release checksum 값이 없습니다."))?;
    let archive = fields
        .next()
        .ok_or_else(|| AppError::blocked("release checksum archive 이름이 없습니다."))?;
    if fields.next().is_some() || archive != expected_archive {
        return Err(AppError::blocked(format!(
            "release checksum archive 이름이 일치하지 않습니다: expected {expected_archive}, found {archive}"
        )));
    }
    if !integrity::is_valid_sha256(checksum) {
        return Err(AppError::blocked(
            "release checksum이 유효한 SHA-256 형식이 아닙니다.",
        ));
    }
    Ok(checksum.to_ascii_lowercase())
}

fn parse_version(value: &str) -> Result<ReleaseVersion, AppError> {
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() != 3
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err(AppError::blocked(format!(
            "version 형식이 유효하지 않습니다: {value} (expected MAJOR.MINOR.PATCH)"
        )));
    }
    let parse = |part: &str| {
        part.parse::<u64>()
            .map_err(|_| AppError::blocked(format!("version 숫자가 너무 큽니다: {value}")))
    };
    Ok(ReleaseVersion {
        major: parse(parts[0])?,
        minor: parse(parts[1])?,
        patch: parse(parts[2])?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_versions_compare_numerically_without_prerelease_guessing() {
        let available = classify_update(
            "0.43.9",
            "v0.44.0",
            "https://github.com/MCprotein/rolling-potato/releases/tag/v0.44.0",
        )
        .unwrap();
        assert!(matches!(available, UpdateAvailability::Available(_)));

        let current = classify_update(
            "0.44.0",
            "v0.43.10",
            "https://github.com/MCprotein/rolling-potato/releases/tag/v0.43.10",
        )
        .unwrap();
        assert!(matches!(current, UpdateAvailability::Current { .. }));

        assert!(parse_release_tag("0.44.0").is_err());
        assert!(parse_release_tag("v0.44.0-beta.1").is_err());
    }

    #[test]
    fn official_asset_plan_covers_exact_release_matrix() {
        let cases = [
            ("macos", "aarch64", "aarch64-apple-darwin", "tar.gz"),
            ("macos", "x86_64", "x86_64-apple-darwin", "tar.gz"),
            ("linux", "aarch64", "aarch64-unknown-linux-gnu", "tar.gz"),
            ("linux", "x86_64", "x86_64-unknown-linux-gnu", "tar.gz"),
            ("windows", "x86_64", "x86_64-pc-windows-msvc", "zip"),
        ];
        for (os, arch, target, extension) in cases {
            let plan = release_asset_plan("v0.44.0", os, arch).unwrap();
            assert_eq!(plan.target, target);
            assert_eq!(
                plan.archive_name,
                format!("rpotato-v0.44.0-{target}.{extension}")
            );
            assert_eq!(plan.checksum_name, format!("{}.sha256", plan.archive_name));
        }
        assert!(release_asset_plan("v0.44.0", "windows", "aarch64").is_err());
    }

    #[test]
    fn checksum_line_binds_hash_to_exact_archive_name() {
        let archive = "rpotato-v0.44.0-aarch64-apple-darwin.tar.gz";
        let hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert_eq!(
            parse_checksum_line(&format!("{hash}  {archive}\n"), archive).unwrap(),
            hash
        );
        assert!(parse_checksum_line(&format!("{hash}  other.tar.gz\n"), archive).is_err());
        assert!(parse_checksum_line(&format!("bad  {archive}\n"), archive).is_err());
    }
}
