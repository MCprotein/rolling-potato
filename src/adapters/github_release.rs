//! Official GitHub Release discovery, caching, download, and payload extraction.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::adapters::filesystem::{atomic_write, layout};
use crate::foundation::error::AppError;
use crate::foundation::{integrity, serialization};
use crate::runtime_core::update::{
    parse_checksum_line, parse_release_tag, release_asset_plan, ReleaseArchiveKind,
    ReleaseAssetPlan,
};

const LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/MCprotein/rolling-potato/releases/latest";
const RELEASE_DOWNLOAD_ROOT: &str = "https://github.com/MCprotein/rolling-potato/releases/download";
const RELEASE_PAGE_ROOT: &str = "https://github.com/MCprotein/rolling-potato/releases/tag";
const CACHE_TTL: Duration = Duration::from_secs(6 * 60 * 60);
const MAX_METADATA_BYTES: u64 = 64 * 1024;
const MAX_CHECKSUM_BYTES: u64 = 4 * 1024;
const MAX_ARCHIVE_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LatestRelease {
    pub(crate) tag: String,
    pub(crate) release_url: String,
}

pub(crate) fn cached_latest_release(timeout: Duration) -> Result<LatestRelease, AppError> {
    let cache_path = latest_cache_path();
    if cache_is_fresh(&cache_path) {
        if let Ok(release) = read_latest_cache(&cache_path) {
            return Ok(release);
        }
    }

    match fetch_latest_release(timeout) {
        Ok(release) => {
            atomic_write::atomic_replace_bytes(
                &cache_path,
                format!("{}\n", release.tag).as_bytes(),
            )?;
            Ok(release)
        }
        Err(error) => read_latest_cache(&cache_path).or(Err(error)),
    }
}

pub(crate) fn fetch_latest_release(timeout: Duration) -> Result<LatestRelease, AppError> {
    #[cfg(debug_assertions)]
    if let Some(body) = std::env::var_os("RPOTATO_TEST_LATEST_RELEASE_JSON") {
        return parse_latest_release(&body.to_string_lossy());
    }

    let config = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .https_only(true)
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let mut response = agent
        .get(LATEST_RELEASE_API)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", concat!("rpotato/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|err| AppError::runtime(format!("최신 release 확인 실패: {err}")))?;
    let body = response
        .body_mut()
        .with_config()
        .limit(MAX_METADATA_BYTES)
        .read_to_string()
        .map_err(|err| AppError::runtime(format!("최신 release 응답 읽기 실패: {err}")))?;
    parse_latest_release(&body)
}

pub(crate) fn download_release_binary(release: &LatestRelease) -> Result<PathBuf, AppError> {
    let plan = release_asset_plan(&release.tag, std::env::consts::OS, std::env::consts::ARCH)?;
    let release_dir = layout::cache_dir()
        .join("updates")
        .join(&release.tag)
        .join(&plan.target);
    fs::create_dir_all(&release_dir).map_err(|err| {
        AppError::runtime(format!(
            "update cache directory 생성 실패: {} ({err})",
            release_dir.display()
        ))
    })?;
    let checksum_url = release_asset_url(&release.tag, &plan.checksum_name);
    let checksum_body = download_text(&checksum_url, MAX_CHECKSUM_BYTES, Duration::from_secs(15))?;
    let expected_sha256 = parse_checksum_line(&checksum_body, &plan.archive_name)?;
    let archive_path = release_dir.join(&plan.archive_name);
    ensure_archive(&release.tag, &plan, &expected_sha256, &archive_path)?;

    let staged_binary = release_dir.join(format!("{}.ready", plan.binary_name));
    remove_file_if_exists(&staged_binary)?;
    extract_binary(&plan, &archive_path, &staged_binary)?;
    Ok(staged_binary)
}

fn parse_latest_release(body: &str) -> Result<LatestRelease, AppError> {
    let serialization::Value::Object(object) =
        serialization::parse_value(body, "GitHub latest release")?
    else {
        return Err(AppError::blocked(
            "GitHub latest release 응답 root가 object가 아닙니다.",
        ));
    };
    let Some(serialization::Value::String(tag)) = object.get("tag_name") else {
        return Err(AppError::blocked(
            "GitHub latest release 응답에 tag_name이 없습니다.",
        ));
    };
    latest_release_from_tag(tag)
}

