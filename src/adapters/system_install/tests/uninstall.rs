#[cfg(unix)]
#[test]
fn clean_uninstall_removes_binary_and_owned_profile_block_only() {
    let root = unique_temp("clean-uninstall");
    let source = root.join("download/rpotato");
    let installed = root.join("home/.local/bin/rpotato");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, "source").unwrap();
    let paths = InstallPaths {
        source_binary: source.clone(),
        installed_binary: installed.clone(),
        user_bin: installed.parent().unwrap().to_path_buf(),
        user_home: root.join("home"),
        app_data: root.join("data/rpotato"),
        project_root: root.join("project"),
        project_state: root.join("project/.rpotato"),
    };
    let profile = unix_path_plan(&paths).0;
    fs::create_dir_all(profile.parent().unwrap()).unwrap();
    fs::write(&profile, "export EDITOR=vim\n").unwrap();
    install_binary(&paths).unwrap();
    ensure_user_path(&paths).unwrap();

    assert_eq!(binary_removal_plan(&paths).unwrap(), Change::Removed);
    assert_eq!(
        user_path_removal_plan(&paths).unwrap().change,
        Change::Removed
    );
    let registration = remove_user_path(&paths).unwrap();
    let binary = remove_installed_binary(&paths).unwrap();

    assert_eq!(registration.change, Change::Removed);
    assert_eq!(binary.change, Change::Removed);
    assert!(!binary.deferred_until_exit);
    assert!(!installed.exists());
    assert!(!install_owner_file(&paths).exists());
    assert!(source.is_file(), "downloaded source remains user-owned");
    let profile_contents = fs::read_to_string(profile).unwrap();
    assert!(profile_contents.contains("export EDITOR=vim"));
    assert!(!profile_contents.contains(PROFILE_BEGIN));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn clean_uninstall_finds_owned_blocks_after_the_login_shell_changes() {
    let root = unique_temp("changed-shell-uninstall");
    let home = root.join("home");
    let installed = home.join(".local/bin/rpotato");
    let paths = InstallPaths {
        source_binary: root.join("download/rpotato"),
        installed_binary: installed.clone(),
        user_bin: installed.parent().unwrap().to_path_buf(),
        user_home: home.clone(),
        app_data: root.join("data/rpotato"),
        project_root: root.join("project"),
        project_state: root.join("project/.rpotato"),
    };
    let zsh_profile = home.join(".zshrc");
    fs::create_dir_all(&home).unwrap();
    fs::write(
        &zsh_profile,
        format!("export EDITOR=vim\n{PROFILE_BEGIN}\nexport PATH='managed'\n{PROFILE_END}\n"),
    )
    .unwrap();

    let plan = user_path_removal_plan(&paths).unwrap();
    let removed = remove_user_path(&paths).unwrap();

    assert_eq!(plan.change, Change::Removed);
    assert!(plan.owner.contains(".zshrc"));
    assert_eq!(removed.change, Change::Removed);
    assert!(!fs::read_to_string(zsh_profile)
        .unwrap()
        .contains(PROFILE_BEGIN));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn clean_uninstall_rejects_binary_outside_managed_bin_boundary() {
    let root = unique_temp("unsafe-uninstall");
    let paths = InstallPaths {
        source_binary: root.join("download/rpotato"),
        installed_binary: root.join("other/rpotato"),
        user_bin: root.join("home/.local/bin"),
        user_home: root.join("home"),
        app_data: root.join("data/rpotato"),
        project_root: root.join("project"),
        project_state: root.join("project/.rpotato"),
    };

    let err = validate_clean_uninstall_targets(&paths).unwrap_err();

    assert_eq!(err.code, 3);
    assert!(err.message.contains("binary 경계"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn clean_uninstall_preserves_an_unowned_binary_at_the_canonical_path() {
    let root = unique_temp("unowned-binary");
    let installed = root.join(if cfg!(windows) {
        "bin/rpotato.exe"
    } else {
        "bin/rpotato"
    });
    fs::create_dir_all(installed.parent().unwrap()).unwrap();
    fs::write(&installed, "user-owned").unwrap();
    let paths = InstallPaths {
        source_binary: root.join("download/source"),
        installed_binary: installed.clone(),
        user_bin: installed.parent().unwrap().to_path_buf(),
        user_home: root.join("home"),
        app_data: root.join("data/rpotato"),
        project_root: root.join("project"),
        project_state: root.join("project/.rpotato"),
    };

    assert_eq!(binary_removal_plan(&paths).unwrap(), Change::Unchanged);
    assert_eq!(
        remove_installed_binary(&paths).unwrap(),
        BinaryRemovalResult {
            change: Change::Unchanged,
            deferred_until_exit: false
        }
    );
    assert_eq!(fs::read_to_string(&installed).unwrap(), "user-owned");
    let _ = fs::remove_dir_all(root);
}
