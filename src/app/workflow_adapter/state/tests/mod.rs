use super::*;

mod callgraph;
mod current_snapshot;
mod lifecycle;
mod source_install;
mod workflow_store;

fn workflow_test_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "rpotato-{name}-{}-{}",
        std::process::id(),
        now_ms()
    ))
}

fn with_workflow_env<T>(name: &str, test: impl FnOnce(&PathBuf) -> T) -> T {
    let root = workflow_test_root(name);
    let project = root.join("project");
    fs::create_dir_all(&project).unwrap();
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
    initialize().unwrap();
    let result = test(&root);
    std::env::remove_var("RPOTATO_TEST_CHECKPOINT_FAULT");
    std::env::remove_var("RPOTATO_TEST_STATE_TRANSITION_FAULT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);
    result
}
