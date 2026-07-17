use super::*;

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

#[cfg(unix)]
#[test]
fn prepared_bundle_strictly_binds_semantic_event_chain_plan() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-prepared-event-chain-{}-{}",
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

    let source = prepare_source_install_v1(
        "intent-event-chain",
        "proposal-event-chain",
        &target,
        b"current source\n",
        b"proposed source\n",
    )
    .unwrap();
    let mut bundle = prepare_source_bundle(
        "intent-event-chain",
        Some("workflow-event-chain"),
        source,
        b"current source\n",
        b"proposed source\n",
    )
    .unwrap();
    let identity = crate::app::workflow_adapter::ledger::validated_current_identity().unwrap();
    let events = [
        crate::app::workflow_adapter::ledger::new_event_for(
            &identity,
            "approval.prepared",
            "승인 준비",
            "intent_id=intent-event-chain workflow_id=workflow-event-chain",
        ),
        crate::app::workflow_adapter::ledger::new_event_for(
            &identity,
            "source.installed",
            "소스 설치",
            "intent_id=intent-event-chain workflow_id=workflow-event-chain",
        ),
    ];
    let writer = crate::app::workflow_adapter::ledger::LedgerWriterGuard::acquire().unwrap();
    let planned = writer.plan_events(&events).unwrap();
    bind_planned_events(&mut bundle, &planned).unwrap();

    let body = render_prepared_source_bundle(&bundle).unwrap();
    assert_eq!(parse_prepared_source_bundle(&body).unwrap(), bundle);
    assert_eq!(bundle.semantic_events, events);
    assert_eq!(bundle.event_chain_plan.len(), 2);
    assert_eq!(
        bundle.event_chain_plan[0].ordinal,
        bundle.ledger_binding.event_count + 1
    );
    assert_eq!(
        bundle.event_chain_plan[1].previous_event_hash,
        bundle.event_chain_plan[0].event_hash
    );

    let wrong_ordinal = body.replacen(
        &format!("\"ordinal\":{}", bundle.event_chain_plan[0].ordinal),
        &format!("\"ordinal\":{}", bundle.event_chain_plan[0].ordinal + 1),
        1,
    );
    assert!(parse_prepared_source_bundle(&wrong_ordinal).is_err());
    let wrong_hash = body.replacen(&bundle.event_chain_plan[1].event_hash, &"f".repeat(64), 1);
    assert!(parse_prepared_source_bundle(&wrong_hash).is_err());

    drop(writer);
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn prepared_production_member_array_has_exact_eleven_order_and_lag_index() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-prepared-exact-eleven-{}-{}",
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
    let source = prepare_source_install_v1(
        "intent-exact-eleven",
        "proposal-exact-eleven",
        &target,
        b"current source\n",
        b"proposed source\n",
    )
    .unwrap();
    let mut bundle = prepare_source_bundle(
        "intent-exact-eleven",
        Some("workflow-exact-eleven"),
        source,
        b"current source\n",
        b"proposed source\n",
    )
    .unwrap();
    let identity = crate::app::workflow_adapter::ledger::validated_current_identity().unwrap();
    let events = (0..10)
        .map(|index| {
            crate::app::workflow_adapter::ledger::new_event_for(
                &identity,
                &format!("approval.event.{index}"),
                &format!("approval event {index}"),
                &format!("intent_id=intent-exact-eleven index={index}"),
            )
        })
        .collect::<Vec<_>>();
    let writer = crate::app::workflow_adapter::ledger::LedgerWriterGuard::acquire().unwrap();
    let planned = writer.plan_events(&events).unwrap();
    bind_planned_events(&mut bundle, &planned).unwrap();
    let member = |kind,
                  path: &str,
                  schema_version,
                  artifact_id: &str,
                  causal_id: Option<&str>,
                  event_id: Option<&str>,
                  role| PreparedMember {
        kind,
        path: path.to_string(),
        schema_version,
        binding: PreparedMemberBinding {
            artifact_id: Some(artifact_id.to_string()),
            causal_id: causal_id.map(str::to_string),
            source_key: None,
            event_id: event_id.map(str::to_string),
        },
        bytes_utf8: format!("{{\"artifact\":\"{artifact_id}\"}}"),
        expected_type: "absent".to_string(),
        expected_identity: None,
        readonly: false,
        mode: 0o600,
        ownership: None,
        semantic_role_rank: role,
    };
    let e1 = events[1].event_id.as_str();
    let e9 = events[9].event_id.as_str();
    let lag = prepare_projection_lag_member("intent-exact-eleven", &planned).unwrap();
    let members = vec![
        lag,
        member(
            PreparedMemberKind::WorkflowPointer,
            ".rpotato/workflows/workflow-exact-eleven.json",
            4,
            "pointer-r2",
            Some("snapshot-r2"),
            Some(e9),
            1,
        ),
        member(
            PreparedMemberKind::ToolOutput,
            "state/tool-output/project/session/workflow/tool.json",
            1,
            "tool-exact-eleven",
            None,
            Some(events[7].event_id.as_str()),
            0,
        ),
        member(
            PreparedMemberKind::CurrentImage,
            "state/current-state.json",
            2,
            "current-exact-eleven",
            Some("snapshot-r2"),
            Some(e9),
            0,
        ),
        member(
            PreparedMemberKind::WorkflowSnapshot,
            ".rpotato/workflows/workflow-exact-eleven.snapshots/00000000000000000002.json",
            4,
            "snapshot-r1",
            None,
            Some(e1),
            0,
        ),
        member(
            PreparedMemberKind::TranscriptV2,
            "state/transcripts/project/session/transcript.json",
            2,
            "transcript-exact-eleven",
            Some("tool-exact-eleven"),
            Some(events[8].event_id.as_str()),
            0,
        ),
        member(
            PreparedMemberKind::WorkflowPointer,
            ".rpotato/workflows/workflow-exact-eleven.json",
            4,
            "pointer-r1",
            Some("snapshot-r1"),
            Some(e1),
            0,
        ),
        member(
            PreparedMemberKind::WorkflowSnapshot,
            ".rpotato/workflows/workflow-exact-eleven.snapshots/00000000000000000003.json",
            4,
            "snapshot-r2",
            None,
            Some(e9),
            1,
        ),
    ];
    bind_additional_members(&mut bundle, members).unwrap();

    let body = render_prepared_source_bundle(&bundle).unwrap();
    assert_eq!(parse_prepared_source_bundle(&body).unwrap(), bundle);
    assert_eq!(bundle.additional_members.len() + 3, 11);
    assert_eq!(bundle.projection_lag_member_index, Some(10));
    assert_eq!(body.matches("\"member_kind\"").count(), 12);
    assert!(body.ends_with(
        "\"projection_lag_v1\":{\"member_kind\":\"projection_lag\",\"member_index\":10}}"
    ));

    let wrong_index = body.replacen("\"member_index\":10", "\"member_index\":9", 1);
    assert!(parse_prepared_source_bundle(&wrong_index).is_err());
    let wrong_shared_path = body.replacen(
        ".rpotato/workflows/workflow-exact-eleven.json",
        ".rpotato/workflows/other.json",
        1,
    );
    assert!(parse_prepared_source_bundle(&wrong_shared_path).is_err());

    drop(writer);
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(root);
}
