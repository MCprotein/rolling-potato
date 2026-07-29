#[test]
fn github_release_adapter_splits_bounded_release_io_by_responsibility() {
    let facade_path = "src/adapters/github_release.rs";
    let discovery_path = "src/adapters/github_release/discovery.rs";
    let download_path = "src/adapters/github_release/download.rs";
    let archive_path = "src/adapters/github_release/archive.rs";
    let tests_path = "src/adapters/github_release/tests.rs";
    for path in [
        facade_path,
        discovery_path,
        download_path,
        archive_path,
        tests_path,
    ] {
        assert!(
            Path::new(path).is_file(),
            "missing GitHub release owner: {path}"
        );
    }

    let facade = fs::read_to_string(facade_path).unwrap();
    let discovery = fs::read_to_string(discovery_path).unwrap();
    let download = fs::read_to_string(download_path).unwrap();
    let archive = fs::read_to_string(archive_path).unwrap();
    let tests = fs::read_to_string(tests_path).unwrap();

    for module in ["mod archive;", "mod discovery;", "mod download;"] {
        assert!(facade.lines().any(|line| line == module));
    }
    assert!(facade.contains("pub(crate) use discovery::{"));
    assert!(facade.contains("pub(crate) use download::download_release_binary;"));
    for (owner, responsibilities) in [
        (
            discovery.as_str(),
            &[
                "pub(crate) fn latest_release_with_cache_fallback(",
                "pub(super) fn parse_latest_release(",
                "fn read_latest_cache(",
            ][..],
        ),
        (
            download.as_str(),
            &[
                "pub(crate) fn download_release_binary(",
                "fn download_text(",
                "fn ensure_archive(",
            ][..],
        ),
        (
            archive.as_str(),
            &[
                "pub(super) fn copy_with_limit(",
                "pub(super) fn extract_binary(",
                "pub(super) fn safe_components(",
            ][..],
        ),
    ] {
        for responsibility in responsibilities {
            assert!(owner.contains(responsibility));
            assert!(!facade.contains(responsibility));
        }
    }
    for regression in [
        "fn startup_lookup_uses_cache_only_when_refresh_fails(",
        "fn archive_paths_reject_parent_and_absolute_components(",
        "fn copy_limit_rejects_oversized_payload(",
    ] {
        assert!(tests.contains(regression));
    }
    for (path, source) in [
        (facade_path, facade),
        (discovery_path, discovery),
        (download_path, download),
        (archive_path, archive),
        (tests_path, tests),
    ] {
        assert!(
            source.lines().count() < 500,
            "GitHub release owner regrew beyond boundary: {path}"
        );
    }
}
