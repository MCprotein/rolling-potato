fn assert_plugin_storage_contract(
    plugin_adapter: &str,
    plugin_registry_path: &str,
    plugin_scanner: &str,
) {
    assert!(
        plugin_adapter.lines().any(|line| line == "mod scanner;"),
        "plugin adapter does not register its scanner owner"
    );
    assert!(
        plugin_adapter.lines().any(|line| line == "mod registry;"),
        "plugin adapter does not register its registry owner"
    );
    let plugin_registry = fs::read_to_string(plugin_registry_path).unwrap();
    for responsibility in [
        "pub(super) struct PluginSnapshot",
        "pub(super) fn persist_plugin(",
        "pub(super) fn verify_imported_snapshot(",
        "pub(super) fn read_plugins(",
        "pub(super) fn read_plugin(",
        "pub(super) fn write_plugin_manifest(",
        "pub(super) fn write_validation_report(",
    ] {
        assert!(
            plugin_registry.contains(responsibility),
            "plugin registry owner is missing: {responsibility}"
        );
        assert!(
            !plugin_adapter.contains(responsibility),
            "plugin adapter still owns registry behavior: {responsibility}"
        );
    }
    for responsibility in [
        "pub(super) struct DirectoryScan",
        "pub(super) fn scan_directory(",
        "pub(super) fn copy_dir_recursive(",
        "fn classify_runtime_file(",
        "pub(super) fn sha256_directory_snapshot(",
        "fn collect_snapshot_entries(",
    ] {
        assert!(
            plugin_scanner.contains(responsibility),
            "plugin scanner owner is missing: {responsibility}"
        );
        assert!(
            !plugin_adapter.contains(responsibility),
            "plugin adapter still owns scanner behavior: {responsibility}"
        );
    }
}
