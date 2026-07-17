use super::*;

#[test]
fn state_writer_callgraph_is_closed_and_serialized_by_project_transition() {
    fn collect_rust_files(directory: &std::path::Path, files: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                collect_rust_files(&path, files);
            } else if path.extension().and_then(|value| value.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }

    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-current-writer-transition-{}-{}",
        std::process::id(),
        now_ms()
    ));
    let project = root.join("project");
    let data = root.join("data");
    fs::create_dir_all(&project).unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
    std::env::set_var("RPOTATO_DATA_HOME", &data);
    let initialized = initialize().unwrap();
    let before = current_state_lease_view().unwrap();
    let transition = transition::TransitionGuard::acquire_for(
        &initialized.identity.project_id,
        transition::CurrentStateIntent::RecordEvent,
    )
    .unwrap();
    let (sender, receiver) = std::sync::mpsc::channel();
    let writer = std::thread::spawn(move || {
        sender.send(session_new_report()).unwrap();
    });
    assert!(receiver
        .recv_timeout(std::time::Duration::from_millis(100))
        .is_err());
    drop(transition);
    receiver
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap()
        .unwrap();
    writer.join().unwrap();
    let after = current_state_lease_view().unwrap();
    assert_eq!(after.revision, before.revision + 1);

    let source = include_str!("../../state.rs")
        .split("\n#[cfg(test)]\nmod tests {")
        .next()
        .unwrap();
    let source_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut patch_files = vec![source_dir.join("patch.rs")];
    collect_rust_files(&source_dir.join("patch"), &mut patch_files);
    let patch_tests = source_dir.join("patch/tests");
    patch_files.retain(|path| !path.starts_with(&patch_tests));
    let patch_source = patch_files
        .into_iter()
        .map(|path| {
            fs::read_to_string(path)
                .unwrap()
                .split("\n#[cfg(test)]\nmod tests {")
                .next()
                .unwrap()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!source.contains("pub fn write_current_state("));
    assert!(!source.contains("pub(crate) fn write_current_state("));
    assert!(!source.contains("pub fn write_current_state_for_session("));
    assert!(!source.contains("pub(crate) fn write_current_state_for_session("));
    assert!(!source.contains("pub(crate) fn install_current_image("));
    assert!(!source.contains("pub(crate) fn install_snapshot("));
    assert!(!source.contains("pub(crate) fn install_pointer("));
    assert!(!patch_source.contains(".install_snapshot("));
    assert!(!patch_source.contains(".install_pointer("));
    assert!(!patch_source.contains("state::install_current_image("));
    assert!(!patch_source.contains("paths::current_state_file()"));

    let state_adapter = source_dir.join("app/workflow_adapter/state.rs");
    let state_children = source_dir.join("app/workflow_adapter/state");
    let recovery_owner = source_dir.join("runtime_core/workflow/application/recovery.rs");
    let transaction_owner =
        source_dir.join("runtime_core/workflow/application/transaction_coordinator.rs");
    let authority_primitives = [
        "install_current_image(",
        "write_workflow_snapshot_bytes(",
        "write_workflow_pointer_for_schema(",
        ".install_snapshot(",
        ".install_pointer(",
    ];
    let mut rust_files = Vec::new();
    collect_rust_files(&source_dir, &mut rust_files);
    for path in rust_files {
        let production = fs::read_to_string(&path)
            .unwrap()
            .split("\n#[cfg(test)]\nmod tests {")
            .next()
            .unwrap()
            .to_string();
        for primitive in authority_primitives {
            if production.contains(primitive) {
                let is_state_owner = path == state_adapter || path.starts_with(&state_children);
                let is_application_port_call =
                    matches!(primitive, ".install_snapshot(" | ".install_pointer(")
                        && (path == recovery_owner || path == transaction_owner);
                assert!(
                    is_state_owner || is_application_port_call,
                    "authority primitive {primitive} escaped the state adapter into {}",
                    path.display(),
                );
            }
        }
    }
    let allowed_patch_transitions = [
        "state::transition_project_current_state_prepared_approval(",
        "state::transition_project_current_state_prepared_verification(",
        "state::transition_project_current_state_prepared_terminal_action(",
    ];
    for call in allowed_patch_transitions {
        assert!(
            patch_source.contains(call),
            "missing allowlisted call: {call}"
        );
    }
    assert_eq!(
        patch_source
            .matches("state::transition_project_current_state_prepared_")
            .count(),
        5,
        "patch.rs semantic writer allowlist changed"
    );

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(root);
}