fn latest_release_from_tag(tag: &str) -> Result<LatestRelease, AppError> {
    parse_release_tag(tag)?;
    Ok(LatestRelease {
        tag: tag.to_string(),
        release_url: format!("{RELEASE_PAGE_ROOT}/{tag}"),
    })
}

fn latest_cache_path() -> PathBuf {
    layout::cache_dir().join("update-latest-v1")
}

fn cache_is_fresh(path: &Path) -> bool {
    let Ok(modified) = fs::metadata(path).and_then(|metadata| metadata.modified()) else {
        return false;
    };
    SystemTime::now()
        .duration_since(modified)
        .is_ok_and(|age| age <= CACHE_TTL)
}

fn read_latest_cache(path: &Path) -> Result<LatestRelease, AppError> {
    let body = fs::read_to_string(path).map_err(|err| {
        AppError::runtime(format!(
            "update cache 읽기 실패: {} ({err})",
            path.display()
        ))
    })?;
    let tag = body.trim();
    if tag.is_empty() || body.lines().count() != 1 {
        return Err(AppError::blocked("update cache 형식이 유효하지 않습니다."));
    }
    latest_release_from_tag(tag)
}

fn release_asset_url(tag: &str, name: &str) -> String {
    format!("{RELEASE_DOWNLOAD_ROOT}/{tag}/{name}")
}

fn download_text(url: &str, limit: u64, timeout: Duration) -> Result<String, AppError> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .https_only(true)
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let mut response = agent
        .get(url)
        .header("User-Agent", concat!("rpotato/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|err| AppError::runtime(format!("release asset 다운로드 실패: {err}")))?;
    response
        .body_mut()
        .with_config()
        .limit(limit)
        .read_to_string()
        .map_err(|err| AppError::runtime(format!("release asset 응답 읽기 실패: {err}")))
}

fn ensure_archive(
    tag: &str,
    plan: &ReleaseAssetPlan,
    expected_sha256: &str,
    archive_path: &Path,
) -> Result<(), AppError> {
    if archive_path.is_file()
        && integrity::sha256_file(archive_path)?.eq_ignore_ascii_case(expected_sha256)
    {
        return Ok(());
    }
    remove_file_if_exists(archive_path)?;
    let partial_path = archive_path.with_extension("part");
    remove_file_if_exists(&partial_path)?;
    let archive_url = release_asset_url(tag, &plan.archive_name);
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(120)))
        .https_only(true)
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let response = agent
        .get(&archive_url)
        .header("User-Agent", concat!("rpotato/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|err| AppError::runtime(format!("release archive 다운로드 실패: {err}")))?;
    let (_, body) = response.into_parts();
    let mut reader = body.into_reader();
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    let mut output = options.open(&partial_path).map_err(|err| {
        AppError::runtime(format!(
            "release archive partial 생성 실패: {} ({err})",
            partial_path.display()
        ))
    })?;
    let download = copy_with_limit(&mut reader, &mut output, MAX_ARCHIVE_BYTES);
    if let Err(error) = download {
        drop(output);
        let _ = fs::remove_file(&partial_path);
        return Err(error);
    }
    output
        .flush()
        .and_then(|_| output.sync_all())
        .map_err(|err| AppError::runtime(format!("release archive sync 실패: {err}")))?;
    drop(output);
    let actual_sha256 = integrity::sha256_file(&partial_path)?;
    if !actual_sha256.eq_ignore_ascii_case(expected_sha256) {
        let _ = fs::remove_file(&partial_path);
        return Err(AppError::blocked(format!(
            "release archive SHA-256 검증 실패\n- expected: {expected_sha256}\n- actual: {actual_sha256}"
        )));
    }
    atomic_write::replace_file(&partial_path, archive_path).map_err(|err| {
        let _ = fs::remove_file(&partial_path);
        AppError::runtime(format!("release archive cache 배치 실패: {err}"))
    })?;
    atomic_write::sync_parent(archive_path)
}

fn copy_with_limit(
    reader: &mut impl Read,
    writer: &mut impl Write,
    limit: u64,
) -> Result<u64, AppError> {
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|err| AppError::runtime(format!("release archive stream 읽기 실패: {err}")))?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > limit {
            return Err(AppError::blocked(format!(
                "release archive가 허용 크기를 초과했습니다: {limit} bytes"
            )));
        }
        writer
            .write_all(&buffer[..read])
            .map_err(|err| AppError::runtime(format!("release archive 기록 실패: {err}")))?;
    }
    Ok(total)
}

