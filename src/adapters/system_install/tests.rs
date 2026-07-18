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

    fs::write(&source, "version-two").unwrap();
    assert_eq!(install_binary(&paths).unwrap(), Change::Updated);
    assert_eq!(fs::read_to_string(&installed).unwrap(), "version-two");

    paths.source_binary = installed.clone();
    assert_eq!(install_binary(&paths).unwrap(), Change::Unchanged);
    assert_eq!(fs::read_to_string(&installed).unwrap(), "version-two");
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
