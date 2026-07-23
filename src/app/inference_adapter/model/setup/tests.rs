use super::*;

#[test]
fn missing_declared_projector_blocks_setup_before_model_switch() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-setup-projector-test-{}",
        std::process::id()
    ));
    let previous = std::env::var_os("RPOTATO_DATA_HOME");
    let _ = std::fs::remove_dir_all(&root);
    std::env::set_var("RPOTATO_DATA_HOME", &root);
    let candidate = find_candidate("gemma-4-e4b").unwrap();

    let error = require_declared_projector(candidate).unwrap_err();

    if let Some(previous) = previous {
        std::env::set_var("RPOTATO_DATA_HOME", previous);
    } else {
        std::env::remove_var("RPOTATO_DATA_HOME");
    }
    let _ = std::fs::remove_dir_all(root);
    assert!(error.message.contains("모델 변경을 중단"));
    assert!(error.message.contains("현재 모델과 backend는 그대로 유지"));
}

#[test]
fn setup_options_expose_each_models_manifest_context_limit() {
    let options = setup_options();

    assert_eq!(
        options
            .iter()
            .find(|option| option.id == "qwen3.5-4b")
            .and_then(|option| option.context_length),
        Some(262_144)
    );
    assert_eq!(
        options
            .iter()
            .find(|option| option.id == "gemma-4-e4b")
            .and_then(|option| option.context_length),
        Some(131_072)
    );
}
