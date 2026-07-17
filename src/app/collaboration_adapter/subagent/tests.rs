use super::*;

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn launch(role: &str) -> ValidatedLaunch {
    let tools = if role == "executor" {
        strings(&["read_file", "render_diff"])
    } else {
        strings(&["read_file"])
    };
    let writes = if role == "executor" {
        strings(&["src/subagent.rs"])
    } else {
        Vec::new()
    };
    validate_launch(
        role,
        "bounded task",
        &tools,
        &strings(&["src/main.rs"]),
        &writes,
        None,
        None,
    )
    .unwrap()
}

fn record(role: &str) -> SubagentRecordV1 {
    SubagentRecordV1::new(
        "project-test",
        "session-test",
        "workflow-test",
        1,
        &"a".repeat(64),
        launch(role),
    )
    .unwrap()
}

fn initialize_parent() -> state::WorkflowRecord {
    fs::create_dir_all(paths::project_root().join("src")).unwrap();
    fs::write(paths::project_root().join("src/main.rs"), "fn main() {}\n").unwrap();
    state::initialize().unwrap();
    state::create_workflow("subagent parent fixture").unwrap()
}

fn completed_result(record: &SubagentRecordV1, context: &crate::context::ContextPack) -> String {
    let evidence_ref = &context.source_pointers[0].stable_ref;
    format!(
        "{{\"schema_version\":1,\"subagent_id\":\"{}\",\"parent_workflow_id\":\"{}\",\"role\":\"{}\",\"status\":\"completed\",\"summary\":\"검증된 결과\",\"findings\":[\"선언된 파일을 확인했습니다.\"],\"patch_proposal\":null,\"evidence_refs\":[\"{}\"],\"validation_gaps\":[],\"suggested_next_action\":\"부모 작업을 계속합니다.\"}}",
        record.subagent_id,
        record.parent_workflow_id,
        record.role.as_str(),
        evidence_ref,
    )
}

#[test]
fn launch_contract_enforces_role_tool_and_write_boundaries() {
    let error = validate_launch(
        "explore",
        "task",
        &strings(&["read_file", "render_diff"]),
        &strings(&["src/main.rs"]),
        &strings(&["src/main.rs"]),
        None,
        None,
    )
    .unwrap_err();
    assert!(error.message.contains("role/tool policy"));

    let error = validate_launch(
        "executor",
        "task",
        &strings(&["read_file", "render_diff"]),
        &strings(&["src/main.rs"]),
        &[],
        None,
        None,
    )
    .unwrap_err();
    assert!(error.message.contains("함께 선언"));

    let error = validate_launch(
        "executor",
        "task",
        &strings(&["read_file", "render_diff"]),
        &strings(&["src/main.rs"]),
        &strings(&["README.md"]),
        None,
        None,
    )
    .unwrap_err();
    assert!(error.message.contains("declared read target"));
}

#[test]
fn launch_contract_enforces_exact_task_and_budget_bounds() {
    validate_launch(
        "explore",
        &"x".repeat(MAX_TASK_BYTES),
        &strings(&["read_file"]),
        &strings(&["src/main.rs"]),
        &[],
        Some(MAX_CHAT_TIMEOUT_MS),
        Some(MAX_MAX_TOKENS),
    )
    .unwrap();
    for error in [
        validate_launch(
            "explore",
            &"x".repeat(MAX_TASK_BYTES + 1),
            &strings(&["read_file"]),
            &strings(&["src/main.rs"]),
            &[],
            None,
            None,
        )
        .unwrap_err(),
        validate_launch(
            "explore",
            "task",
            &strings(&["read_file"]),
            &strings(&["src/main.rs"]),
            &[],
            Some(0),
            None,
        )
        .unwrap_err(),
        validate_launch(
            "explore",
            "task",
            &strings(&["read_file"]),
            &strings(&["src/main.rs"]),
            &[],
            None,
            Some(MAX_MAX_TOKENS + 1),
        )
        .unwrap_err(),
    ] {
        assert_eq!(error.code, 2);
    }
}

#[test]
fn launch_contract_rejects_traversal_duplicates_and_excess_paths() {
    for paths in [
        strings(&["../secret"]),
        strings(&["src/main.rs", "src/main.rs"]),
        strings(&["a", "b", "c", "d", "e"]),
        strings(&["C:\\secret"]),
    ] {
        let error = validate_launch(
            "explore",
            "task",
            &strings(&["read_file"]),
            &paths,
            &[],
            None,
            None,
        )
        .unwrap_err();
        assert!(matches!(error.code, 2 | 3));
    }
}

