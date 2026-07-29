#[cfg(unix)]
#[test]
fn source_install_v1_round_trips_exact_order_and_bindings() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-source-install-v1-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("src")).unwrap();
    let target = root.join("src/lib.rs");
    fs::write(&target, b"current source\n").unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &root);
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    crate::app::workflow_adapter::state::initialize().unwrap();
    let plan = prepare_source_install_v1(
        "intent-source-fixture",
        "proposal-fixture",
        &target,
        b"current source\n",
        b"proposed source\n",
    )
    .unwrap();
    let body = render_source_install_v1(&plan).unwrap();
    assert_eq!(parse_source_install_v1(&body).unwrap(), plan);
    assert_eq!(plan.operations.len(), 19);
    assert_eq!(plan.target.path, "src/lib.rs");
    assert!(plan
        .rollback_final
        .path
        .starts_with(".rpotato/patches/proposal-fixture/intent-source-fixture-"));
    assert!(!body.ends_with('\n'));
    assert!(body.starts_with("{\"schema_version\":1,\"source_key\":"));

    let reordered = body.replacen("\"schema_version\":1,\"source_key\":", "\"source_key\":", 1);
    assert!(parse_source_install_v1(&reordered).is_err());

    let bundle = prepare_source_bundle(
        "intent-source-fixture",
        None,
        plan,
        b"current source\n",
        b"proposed source\n",
    )
    .unwrap();
    let bundle_body = render_prepared_source_bundle(&bundle).unwrap();
    assert_eq!(parse_prepared_source_bundle(&bundle_body).unwrap(), bundle);
    assert_eq!(bundle_body.matches("\"member_kind\"").count(), 3);
    let journal = commit_prepared_source_bundle(&bundle).unwrap();
    assert_eq!(commit_prepared_source_bundle(&bundle).unwrap(), journal);
    assert!(
        !paths::project_transition_journal_temp(&bundle.project_id, &bundle.intent_id).exists()
    );
    remove_committed_source_bundle(&bundle, &journal).unwrap();
    assert!(!journal.exists());
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn source_install_initial_admission_rejects_preexisting_exact_rollback() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-source-rollback-admission-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("src")).unwrap();
    let target = root.join("src/lib.rs");
    fs::write(&target, b"current source\n").unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &root);
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    crate::app::workflow_adapter::state::initialize().unwrap();
    let plan = prepare_source_install_v1(
        "intent-rollback-admission",
        "proposal-rollback-admission",
        &target,
        b"current source\n",
        b"proposed source\n",
    )
    .unwrap();
    let rollback = root.join(&plan.rollback_final.path);
    fs::create_dir_all(rollback.parent().unwrap()).unwrap();
    fs::write(&rollback, b"current source\n").unwrap();

    let error = prepare_source_install_v1(
        "intent-rollback-admission",
        "proposal-rollback-admission",
        &target,
        b"current source\n",
        b"proposed source\n",
    )
    .unwrap_err();

    assert!(error
        .message
        .contains("rollback path가 journal commit 전에 이미 존재"));
    assert!(!paths::project_transition_journal_file(
        &crate::app::workflow_adapter::ledger::fresh_identity().project_id,
        "intent-rollback-admission"
    )
    .exists());
    assert_eq!(fs::read(&target).unwrap(), b"current source\n");
    assert_eq!(fs::read(&rollback).unwrap(), b"current source\n");

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn source_install_v1_rejects_metadata_changes_in_prepared_bytes() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-source-install-metadata-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("src")).unwrap();
    let target = root.join("src/lib.rs");
    fs::write(&target, b"current source\n").unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &root);
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    crate::app::workflow_adapter::state::initialize().unwrap();
    let plan = prepare_source_install_v1(
        "intent-source-metadata",
        "proposal-metadata",
        &target,
        b"current source\n",
        b"proposed source\n",
    )
    .unwrap();

    let mut readonly = plan.clone();
    readonly.permissions.install_readonly = !readonly.permissions.before_readonly;
    assert!(validate_source_install_v1(&readonly).is_err());

    let mut mode = plan.clone();
    mode.permissions.install_mode ^= 0o100;
    mode.unix_metadata.install_mode = mode.permissions.install_mode;
    assert!(validate_source_install_v1(&mode).is_err());

    let mut owner = plan;
    owner.unix_metadata.install_uid = owner.unix_metadata.install_uid.wrapping_add(1);
    owner.ownership.install_owner = format!(
        "uid:{}:gid:{}",
        owner.unix_metadata.install_uid, owner.unix_metadata.install_gid
    );
    assert!(validate_source_install_v1(&owner).is_err());

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn aggregate_bundle_limit_rejects_before_journal_commit() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-prepared-aggregate-cap-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("src")).unwrap();
    let target = root.join("src/lib.rs");
    let before = vec![b'"'; MAX_SOURCE_BLOB_BYTES];
    let proposed = vec![b'\\'; MAX_SOURCE_BLOB_BYTES];
    fs::write(&target, &before).unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &root);
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    crate::app::workflow_adapter::state::initialize().unwrap();
    let plan = prepare_source_install_v1(
        "intent-aggregate-cap",
        "proposal-aggregate-cap",
        &target,
        &before,
        &proposed,
    )
    .unwrap();
    let bundle =
        prepare_source_bundle("intent-aggregate-cap", None, plan, &before, &proposed).unwrap();
    let journal = paths::project_transition_journal_file(&bundle.project_id, &bundle.intent_id);

    let error = commit_prepared_source_bundle(&bundle).unwrap_err();

    assert!(error.message.contains("prepared bundle byte limit"));
    assert!(!journal.exists());
    assert!(
        !paths::project_transition_journal_temp(&bundle.project_id, &bundle.intent_id,).exists()
    );
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(root);
}
