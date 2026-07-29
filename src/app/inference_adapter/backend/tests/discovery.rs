#[test]
fn default_discovery_uses_managed_path() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    env::remove_var(ENV_BACKEND_PATH);
    env::remove_var(ENV_BACKEND_PORT);
    let data_root = env::temp_dir().join(format!("rpotato-backend-test-{}", std::process::id()));
    env::set_var("RPOTATO_DATA_HOME", &data_root);

    let discovery = llama_backend::discover();

    env::remove_var("RPOTATO_DATA_HOME");
    assert_eq!(discovery.adapter_id, "llama.cpp");
    assert_eq!(discovery.selected_source, "managed");
    assert!(discovery
        .selected_path
        .ends_with(LlamaCppAdapter.binary_name()));
    assert_eq!(discovery.port, DEFAULT_PORT);
}

#[test]
fn backend_path_and_port_can_come_from_env() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let override_path = env::temp_dir().join("custom-llama-server");
    env::set_var(ENV_BACKEND_PATH, &override_path);
    env::set_var(ENV_BACKEND_PORT, "19090");

    let discovery = llama_backend::discover();

    env::remove_var(ENV_BACKEND_PATH);
    env::remove_var(ENV_BACKEND_PORT);
    assert_eq!(discovery.selected_path, override_path);
    assert_eq!(discovery.selected_source, "env override");
    assert_eq!(discovery.port, 19090);
    assert_eq!(discovery.port_source, "env override");
}

#[test]
fn invalid_backend_port_falls_back_to_default() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    env::set_var(ENV_BACKEND_PORT, "0");

    let discovery = llama_backend::discover();

    env::remove_var(ENV_BACKEND_PORT);
    assert_eq!(discovery.port, DEFAULT_PORT);
    assert_eq!(discovery.port_source, "invalid env, default");
}