#[test]
fn lifecycle_transition_matrix_is_closed() {
    let terminal = [
        SubagentStatus::Completed,
        SubagentStatus::Blocked,
        SubagentStatus::Failed,
        SubagentStatus::Cancelled,
        SubagentStatus::TimedOut,
    ];
    assert!(SubagentStatus::Requested.permits(SubagentStatus::Admitted));
    assert!(SubagentStatus::Admitted.permits(SubagentStatus::Running));
    for status in terminal {
        assert!(SubagentStatus::Running.permits(status));
        assert!(!status.permits(SubagentStatus::Requested));
    }
    assert!(!SubagentStatus::Requested.permits(SubagentStatus::Running));
    assert!(!SubagentStatus::Admitted.permits(SubagentStatus::Completed));
}

#[test]
fn canonical_state_round_trips_and_preserves_hash_chain() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let requested = create_record(record("explore")).unwrap();
    let mut admitted = requested.clone();
    admitted
        .transition_to(SubagentStatus::Admitted, None)
        .unwrap();
    let admitted = checkpoint_record(admitted, requested.revision).unwrap();
    let mut running = admitted.clone();
    running
        .transition_to(SubagentStatus::Running, None)
        .unwrap();
    let running = checkpoint_record(running, admitted.revision).unwrap();
    let mut completed = running.clone();
    completed.backend_event_id = "backend-event-test".to_string();
    completed.result_artifact_id = "result-test".to_string();
    completed.result_artifact_hash = "b".repeat(64);
    completed.evidence_id = "evidence-test".to_string();
    completed.evidence_hash = "c".repeat(64);
    completed
        .transition_to(SubagentStatus::Completed, None)
        .unwrap();
    let completed = checkpoint_record(completed, running.revision).unwrap();
    assert_eq!(completed.revision, 4);
    assert_eq!(load_record(&completed.subagent_id).unwrap(), completed);
    for revision in 1..=4 {
        assert!(paths::project_subagent_snapshot_file(&completed.subagent_id, revision).is_file());
    }
}

#[test]
fn stale_revision_and_immutable_binding_changes_fail_closed() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let requested = create_record(record("explore")).unwrap();
    let mut admitted = requested.clone();
    admitted
        .transition_to(SubagentStatus::Admitted, None)
        .unwrap();
    let admitted = checkpoint_record(admitted, requested.revision).unwrap();

    let mut stale = requested.clone();
    stale
        .transition_to(SubagentStatus::Cancelled, Some("user-cancelled"))
        .unwrap();
    assert!(checkpoint_record(stale, requested.revision)
        .unwrap_err()
        .message
        .contains("stale revision"));

    let mut forged = admitted.clone();
    forged.parent_workflow_id = "workflow-other".to_string();
    forged.transition_to(SubagentStatus::Running, None).unwrap();
    assert!(checkpoint_record(forged, admitted.revision)
        .unwrap_err()
        .message
        .contains("immutable"));
}

#[test]
fn tampered_current_or_snapshot_state_is_rejected() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let requested = create_record(record("explore")).unwrap();
    let current = paths::project_subagent_file(&requested.subagent_id);
    let original = fs::read_to_string(&current).unwrap();
    fs::write(&current, original.replace("requested", "admitted")).unwrap();
    assert!(load_record(&requested.subagent_id).is_err());

    fs::write(&current, &original).unwrap();
    let snapshot = paths::project_subagent_snapshot_file(&requested.subagent_id, 1);
    fs::write(&snapshot, original.replace("project-test", "project-evil")).unwrap();
    assert!(load_record(&requested.subagent_id).is_err());
}

#[test]
fn conflicting_preinstalled_snapshot_blocks_checkpoint() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let pending = record("explore");
    let path = paths::project_subagent_snapshot_file(&pending.subagent_id, 1);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "forged").unwrap();
    assert!(create_record(pending)
        .unwrap_err()
        .message
        .contains("snapshot 충돌"));
}

