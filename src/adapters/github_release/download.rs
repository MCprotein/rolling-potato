use super::archive::{copy_with_limit, extract_binary, MAX_ARCHIVE_BYTES};
use super::*;

const RELEASE_DOWNLOAD_ROOT: &str = "https://github.com/MCprotein/rolling-potato/releases/download";
const MAX_CHECKSUM_BYTES: u64 = 4 * 1024;
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
