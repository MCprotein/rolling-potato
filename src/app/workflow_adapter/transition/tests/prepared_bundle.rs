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