#[test]
fn admission_binds_active_parent_and_records_ordered_events() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let parent = initialize_parent();
    let admitted = admit_launch(launch("explore")).unwrap();
    let admitted = admitted.record;
    assert_eq!(admitted.status, SubagentStatus::Admitted);
    assert_eq!(admitted.revision, 2);
    assert_eq!(admitted.project_id, parent.project_id);
    assert_eq!(admitted.session_id, parent.session_id);
    assert_eq!(admitted.parent_workflow_id, parent.workflow_id);
    assert_eq!(admitted.parent_revision, parent.revision);
    assert_eq!(admitted.parent_artifact_hash, parent.artifact_hash);

    let lifecycle = ledger::read_runtime_events()
        .unwrap()
        .into_iter()
        .filter(|event| event.event_type.starts_with("team.subagent."))
        .map(|event| event.event_type)
        .collect::<Vec<_>>();
    assert_eq!(
        lifecycle,
        vec![
            "team.subagent.requested".to_string(),
            "team.subagent.admitted".to_string(),
        ]
    );
}

#[test]
fn admission_requires_parent_and_blocks_second_non_terminal_child() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    state::initialize().unwrap();
    assert!(admit_launch(launch("explore"))
        .unwrap_err()
        .message
        .contains("active non-terminal parent"));

    fs::create_dir_all(paths::project_root().join("src")).unwrap();
    fs::write(paths::project_root().join("src/main.rs"), "fn main() {}\n").unwrap();
    state::create_workflow("subagent parent fixture").unwrap();
    let first = admit_launch(launch("explore")).unwrap().record;
    let error = admit_launch(launch("planner")).unwrap_err();
    assert!(error.message.contains("non-terminal child"));
    assert_eq!(
        records_for_parent(&first.parent_workflow_id).unwrap().len(),
        1
    );
}

#[test]
fn admission_rejects_terminal_parent() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let parent = initialize_parent();
    let mut terminal = parent.clone();
    terminal.phase = "complete".to_string();
    let terminal = state::checkpoint_workflow(terminal, parent.revision).unwrap();
    assert!(terminal.is_terminal());
    let error = admit_launch(launch("explore")).unwrap_err();
    assert!(error.message.contains("active non-terminal 상태"));
    assert!(records_for_parent(&parent.workflow_id).unwrap().is_empty());
}

#[test]
fn status_defaults_to_active_parent_and_cancel_is_idempotent() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    initialize_parent();
    let admitted = admit_launch(launch("explore")).unwrap().record;
    let status = status_report(None).unwrap();
    assert!(status.contains(&admitted.subagent_id));
    assert!(status.contains("status: admitted"));

    let cancelled_report = cancel_report(&admitted.subagent_id).unwrap();
    assert!(cancelled_report.contains("action: cancelled"));
    let cancelled = load_record(&admitted.subagent_id).unwrap();
    assert_eq!(cancelled.status, SubagentStatus::Cancelled);
    assert_eq!(cancelled.revision, 3);

    let retry = cancel_report(&admitted.subagent_id).unwrap();
    assert!(retry.contains("already-cancelled-no-op"));
    assert_eq!(load_record(&admitted.subagent_id).unwrap().revision, 3);
}

#[test]
fn dispatch_completes_and_merges_evidence_once() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let parent = initialize_parent();
    let admitted = admit_launch(launch("explore")).unwrap();
    let response = completed_result(&admitted.record, &admitted.context);
    let completed = dispatch_admitted(admitted, "bounded task", true, |prompt, max, timeout| {
        assert!(prompt.contains("canonical compact JSON"));
        assert_eq!(max, DEFAULT_MAX_TOKENS);
        assert_eq!(timeout, DEFAULT_TIMEOUT_MS);
        Ok(WorkerGeneration {
            backend_event_id: "backend-event-test".to_string(),
            effective_max_tokens: 128,
            response,
        })
    })
    .unwrap();
    assert_eq!(completed.record.status, SubagentStatus::Completed);
    assert_eq!(completed.record.revision, 4);
    assert_eq!(completed.record.effective_max_tokens, 128);
    assert!(!completed.record.result_artifact_id.is_empty());
    assert!(!completed.record.evidence_id.is_empty());
    assert!(paths::project_subagent_result_file(&completed.record.result_artifact_id).is_file());
    assert!(paths::project_evidence_dir()
        .join(format!("{}.json", completed.record.evidence_id))
        .is_file());
    let merged_parent = state::load_workflow(&parent.workflow_id).unwrap();
    assert_eq!(merged_parent.revision, parent.revision + 1);
    assert_eq!(merged_parent.skill_evidence, completed.record.evidence_id);
    assert_eq!(
        ledger::read_runtime_events()
            .unwrap()
            .into_iter()
            .filter(|event| event.event_type.starts_with("team.subagent."))
            .map(|event| event.event_type)
            .collect::<Vec<_>>(),
        vec![
            "team.subagent.requested",
            "team.subagent.admitted",
            "team.subagent.started",
            "team.subagent.completed",
            "team.subagent.result-merged",
        ]
    );
    merge_completed_result(&completed.record).unwrap();
    assert_eq!(
        ledger::read_runtime_events()
            .unwrap()
            .into_iter()
            .filter(|event| event.event_type == "team.subagent.result-merged")
            .count(),
        1
    );
}

