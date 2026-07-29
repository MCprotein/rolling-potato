#[test]
fn recovery_rejects_and_preserves_unknown_lock_candidates() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-transition-lock-candidates-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let project_root = root.join("project");
    let data_home = root.join("data");
    fs::create_dir_all(&project_root).unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
    std::env::set_var("RPOTATO_DATA_HOME", &data_home);
    crate::app::workflow_adapter::state::initialize().unwrap();
    let project_id = crate::app::workflow_adapter::ledger::validated_current_identity()
        .unwrap()
        .project_id;
    let transition_guard = TransitionGuard::acquire(&project_id).unwrap();
    let directory = paths::project_transition_journal_dir(&project_id);
    let malformed = directory.join("transition.candidate.1.2");
    fs::write(&malformed, b"").unwrap();
    let error = recover_pending_bundles_under_guard(&project_id).unwrap_err();
    assert!(error.message.contains("unknown transition journal entry"));
    assert!(malformed.exists());

    drop(transition_guard);
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn recovery_enforces_file_and_directory_read_bounds_before_parsing() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-transition-recovery-bounds-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let project_root = root.join("project");
    let data_home = root.join("data");
    fs::create_dir_all(&project_root).unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
    std::env::set_var("RPOTATO_DATA_HOME", &data_home);
    crate::app::workflow_adapter::state::initialize().unwrap();
    let project_id = crate::app::workflow_adapter::ledger::validated_current_identity()
        .unwrap()
        .project_id;
    let transition_guard = TransitionGuard::acquire(&project_id).unwrap();
    let directory = paths::project_transition_journal_dir(&project_id);

    for index in 0..MAX_RECOVERY_JOURNAL_ENTRIES {
        fs::write(
            directory.join(format!("intent-bound-{index}.prepared.json")),
            b"{}",
        )
        .unwrap();
    }
    let entry_error = recover_pending_bundles_under_guard(&project_id).unwrap_err();
    assert!(entry_error
        .message
        .contains("transition journal recovery bound"));

    for index in 0..MAX_RECOVERY_JOURNAL_ENTRIES {
        fs::remove_file(directory.join(format!("intent-bound-{index}.prepared.json"))).unwrap();
    }
    let oversized = directory.join("intent-oversized.prepared.json");
    fs::write(&oversized, vec![b'x'; MAX_PREPARED_BUNDLE_BYTES + 1]).unwrap();
    let byte_error = recover_pending_bundles_under_guard(&project_id).unwrap_err();
    assert!(byte_error.message.contains("regular-file/byte budget"));

    fs::remove_file(oversized).unwrap();
    let lag_directory = paths::projection_lag_dir();
    fs::create_dir_all(&lag_directory).unwrap();
    let oversized_lag = lag_directory.join("oversized.json");
    fs::write(&oversized_lag, vec![b'x'; MAX_PROJECTION_LAG_BYTES + 1]).unwrap();
    let lag_error = recover_pending_bundles_under_guard(&project_id).unwrap_err();
    assert!(lag_error.message.contains("projection lag recovery bound"));

    assert!(oversized_lag.exists());
    drop(transition_guard);
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn recovery_discovery_treats_oversized_project_root_as_suspicious() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-transition-project-discovery-bound-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let project_root = root.join("project");
    let data_home = root.join("data");
    fs::create_dir_all(&project_root).unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
    std::env::set_var("RPOTATO_DATA_HOME", &data_home);
    crate::app::workflow_adapter::state::initialize().unwrap();
    let journal_root = paths::project_state_dir().join("transition-journal");
    for index in 0..=MAX_RECOVERY_PROJECT_ENTRIES {
        fs::create_dir_all(journal_root.join(format!("empty-project-{index}"))).unwrap();
    }

    assert!(recovery_work_may_exist());

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn bounded_recovery_file_read_rejects_oversized_bytes() {
    let path = std::env::temp_dir().join(format!(
        "rpotato-transition-bounded-read-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::write(&path, vec![b'x'; 65]).unwrap();

    let error = read_regular_utf8_bounded(&path, 64, "bounded fixture").unwrap_err();

    assert!(error.message.contains("regular-file/byte budget"));
    let _ = fs::remove_file(path);
}

#[test]
fn projection_lag_member_full_bytes_golden_is_independent() {
    let planned = (0_u64..10)
        .map(|index| crate::app::workflow_adapter::ledger::PlannedEvent {
            event: crate::app::workflow_adapter::ledger::LedgerEvent {
                event_id: format!("event-{index}"),
                ts_ms: u128::from(index),
                event_type: "approval.event".to_string(),
                project_id: "project-golden".to_string(),
                session_id: "session-golden".to_string(),
                summary: "golden".to_string(),
                details: format!("index={index}"),
            },
            ordinal: index + 1,
            previous_event_hash: "0".repeat(64),
            event_hash: if index == 9 {
                "a".repeat(64)
            } else {
                "0".repeat(64)
            },
        })
        .collect::<Vec<_>>();

    let member = prepare_projection_lag_member("intent-golden", &planned).unwrap();

    assert_eq!(
        member.bytes_utf8.as_bytes(),
        b"{\"schema_version\":1,\"intent_id\":\"intent-golden\",\"event_id\":\"event-9\",\"event_ordinal\":10,\"event_hash\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"required_outputs\":[\"project-session-ledger\",\"global-operation-log\",\"sqlite\"],\"required_event_ids\":[\"event-0\",\"event-1\",\"event-2\",\"event-3\",\"event-4\",\"event-5\",\"event-6\",\"event-7\",\"event-8\",\"event-9\"]}"
    );
    assert_eq!(member.binding.event_id.as_deref(), Some("event-9"));
    assert_eq!(
        member.path,
        "state/projection-lag/intent-golden-event-9.json"
    );
}

#[test]
fn transition_component_byte_caps_accept_limit_and_reject_limit_plus_one() {
    for (label, limit) in [
        ("before-blob", MAX_SOURCE_BLOB_BYTES),
        ("proposed-blob", MAX_SOURCE_BLOB_BYTES),
        ("tool-output", 262_144),
        ("transcript-v2", 131_072),
        ("workflow-snapshot", 65_536),
        ("workflow-pointer", 16_384),
        ("current-image", 65_536),
        ("semantic-event", MAX_PREPARED_EVENT_BYTES),
        ("semantic-events", MAX_PREPARED_EVENTS_BYTES),
        ("projection-lag", 4_096),
        ("source-install-v1", MAX_SOURCE_INSTALL_BYTES),
        ("full-journal", MAX_PREPARED_BUNDLE_BYTES),
    ] {
        assert!(
            enforce_byte_limit(limit - 1, limit, "limit exceeded").is_ok(),
            "{label} limit-1"
        );
        assert!(
            enforce_byte_limit(limit, limit, "limit exceeded").is_ok(),
            "{label} limit"
        );
        assert!(
            enforce_byte_limit(limit + 1, limit, "limit exceeded").is_err(),
            "{label} limit+1"
        );
    }
    assert!(checked_add_bytes(
        usize::MAX,
        1,
        MAX_PREPARED_BUNDLE_BYTES,
        "overflow",
        "limit exceeded",
    )
    .unwrap_err()
    .message
    .contains("overflow"));
    let multibyte = "가".repeat((MAX_PREPARED_EVENT_BYTES / 3) + 1);
    assert!(multibyte.chars().count() < MAX_PREPARED_EVENT_BYTES);
    assert!(
        enforce_byte_limit(multibyte.len(), MAX_PREPARED_EVENT_BYTES, "limit exceeded").is_err()
    );
}

#[test]
fn source_identity_v1_matches_normative_golden() {
    let hash = "473b0fef5f0626d3fe806f10b931f085d511ba15b1117c53d5f2ec27d5b9452e";
    assert_eq!(sha256_bytes(b"current source\n"), hash);
    assert_eq!(
        source_identity_v1(0x0102_0304_0506_0708, 0x1112_1314_1516_1718, hash).unwrap(),
        "2b3452be6ffa18621fcd39e56162e5b46ef9428657dd6cdc9e02847e521420d0"
    );
    assert!(source_identity_v1(
        0x0102_0304_0506_0708,
        0x1112_1314_1516_1718,
        &hash.to_ascii_uppercase()
    )
    .is_err());
}
