#[test]
fn executable_install_creates_updates_and_preserves_managed_target() {
    let root = unique_temp("binary");
    let source = root.join("download/rpotato");
    let installed = root.join("home/.local/bin/rpotato");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, "version-one").unwrap();
    let mut paths = InstallPaths {
        source_binary: source.clone(),
        installed_binary: installed.clone(),
        user_bin: installed.parent().unwrap().to_path_buf(),
        user_home: root.join("home"),
        app_data: root.join("data/rpotato"),
        project_root: root.join("project"),
        project_state: root.join("project/.rpotato"),
    };

    assert_eq!(install_binary(&paths).unwrap(), Change::Created);
    assert_eq!(fs::read_to_string(&installed).unwrap(), "version-one");
    assert!(install_owner_file(&paths).is_file());

    fs::write(&source, "version-two").unwrap();
    assert_eq!(install_binary(&paths).unwrap(), Change::Updated);
    assert_eq!(fs::read_to_string(&installed).unwrap(), "version-two");

    paths.source_binary = installed.clone();
    assert_eq!(install_binary(&paths).unwrap(), Change::Unchanged);
    assert_eq!(fs::read_to_string(&installed).unwrap(), "version-two");
    assert!(install_owner_file(&paths).is_file());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn staged_update_replaces_only_the_managed_binary() {
    let root = unique_temp("self-update");
    let staged = root.join("cache/rpotato.ready");
    let installed = root.join("home/.local/bin/rpotato");
    fs::create_dir_all(staged.parent().unwrap()).unwrap();
    fs::create_dir_all(installed.parent().unwrap()).unwrap();
    fs::write(&staged, "version-two").unwrap();
    fs::write(&installed, "version-one").unwrap();
    let paths = InstallPaths {
        source_binary: installed.clone(),
        installed_binary: installed.clone(),
        user_bin: installed.parent().unwrap().to_path_buf(),
        user_home: root.join("home"),
        app_data: root.join("data/rpotato"),
        project_root: root.join("project"),
        project_state: root.join("project/.rpotato"),
    };

    assert_eq!(
        apply_staged_update(&paths, &staged).unwrap(),
        BinaryUpdateResult::Applied
    );
    assert_eq!(fs::read_to_string(&installed).unwrap(), "version-two");
    assert!(staged.is_file());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn windows_deferred_update_waits_for_exit_without_abandonment_deadline() {
    assert!(WINDOWS_SELF_UPDATE_SCRIPT.contains("if (-not $SkipParentWait)"));
    assert!(WINDOWS_SELF_UPDATE_SCRIPT
        .contains("while (Get-Process -Id $ParentPid -ErrorAction SilentlyContinue)"));
    assert!(!WINDOWS_SELF_UPDATE_SCRIPT.contains("6000"));
    assert!(!WINDOWS_SELF_UPDATE_SCRIPT.contains("{ exit 1 }"));
}

#[test]
fn windows_pending_update_reservation_is_single_flight() {
    let root = unique_temp("windows-update-reservation");
    let installed = root.join("bin/rpotato.exe");
    let paths = InstallPaths {
        source_binary: installed.clone(),
        installed_binary: installed,
        user_bin: root.join("bin"),
        user_home: root.join("home"),
        app_data: root.join("data/rpotato"),
        project_root: root.join("project"),
        project_state: root.join("project/.rpotato"),
    };
    fs::create_dir_all(&paths.user_bin).unwrap();

    let marker = reserve_windows_update_marker(&paths, "first-operation").unwrap();
    let error = reserve_windows_update_marker(&paths, "second-operation").unwrap_err();

    assert_eq!(marker, pending_update_marker_path(&paths));
    assert_eq!(error.code, 3);
    assert!(error.message.contains("pending update"));
    assert!(fs::read_to_string(&marker)
        .unwrap()
        .contains("first-operation"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn pending_update_blocks_every_binary_mutation_entry_point() {
    let root = unique_temp("pending-update-mutation-guard");
    let source = root.join("download/rpotato");
    let installed = root.join("bin/rpotato");
    let staged = root.join("download/rpotato-next");
    let paths = InstallPaths {
        source_binary: source.clone(),
        installed_binary: installed.clone(),
        user_bin: root.join("bin"),
        user_home: root.join("home"),
        app_data: root.join("data/rpotato"),
        project_root: root.join("project"),
        project_state: root.join("project/.rpotato"),
    };
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::create_dir_all(&paths.user_bin).unwrap();
    fs::write(&source, "installer").unwrap();
    fs::write(&installed, "installed").unwrap();
    fs::write(&staged, "update").unwrap();
    let marker = reserve_windows_update_marker(&paths, "scheduled-operation").unwrap();

    for error in [
        install_binary(&paths).unwrap_err(),
        update_installed_binary(&paths, &staged).unwrap_err(),
        remove_installed_binary(&paths).unwrap_err(),
    ] {
        assert_eq!(error.code, 3);
        assert!(error.message.contains("pending update"));
    }
    assert_eq!(fs::read_to_string(&installed).unwrap(), "installed");
    assert!(marker.is_file());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn windows_deferred_update_script_uses_target_cas_and_operation_paths() {
    assert!(WINDOWS_SELF_UPDATE_SCRIPT.contains("[System.Security.Cryptography.SHA256]::Create()"));
    assert!(!WINDOWS_SELF_UPDATE_SCRIPT.contains("Get-FileHash"));
    assert!(WINDOWS_SELF_UPDATE_SCRIPT.contains("$ExpectedTargetSha"));
    assert!(WINDOWS_SELF_UPDATE_SCRIPT.contains("$BackupPath"));
    assert!(WINDOWS_SELF_UPDATE_SCRIPT.contains("$MarkerPath"));
    assert!(!WINDOWS_SELF_UPDATE_SCRIPT.contains("$Target.rpotato-update-backup"));
}

#[cfg(windows)]
#[test]
fn windows_deferred_update_preserves_target_changed_after_schedule() {
    let root = unique_temp("windows-update-cas");
    let script = root.join("update.ps1");
    let source = root.join("rpotato.pending.exe");
    let target = root.join("rpotato.exe");
    let marker = root.join(".rpotato-update-pending");
    let backup = root.join("rpotato.backup.exe");
    fs::create_dir_all(&root).unwrap();
    fs::write(&script, WINDOWS_SELF_UPDATE_SCRIPT).unwrap();
    fs::write(&source, "scheduled-update").unwrap();
    fs::write(&target, "version-one").unwrap();
    fs::write(&marker, "test-operation\n").unwrap();
    let expected = crate::foundation::integrity::sha256_file(&target).unwrap();
    fs::write(&target, "newer-install").unwrap();

    let output = std::process::Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&script)
        .arg("0")
        .arg(&source)
        .arg(&target)
        .arg(&script)
        .arg(&marker)
        .arg(&expected)
        .arg(&backup)
        .arg("test-operation")
        .arg("-SkipParentWait")
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(3),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read_to_string(&target).unwrap(), "newer-install");
    assert!(!source.exists());
    assert!(!marker.exists());
    assert!(!backup.exists());
    let _ = fs::remove_dir_all(root);
}