#[test]
fn admission_recovers_merge_interrupted_after_parent_checkpoint() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let parent = initialize_parent();
    let admitted = admit_launch(launch("explore")).unwrap();
    let (running, context) = prepare_running(&admitted).unwrap();
    let body = completed_result(&running, &context);
    let stored = crate::app::collaboration_adapter::subagent_result::parse_and_store(
        &running, &context, &body,
    )
    .unwrap();
    crate::app::collaboration_adapter::subagent_result::verify_stored_artifacts(&running, &stored)
        .unwrap();

    let mut completed = running.clone();
    completed.backend_event_id = "backend-event-interrupted".to_string();
    completed.effective_max_tokens = 128;
    completed.result_artifact_id = stored.result_artifact_id;
    completed.result_artifact_hash = stored.result_artifact_hash;
    completed.evidence_id = stored.evidence_id;
    completed.evidence_hash = stored.evidence_hash;
    completed
        .transition_to(SubagentStatus::Completed, None)
        .unwrap();
    let completed = checkpoint_record(completed, running.revision).unwrap();

    let mut interrupted_parent = parent.clone();
    interrupted_parent.skill_evidence = completed.evidence_id.clone();
    let interrupted_parent =
        state::checkpoint_workflow(interrupted_parent, parent.revision).unwrap();
    assert_eq!(
        ledger::read_runtime_events()
            .unwrap()
            .into_iter()
            .filter(|event| event.event_type == "team.subagent.result-merged")
            .count(),
        0
    );

    let next = admit_launch(launch("planner")).unwrap();
    assert_eq!(next.record.parent_revision, interrupted_parent.revision);
    assert_eq!(
        state::load_workflow(&parent.workflow_id).unwrap(),
        interrupted_parent
    );
    assert_eq!(
        ledger::read_runtime_events()
            .unwrap()
            .into_iter()
            .filter(|event| event.event_type == "team.subagent.result-merged")
            .count(),
        1
    );

    merge_completed_result(&completed).unwrap();
    assert_eq!(
        ledger::read_runtime_events()
            .unwrap()
            .into_iter()
            .filter(|event| event.event_type == "team.subagent.result-merged")
            .count(),
        1
    );
}

#[test]
fn dispatch_blocks_invalid_result_without_parent_merge() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let parent = initialize_parent();
    let admitted = admit_launch(launch("explore")).unwrap();
    let subagent_id = admitted.record.subagent_id.clone();
    let error = dispatch_admitted(admitted, "bounded task", true, |_, _, _| {
        Ok(WorkerGeneration {
            backend_event_id: "backend-event-invalid".to_string(),
            effective_max_tokens: 128,
            response: "{}".to_string(),
        })
    })
    .unwrap_err();
    assert!(error.message.contains("result 검증 차단"));
    let blocked = load_record(&subagent_id).unwrap();
    assert_eq!(blocked.status, SubagentStatus::Blocked);
    assert_eq!(blocked.failure_code, "invalid-result");
    assert_eq!(state::load_workflow(&parent.workflow_id).unwrap(), parent);
}

#[test]
fn dispatch_timeout_discards_partial_output_and_records_timed_out() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    initialize_parent();
    let admitted = admit_launch(launch("explore")).unwrap();
    let subagent_id = admitted.record.subagent_id.clone();
    let error = dispatch_admitted(admitted, "bounded task", true, |_, _, _| {
        Err(AppError::runtime(
            "backend chat 중단: 제한 시간 초과로 취소됨",
        ))
    })
    .unwrap_err();
    assert!(error.message.contains("partial output: discarded"));
    let timed_out = load_record(&subagent_id).unwrap();
    assert_eq!(timed_out.status, SubagentStatus::TimedOut);
    assert_eq!(timed_out.failure_code, "backend-timeout");
    assert!(timed_out.result_artifact_id.is_empty());
}

