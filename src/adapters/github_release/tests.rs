use super::archive::{copy_with_limit, extract_binary, safe_components};
use super::discovery::parse_latest_release;
use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn latest_release_uses_only_valid_stable_tag() {
    let release = parse_latest_release(
            r#"{"tag_name":"v0.44.0","html_url":"https://evil.invalid/release","assets":[{"name":"rpotato-v0.44.0-checksums.txt","state":"uploaded"}]}"#,
        )
        .unwrap();
    assert_eq!(release.tag, "v0.44.0");
    assert_eq!(
        release.release_url,
        "https://github.com/MCprotein/rolling-potato/releases/tag/v0.44.0"
    );
    assert!(parse_latest_release(r#"{"tag_name":"nightly","assets":[]}"#).is_err());
    assert!(parse_latest_release(r#"{"name":"v0.44.0"}"#).is_err());
}

#[test]
fn latest_release_is_hidden_until_verified_assets_are_uploaded() {
    assert!(parse_latest_release(r#"{"tag_name":"v0.44.0","assets":[]}"#).is_err());
    assert!(parse_latest_release(
            r#"{"tag_name":"v0.44.0","assets":[{"name":"rpotato-v0.44.0-checksums.txt","state":"new"}]}"#
        )
        .is_err());
    assert!(parse_latest_release(
            r#"{"tag_name":"v0.44.0","assets":[{"name":"rpotato-v0.44.0-aarch64-apple-darwin.tar.gz","state":"uploaded"}]}"#
        )
        .is_err());
}

#[test]
fn startup_lookup_refreshes_an_existing_cache_before_returning_it() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = unique_temp("startup-refresh");
    let cache_path = root.join("cache/update-latest-v2");
    fs::create_dir_all(cache_path.parent().unwrap()).unwrap();
    fs::write(&cache_path, "v0.47.1\n").unwrap();
    std::env::set_var("RPOTATO_DATA_HOME", &root);
    std::env::set_var(
        "RPOTATO_TEST_LATEST_RELEASE_JSON",
        r#"{"tag_name":"v0.48.0","assets":[{"name":"rpotato-v0.48.0-checksums.txt","state":"uploaded"}]}"#,
    );

    let release = latest_release_with_cache_fallback(Duration::from_millis(10)).unwrap();

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_TEST_LATEST_RELEASE_JSON");
    assert_eq!(release.tag, "v0.48.0");
    assert_eq!(fs::read_to_string(&cache_path).unwrap(), "v0.48.0\n");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn startup_lookup_uses_cache_only_when_refresh_fails() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = unique_temp("startup-fallback");
    let cache_path = root.join("cache/update-latest-v2");
    fs::create_dir_all(cache_path.parent().unwrap()).unwrap();
    fs::write(&cache_path, "v0.47.1\n").unwrap();
    std::env::set_var("RPOTATO_DATA_HOME", &root);
    std::env::set_var("RPOTATO_TEST_LATEST_RELEASE_JSON", "not-json");

    let release = latest_release_with_cache_fallback(Duration::from_millis(10)).unwrap();

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_TEST_LATEST_RELEASE_JSON");
    assert_eq!(release.tag, "v0.47.1");
    let _ = fs::remove_dir_all(root);
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
