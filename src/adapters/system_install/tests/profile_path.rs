#[test]
fn managed_profile_is_idempotent_and_replaces_owned_block_only() {
    let first = render_managed_profile(
        "export EDITOR=vim\n",
        &format!("{PROFILE_BEGIN}\nPATH=v1\n{PROFILE_END}"),
    )
    .unwrap();
    assert_eq!(
        first,
        format!("export EDITOR=vim\n\n{PROFILE_BEGIN}\nPATH=v1\n{PROFILE_END}\n")
    );

    let second =
        render_managed_profile(&first, &format!("{PROFILE_BEGIN}\nPATH=v1\n{PROFILE_END}"))
            .unwrap();
    assert_eq!(second, first);

    let replaced =
        render_managed_profile(&first, &format!("{PROFILE_BEGIN}\nPATH=v2\n{PROFILE_END}"))
            .unwrap();
    assert!(replaced.contains("export EDITOR=vim"));
    assert!(replaced.contains("PATH=v2"));
    assert!(!replaced.contains("PATH=v1"));
}
#[test]
fn malformed_managed_profile_is_blocked() {
    let err = render_managed_profile(PROFILE_BEGIN, "replacement").unwrap_err();
    assert_eq!(err.code, 3);
    assert!(err.message.contains("marker"));
}

#[test]
fn managed_profile_removal_deletes_only_owned_block_and_is_idempotent() {
    let installed = render_managed_profile(
        "export EDITOR=vim\n",
        &format!("{PROFILE_BEGIN}\nPATH=managed\n{PROFILE_END}"),
    )
    .unwrap();

    let removed = render_profile_without_managed_block(&installed).unwrap();

    assert!(removed.contains("export EDITOR=vim"));
    assert!(!removed.contains(PROFILE_BEGIN));
    assert!(!removed.contains("PATH=managed"));
    assert_eq!(
        render_profile_without_managed_block(&removed).unwrap(),
        removed
    );
    assert_eq!(
        render_profile_without_managed_block(&format!(
            "{PROFILE_BEGIN}\nPATH=managed\n{PROFILE_END}\n"
        ))
        .unwrap(),
        ""
    );
}

#[test]
fn marker_text_inside_user_lines_is_not_treated_as_an_owned_block() {
    let user_text =
        format!("echo '{PROFILE_BEGIN}'\n# documentation: {PROFILE_END}\nexport EDITOR=vim\n");

    assert_eq!(
        render_profile_without_managed_block(&user_text).unwrap(),
        user_text
    );
    let installed = render_managed_profile(
        &user_text,
        &format!("{PROFILE_BEGIN}\nPATH=managed\n{PROFILE_END}"),
    )
    .unwrap();
    assert!(installed.contains(&format!("echo '{PROFILE_BEGIN}'")));
    assert_eq!(exact_line_ranges(&installed, PROFILE_BEGIN).len(), 1);
    assert_eq!(exact_line_ranges(&installed, PROFILE_END).len(), 1);
}

#[test]
fn binary_and_profile_plans_are_exact_and_read_only() {
    let root = unique_temp("plans");
    let source = root.join("download/rpotato");
    let installed = root.join("home/.local/bin/rpotato");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, "binary").unwrap();
    let paths = InstallPaths {
        source_binary: source,
        installed_binary: installed.clone(),
        user_bin: installed.parent().unwrap().to_path_buf(),
        user_home: root.join("home"),
        app_data: root.join("data/rpotato"),
        project_root: root.join("project"),
        project_state: root.join("project/.rpotato"),
    };
    let profile = unix_path_plan(&paths).0;

    assert_eq!(binary_install_plan(&paths).unwrap(), Change::Created);
    assert_eq!(
        user_path_change_plan(&paths).unwrap().change,
        Change::Created
    );
    assert!(!installed.exists());
    assert!(!profile.exists());

    install_binary(&paths).unwrap();
    ensure_user_path(&paths).unwrap();

    assert_eq!(binary_install_plan(&paths).unwrap(), Change::Updated);
    assert_eq!(
        user_path_change_plan(&paths).unwrap().change,
        Change::Unchanged
    );
    let _ = fs::remove_dir_all(root);
}

#[cfg(windows)]
#[test]
fn windows_powershell_path_update_is_idempotent_without_persisting_user_state() {
    let root = unique_temp("windows-path");
    let paths = InstallPaths {
        source_binary: root.join("download/rpotato.exe"),
        installed_binary: root.join("bin/rpotato.exe"),
        user_bin: root.join("bin"),
        user_home: root.join("home"),
        app_data: root.join("data/rpotato"),
        project_root: root.join("project"),
        project_state: root.join("project/.rpotato"),
    };

    let registrations =
        windows_path_registration(&paths, true, WindowsPathScope::Process, 2).unwrap();

    assert_eq!(registrations.len(), 2);
    assert_ne!(registrations[0].change, Change::Unchanged);
    assert_eq!(registrations[1].change, Change::Unchanged);
    assert_eq!(registrations[0].owner, "PowerShell process PATH");
}

#[cfg(windows)]
#[test]
fn windows_powershell_path_removal_is_exact_and_idempotent() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = unique_temp("windows-path-removal");
    let paths = InstallPaths {
        source_binary: root.join("download/rpotato.exe"),
        installed_binary: root.join("bin/rpotato.exe"),
        user_bin: root.join("bin"),
        user_home: root.join("home"),
        app_data: root.join("data/rpotato"),
        project_root: root.join("project"),
        project_state: root.join("project/.rpotato"),
    };
    let original_path = env::var_os("PATH");
    let seeded = match &original_path {
        Some(current) => format!("{};{}", paths.user_bin.display(), current.to_string_lossy()),
        None => paths.user_bin.display().to_string(),
    };
    env::set_var("PATH", seeded);

    let registrations = windows_path_removal(&paths, true, WindowsPathScope::Process, 2).unwrap();

    match original_path {
        Some(value) => env::set_var("PATH", value),
        None => env::remove_var("PATH"),
    }
    assert_eq!(registrations.len(), 2);
    assert_eq!(registrations[0].change, Change::Removed);
    assert_eq!(registrations[1].change, Change::Unchanged);
}

#[cfg(windows)]
#[test]
fn windows_user_path_without_owner_marker_is_preserved() {
    let root = unique_temp("windows-unowned-path");
    let paths = InstallPaths {
        source_binary: root.join("download/rpotato.exe"),
        installed_binary: root.join("bin/rpotato.exe"),
        user_bin: root.join("bin"),
        user_home: root.join("home"),
        app_data: root.join("data/rpotato"),
        project_root: root.join("project"),
        project_state: root.join("project/.rpotato"),
    };

    let registration = windows_path_removal(&paths, true, WindowsPathScope::User, 1).unwrap();

    assert_eq!(registration.len(), 1);
    assert_eq!(registration[0].change, Change::Unchanged);
    assert!(!windows_path_owner_file(&paths).exists());
}
