#[test]
fn uninstall_adapter_has_platform_and_ownership_boundaries() {
    let facade = fs::read_to_string("src/adapters/system_install/uninstall.rs").unwrap();
    let ownership =
        fs::read_to_string("src/adapters/system_install/uninstall/ownership.rs").unwrap();
    let path_registration =
        fs::read_to_string("src/adapters/system_install/uninstall/path_registration.rs").unwrap();
    let windows_cleanup =
        fs::read_to_string("src/adapters/system_install/uninstall/windows_cleanup.rs").unwrap();

    for owner in ["ownership", "path_registration"] {
        assert!(
            facade.lines().any(|line| line == format!("mod {owner};")),
            "uninstall facade does not register {owner}"
        );
    }
    assert!(facade.contains("mod windows_cleanup;"));

    for responsibility in [
        "pub(crate) fn validate_clean_uninstall_targets(",
        "pub(crate) fn binary_removal_plan(",
        "pub(crate) fn remove_installed_binary(",
        "fn record_install_ownership(",
        "fn install_is_owned(",
    ] {
        assert!(ownership.contains(responsibility));
        assert!(!facade.contains(responsibility));
    }
    for responsibility in [
        "pub(crate) fn user_path_removal_plan(",
        "pub(crate) fn remove_user_path(",
        "fn render_profile_without_managed_block(",
        "fn windows_path_removal(",
    ] {
        assert!(path_registration.contains(responsibility));
        assert!(!facade.contains(responsibility));
    }
    for responsibility in [
        "fn schedule_windows_self_delete(",
        "fn remove_empty_windows_install_dirs(",
    ] {
        assert!(windows_cleanup.contains(responsibility));
        assert!(!facade.contains(responsibility));
    }

    assert!(facade.lines().count() < 50);
    assert!(ownership.lines().count() < 200);
    assert!(path_registration.lines().count() < 350);
    assert!(windows_cleanup.lines().count() < 175);
}
