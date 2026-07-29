use super::*;

const LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/MCprotein/rolling-potato/releases/latest";
const RELEASE_PAGE_ROOT: &str = "https://github.com/MCprotein/rolling-potato/releases/tag";
const MAX_METADATA_BYTES: u64 = 64 * 1024;
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LatestRelease {
    pub(crate) tag: String,
    pub(crate) release_url: String,
}

pub(crate) fn latest_release_with_cache_fallback(
    timeout: Duration,
) -> Result<LatestRelease, AppError> {
    let cache_path = latest_cache_path();
    match refresh_latest_release(timeout) {
        Ok(release) => Ok(release),
        Err(error) => read_latest_cache(&cache_path).or(Err(error)),
    }
}

pub(crate) fn refresh_latest_release(timeout: Duration) -> Result<LatestRelease, AppError> {
    let release = fetch_latest_release(timeout)?;
    let cache_path = latest_cache_path();
    let _ =
        atomic_write::atomic_replace_bytes(&cache_path, format!("{}\n", release.tag).as_bytes());
    Ok(release)
}

fn fetch_latest_release(timeout: Duration) -> Result<LatestRelease, AppError> {
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

pub(super) fn parse_latest_release(body: &str) -> Result<LatestRelease, AppError> {
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
    require_ready_release_assets(&object, tag)?;
    latest_release_from_tag(tag)
}

fn require_ready_release_assets(object: &serialization::Object, tag: &str) -> Result<(), AppError> {
    parse_release_tag(tag)?;
    let expected = format!("rpotato-{tag}-checksums.txt");
    let Some(serialization::Value::Array(assets)) = object.get("assets") else {
        return Err(AppError::blocked(
            "GitHub latest release 응답에 assets 목록이 없습니다.",
        ));
    };
    let ready = assets.iter().any(|asset| {
        let serialization::Value::Object(asset) = asset else {
            return false;
        };
        matches!(asset.get("name"), Some(serialization::Value::String(name)) if name == &expected)
            && matches!(asset.get("state"), Some(serialization::Value::String(state)) if state == "uploaded")
    });
    if !ready {
        return Err(AppError::blocked(format!(
            "latest release asset 검증이 아직 완료되지 않았습니다: {expected}"
        )));
    }
    Ok(())
}

fn latest_release_from_tag(tag: &str) -> Result<LatestRelease, AppError> {
    parse_release_tag(tag)?;
    Ok(LatestRelease {
        tag: tag.to_string(),
        release_url: format!("{RELEASE_PAGE_ROOT}/{tag}"),
    })
}

fn latest_cache_path() -> PathBuf {
    layout::cache_dir().join("update-latest-v2")
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
