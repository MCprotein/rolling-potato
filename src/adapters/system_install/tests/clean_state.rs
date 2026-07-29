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