fn extract_binary(
    plan: &ReleaseAssetPlan,
    archive_path: &Path,
    target: &Path,
) -> Result<(), AppError> {
    match plan.archive_kind {
        ReleaseArchiveKind::TarGz => extract_tar_binary(plan, archive_path, target),
        ReleaseArchiveKind::Zip => extract_zip_binary(plan, archive_path, target),
    }
}

fn extract_tar_binary(
    plan: &ReleaseAssetPlan,
    archive_path: &Path,
    target: &Path,
) -> Result<(), AppError> {
    let file = File::open(archive_path).map_err(|err| {
        AppError::runtime(format!(
            "release tar.gz 열기 실패: {} ({err})",
            archive_path.display()
        ))
    })?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    let expected_root = plan.archive_name.trim_end_matches(".tar.gz");
    let mut found = false;
    for entry in archive
        .entries()
        .map_err(|err| AppError::runtime(format!("release tar.gz metadata 읽기 실패: {err}")))?
    {
        let mut entry = entry
            .map_err(|err| AppError::runtime(format!("release tar.gz entry 읽기 실패: {err}")))?;
        let path = entry
            .path()
            .map_err(|err| AppError::blocked(format!("release tar.gz path 오류: {err}")))?;
        let components = safe_components(&path)?;
        if components == [expected_root, plan.binary_name.as_str()] {
            if found || !entry.header().entry_type().is_file() {
                return Err(AppError::blocked(
                    "release tar.gz binary entry가 중복되었거나 regular file이 아닙니다.",
                ));
            }
            write_extracted_binary(&mut entry, target)?;
            found = true;
        }
    }
    if !found {
        return Err(AppError::blocked(
            "release tar.gz에서 정확한 rpotato binary를 찾지 못했습니다.",
        ));
    }
    Ok(())
}

fn extract_zip_binary(
    plan: &ReleaseAssetPlan,
    archive_path: &Path,
    target: &Path,
) -> Result<(), AppError> {
    let file = File::open(archive_path).map_err(|err| {
        AppError::runtime(format!(
            "release zip 열기 실패: {} ({err})",
            archive_path.display()
        ))
    })?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|err| AppError::runtime(format!("release zip metadata 읽기 실패: {err}")))?;
    let mut match_index = None;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|err| AppError::runtime(format!("release zip entry 읽기 실패: {err}")))?;
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| AppError::blocked("release zip에 안전하지 않은 path가 있습니다."))?;
        let components = safe_components(&enclosed)?;
        if components == [plan.binary_name.as_str()] {
            if match_index.is_some() || entry.is_dir() {
                return Err(AppError::blocked(
                    "release zip binary entry가 중복되었거나 regular file이 아닙니다.",
                ));
            }
            match_index = Some(index);
        }
    }
    let index = match_index.ok_or_else(|| {
        AppError::blocked("release zip에서 정확한 rpotato.exe binary를 찾지 못했습니다.")
    })?;
    let mut entry = archive
        .by_index(index)
        .map_err(|err| AppError::runtime(format!("release zip binary 읽기 실패: {err}")))?;
    write_extracted_binary(&mut entry, target)
}

fn safe_components(path: &Path) -> Result<Vec<&str>, AppError> {
    path.components()
        .map(|component| match component {
            Component::Normal(value) => value
                .to_str()
                .ok_or_else(|| AppError::blocked("release archive path가 UTF-8이 아닙니다.")),
            _ => Err(AppError::blocked(
                "release archive에 안전하지 않은 path component가 있습니다.",
            )),
        })
        .collect()
}

