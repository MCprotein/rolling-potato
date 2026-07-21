use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

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
fn clean_state_removes_only_managed_roots() {
    let root = unique_temp("clean-state");
    let home = root.join("home");
    let project = root.join("project");
    let app_data = root.join("data").join("rpotato");
    let project_state = project.join(".rpotato");
    let installed_binary = home.join(".local/bin/rpotato");
    fs::create_dir_all(&app_data).unwrap();
    fs::create_dir_all(&project_state).unwrap();
    fs::create_dir_all(installed_binary.parent().unwrap()).unwrap();
    fs::write(app_data.join("model"), "managed").unwrap();
    fs::write(project_state.join("state"), "managed").unwrap();
    fs::write(project.join("keep.txt"), "keep").unwrap();
    fs::write(&installed_binary, "binary").unwrap();
    let paths = InstallPaths {
        source_binary: root.join("source"),
        installed_binary,
        user_bin: home.join(".local/bin"),
        user_home: home,
        app_data: app_data.clone(),
        project_root: project.clone(),
        project_state: project_state.clone(),
    };

    let result = remove_clean_state(&paths).unwrap();

    assert_eq!(
        result,
        CleanStateResult {
            app_data_removed: true,
            project_state_removed: true
        }
    );
    assert!(!app_data.exists());
    assert!(!project_state.exists());
    assert!(project.join("keep.txt").is_file());
    assert!(paths.installed_binary.is_file());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn clean_state_rejects_project_root_as_data_home() {
    let root = unique_temp("unsafe-clean");
    let home = root.join("home");
    let project = root.join("project");
    let paths = InstallPaths {
        source_binary: root.join("source"),
        installed_binary: home.join(".local/bin/rpotato"),
        user_bin: home.join(".local/bin"),
        user_home: home,
        app_data: project.clone(),
        project_root: project.clone(),
        project_state: project.join(".rpotato"),
    };

    let err = validate_clean_targets(&paths).unwrap_err();

    assert_eq!(err.code, 3);
    assert!(err.message.contains("보호 경로"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn clean_state_rejects_source_binary_inside_each_deletion_root() {
    let root = unique_temp("protected-source");
    let home = root.join("home");
    let project = root.join("project");
    let app_data = root.join("data/rpotato");
    let project_state = project.join(".rpotato");
    let installed_binary = home.join(".local/bin/rpotato");
    fs::create_dir_all(&app_data).unwrap();
    fs::create_dir_all(&project_state).unwrap();

    for source_binary in [
        app_data.join("download/rpotato"),
        project_state.join("download/rpotato"),
    ] {
        fs::create_dir_all(source_binary.parent().unwrap()).unwrap();
        fs::write(&source_binary, "source").unwrap();
        let paths = InstallPaths {
            source_binary,
            installed_binary: installed_binary.clone(),
            user_bin: installed_binary.parent().unwrap().to_path_buf(),
            user_home: home.clone(),
            app_data: app_data.clone(),
            project_root: project.clone(),
            project_state: project_state.clone(),
        };

        let err = validate_clean_targets(&paths).unwrap_err();

        assert_eq!(err.code, 3);
        assert!(err.message.contains("차단"));
    }
    let _ = fs::remove_dir_all(root);
}

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

#[cfg(unix)]
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
    assert!(WINDOWS_SELF_UPDATE_SCRIPT.contains("Get-FileHash"));
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

    let status = std::process::Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&script)
        .arg(i32::MAX.to_string())
        .arg(&source)
        .arg(&target)
        .arg(&script)
        .arg(&marker)
        .arg(&expected)
        .arg(&backup)
        .arg("test-operation")
        .status()
        .unwrap();

    assert_eq!(status.code(), Some(3));
    assert_eq!(fs::read_to_string(&target).unwrap(), "newer-install");
    assert!(!source.exists());
    assert!(!marker.exists());
    assert!(!backup.exists());
    let _ = fs::remove_dir_all(root);
}

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

#[cfg(unix)]
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

#[cfg(unix)]
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

fn unique_temp(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    env::temp_dir().join(format!(
        "rpotato-system-install-{label}-{}-{nonce}",
        std::process::id()
    ))
}
