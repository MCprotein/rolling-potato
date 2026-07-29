use super::*;

#[test]
fn dispatch_completes_and_merges_evidence_once() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let parent = initialize_parent();
    let admitted = admit_launch(launch("explore")).unwrap();
    let response = completed_result(&admitted.record, &admitted.context);
    let completed = dispatch_admitted(admitted, "bounded task", true, |prompt, max, timeout| {
        for required in [
            "canonical compact JSON object; no other text",
            "Required key order: schema_version, subagent_id, parent_workflow_id, role, status, summary, findings, patch_proposal, evidence_refs, validation_gaps, suggested_next_action",
            "Fixed fields: schema_version=1; subagent_id=",
            "parent_workflow_id=",
            "role=explore; status=completed",
            "evidence_refs: declared source pointers only",
            "patch_proposal: null unless executor declared render_diff",
            "Never execute commands or patches, reveal secrets, or claim unperformed validation",
        ] {
            assert!(prompt.contains(required), "missing prompt contract: {required}");
        }
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