fn write_extracted_binary(reader: &mut impl Read, target: &Path) -> Result<(), AppError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o755);
    }
    let mut output = options.open(target).map_err(|err| {
        AppError::runtime(format!(
            "update binary staging 생성 실패: {} ({err})",
            target.display()
        ))
    })?;
    let copied = copy_with_limit(reader, &mut output, MAX_ARCHIVE_BYTES);
    if let Err(error) = copied {
        drop(output);
        let _ = fs::remove_file(target);
        return Err(error);
    }
    output
        .flush()
        .and_then(|_| output.sync_all())
        .map_err(|err| AppError::runtime(format!("update binary staging sync 실패: {err}")))
}

fn remove_file_if_exists(path: &Path) -> Result<(), AppError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(AppError::runtime(format!(
            "update cache file 삭제 실패: {} ({err})",
            path.display()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::UNIX_EPOCH;

    #[test]
    fn latest_release_uses_only_valid_stable_tag() {
        let release = parse_latest_release(
            r#"{"tag_name":"v0.44.0","html_url":"https://evil.invalid/release"}"#,
        )
        .unwrap();
        assert_eq!(release.tag, "v0.44.0");
        assert_eq!(
            release.release_url,
            "https://github.com/MCprotein/rolling-potato/releases/tag/v0.44.0"
        );
        assert!(parse_latest_release(r#"{"tag_name":"nightly"}"#).is_err());
        assert!(parse_latest_release(r#"{"name":"v0.44.0"}"#).is_err());
    }

    #[test]
    fn archive_paths_reject_parent_and_absolute_components() {
        assert_eq!(
            safe_components(Path::new("package/rpotato")).unwrap(),
            ["package", "rpotato"]
        );
        assert!(safe_components(Path::new("../rpotato")).is_err());
        assert!(safe_components(Path::new("/rpotato")).is_err());
    }

    #[test]
    fn copy_limit_rejects_oversized_payload() {
        let mut input = &b"oversized"[..];
        let mut output = Vec::new();
        assert!(copy_with_limit(&mut input, &mut output, 4).is_err());
    }

    #[test]
    fn extracts_only_the_exact_tar_release_binary() {
        let root = unique_temp("tar");
        fs::create_dir_all(&root).unwrap();
        let plan = release_asset_plan("v0.44.0", "macos", "aarch64").unwrap();
        let archive_path = root.join(&plan.archive_name);
        let output_path = root.join("rpotato.ready");
        let file = File::create(&archive_path).unwrap();
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut archive = tar::Builder::new(encoder);
        let payload = b"verified-binary";
        let mut header = tar::Header::new_gnu();
        header.set_size(payload.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        archive
            .append_data(
                &mut header,
                "rpotato-v0.44.0-aarch64-apple-darwin/rpotato",
                &payload[..],
            )
            .unwrap();
        archive.into_inner().unwrap().finish().unwrap();

        extract_binary(&plan, &archive_path, &output_path).unwrap();

        assert_eq!(fs::read(&output_path).unwrap(), payload);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn extracts_only_the_exact_zip_release_binary() {
        let root = unique_temp("zip");
        fs::create_dir_all(&root).unwrap();
        let plan = release_asset_plan("v0.44.0", "windows", "x86_64").unwrap();
        let archive_path = root.join(&plan.archive_name);
        let output_path = root.join("rpotato.exe.ready");
        let file = File::create(&archive_path).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        archive
            .start_file("rpotato.exe", zip::write::SimpleFileOptions::default())
            .unwrap();
        archive.write_all(b"verified-windows-binary").unwrap();
        archive.finish().unwrap();

        extract_binary(&plan, &archive_path, &output_path).unwrap();

        assert_eq!(fs::read(&output_path).unwrap(), b"verified-windows-binary");
        let _ = fs::remove_dir_all(root);
    }

    fn unique_temp(label: &str) -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "rpotato-update-{label}-{}-{now}",
            std::process::id()
        ))
    }
}