#[test]
fn dispatch_resource_denial_records_blocked_without_result_or_parent_merge() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let parent = initialize_parent();
    let admitted = admit_launch(launch("explore")).unwrap();
    let subagent_id = admitted.record.subagent_id.clone();
    let error = dispatch_admitted(admitted, "bounded task", true, |_, _, _| {
        Err(AppError::blocked(
            "backend chat resource governor 차단: critical pressure",
        ))
    })
    .unwrap_err();
    assert!(error.message.contains("resource governor"));
    let blocked = load_record(&subagent_id).unwrap();
    assert_eq!(blocked.status, SubagentStatus::Blocked);
    assert_eq!(blocked.failure_code, "backend-blocked");
    assert!(blocked.backend_event_id.is_empty());
    assert!(blocked.result_artifact_id.is_empty());
    assert!(blocked.evidence_id.is_empty());
    assert_eq!(state::load_workflow(&parent.workflow_id).unwrap(), parent);
    assert!(ledger::read_runtime_events()
        .unwrap()
        .iter()
        .any(|event| event.event_type == "team.subagent.blocked"));
}

#[test]
fn manual_cancel_wins_before_backend_completion_merge() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let parent = initialize_parent();
    let admitted = admit_launch(launch("explore")).unwrap();
    let subagent_id = admitted.record.subagent_id.clone();
    let response = completed_result(&admitted.record, &admitted.context);
    let error = dispatch_admitted(admitted, "bounded task", true, |_, _, _| {
        let report = cancel_report(&subagent_id).unwrap();
        assert!(report.contains("action: cancelled"));
        Ok(WorkerGeneration {
            backend_event_id: "backend-event-after-cancel".to_string(),
            effective_max_tokens: 128,
            response,
        })
    })
    .unwrap_err();
    assert!(error.message.contains("cancellation이 먼저"));
    let cancelled = load_record(&subagent_id).unwrap();
    assert_eq!(cancelled.status, SubagentStatus::Cancelled);
    assert!(cancelled.result_artifact_id.is_empty());
    assert_eq!(state::load_workflow(&parent.workflow_id).unwrap(), parent);
}

#[test]
fn stale_parent_or_context_blocks_completion_without_merge() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let parent = initialize_parent();
    let admitted = admit_launch(launch("explore")).unwrap();
    let subagent_id = admitted.record.subagent_id.clone();
    let response = completed_result(&admitted.record, &admitted.context);
    let mut changed_parent = parent.clone();
    changed_parent.result_summary = "parent changed".to_string();
    let error = dispatch_admitted(admitted, "bounded task", true, |_, _, _| {
        state::checkpoint_workflow(changed_parent, parent.revision).unwrap();
        Ok(WorkerGeneration {
            backend_event_id: "backend-event-stale-parent".to_string(),
            effective_max_tokens: 128,
            response,
        })
    })
    .unwrap_err();
    assert!(error.message.contains("stale parent"));
    assert_eq!(
        load_record(&subagent_id).unwrap().failure_code,
        "stale-parent"
    );

    let current_parent = state::load_workflow(&parent.workflow_id).unwrap();
    let admitted = admit_launch(launch("explore")).unwrap();
    let subagent_id = admitted.record.subagent_id.clone();
    let response = completed_result(&admitted.record, &admitted.context);
    let error = dispatch_admitted(admitted, "bounded task", true, |_, _, _| {
        fs::write(
            paths::project_root().join("src/main.rs"),
            "fn main() { changed(); }\n",
        )
        .unwrap();
        Ok(WorkerGeneration {
            backend_event_id: "backend-event-stale-context".to_string(),
            effective_max_tokens: 128,
            response,
        })
    })
    .unwrap_err();
    assert!(error.message.contains("source binding"));
    assert_eq!(
        load_record(&subagent_id).unwrap().failure_code,
        "stale-context"
    );
    assert_eq!(
        state::load_workflow(&parent.workflow_id).unwrap(),
        current_parent
    );
}

#[test]
fn stale_running_child_recovers_as_failed_without_backend_replay() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    initialize_parent();
    let admitted = admit_launch(launch("explore")).unwrap().record;
    let mut running = admitted.clone();
    running
        .transition_to(SubagentStatus::Running, None)
        .unwrap();
    let running = checkpoint_record(running, admitted.revision).unwrap();

    let replacement = admit_launch(launch("planner")).unwrap().record;
    let recovered = load_record(&running.subagent_id).unwrap();
    assert_eq!(recovered.status, SubagentStatus::Failed);
    assert_eq!(recovered.failure_code, "interrupted-no-replay");
    assert_eq!(replacement.status, SubagentStatus::Admitted);
    assert_ne!(replacement.subagent_id, recovered.subagent_id);
}
