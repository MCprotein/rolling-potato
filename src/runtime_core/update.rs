//! Stable-release comparison and official release-asset naming.

use std::cmp::Ordering;

use crate::foundation::error::AppError;
use crate::foundation::integrity;

const RELEASE_TARGET_MANIFEST: &str = include_str!("../../config/release-targets.tsv");

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
    let (current, current_is_prerelease) = parse_current_version(current)?;
    let latest = parse_release_tag(latest_tag)?;
    match latest.cmp(&current) {
        Ordering::Greater => Ok(UpdateAvailability::Available(AvailableRelease {
            tag: latest_tag.to_string(),
            version: latest,
            release_url: release_url.to_string(),
        })),
        Ordering::Equal if current_is_prerelease => {
            Ok(UpdateAvailability::Available(AvailableRelease {
                tag: latest_tag.to_string(),
                version: latest,
                release_url: release_url.to_string(),
            }))
        }
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
    let Some((target, archive_kind, extension, binary_name)) = release_target(os, arch)? else {
        return Err(AppError::blocked(format!(
            "자동 업데이트를 지원하지 않는 platform입니다: {os}/{arch}"
        )));
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

fn release_target(
    os: &str,
    arch: &str,
) -> Result<Option<(&'static str, ReleaseArchiveKind, &'static str, &'static str)>, AppError> {
    let mut selected = None;
    for (index, line) in RELEASE_TARGET_MANIFEST.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() != 6 || fields.iter().any(|field| field.is_empty()) {
            return Err(AppError::runtime(format!(
                "release target manifest 행이 유효하지 않습니다: line {}",
                index + 1
            )));
        }
        if fields[0] != os || fields[1] != arch {
            continue;
        }
        let archive_kind = match fields[4] {
            "tar.gz" => ReleaseArchiveKind::TarGz,
            "zip" => ReleaseArchiveKind::Zip,
            archive => {
                return Err(AppError::runtime(format!(
                    "release target archive 형식이 유효하지 않습니다: {archive}"
                )))
            }
        };
        if selected.is_some() {
            return Err(AppError::runtime(format!(
                "release target manifest에 중복 platform이 있습니다: {os}/{arch}"
            )));
        }
        selected = Some((fields[2], archive_kind, fields[4], fields[3]));
    }
    Ok(selected)
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

fn parse_current_version(value: &str) -> Result<(ReleaseVersion, bool), AppError> {
    let without_build = value.split_once('+').map_or(value, |(base, _)| base);
    let (core, prerelease) = without_build
        .split_once('-')
        .map_or((without_build, None), |(core, suffix)| (core, Some(suffix)));
    if prerelease.is_some_and(|suffix| {
        suffix.is_empty()
            || suffix.split('.').any(|identifier| identifier.is_empty())
            || !suffix
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'.')
    }) {
        return Err(AppError::blocked(format!(
            "version prerelease 형식이 유효하지 않습니다: {value}"
        )));
    }
    Ok((parse_version(core)?, prerelease.is_some()))
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

        let stable_from_prerelease = classify_update(
            "0.44.0-alpha.1",
            "v0.44.0",
            "https://github.com/MCprotein/rolling-potato/releases/tag/v0.44.0",
        )
        .unwrap();
        assert!(matches!(
            stable_from_prerelease,
            UpdateAvailability::Available(_)
        ));

        let older_stable = classify_update(
            "0.45.0-alpha.1",
            "v0.44.0",
            "https://github.com/MCprotein/rolling-potato/releases/tag/v0.44.0",
        )
        .unwrap();
        assert!(matches!(older_stable, UpdateAvailability::Current { .. }));
    }

    #[test]
    fn official_asset_plan_covers_exact_release_matrix() {
        let cases = RELEASE_TARGET_MANIFEST
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(|line| {
                let fields = line.split('\t').collect::<Vec<_>>();
                assert_eq!(fields.len(), 6, "invalid release target row: {line}");
                (fields[0], fields[1], fields[2], fields[3], fields[4])
            })
            .collect::<Vec<_>>();
        assert_eq!(cases.len(), 5);
        for (os, arch, target, binary, extension) in cases {
            let plan = release_asset_plan("v0.44.0", os, arch).unwrap();
            assert_eq!(plan.target, target);
            assert_eq!(plan.binary_name, binary);
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
