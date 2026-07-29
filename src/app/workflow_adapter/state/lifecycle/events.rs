use super::*;

pub fn record_event(event_type: &str, summary: &str, details: &str) -> Result<String, AppError> {
    ensure_layout()?;
    if !paths::current_state_file().exists() {
        initialize()?;
    }
    let identity = ledger::validated_current_identity()?;
    let event = ledger::new_event_for(&identity, event_type, summary, details);
    let event_id = event.event_id.clone();
    let active_workflow = read_valid_current_for_transition()?
        .and_then(|snapshot| snapshot.active_workflow)
        .map(|binding| binding.workflow_id);
    let intent_id = internal_transition_intent_id(&event);
    commit_state_event(
        &intent_id,
        transition::CurrentStateIntent::RecordEvent,
        &identity,
        &event,
        None,
        active_workflow.as_deref(),
        CompactionBoundaryCommit::preserve(),
    )?;
    Ok(event_id)
}

pub(crate) fn current_compaction_boundary(session_id: &str) -> Result<Option<String>, AppError> {
    let identity = ledger::validated_current_identity()?;
    let Some(snapshot) = read_valid_current_for_transition()? else {
        return Ok(None);
    };
    if snapshot.project_id != identity.project_id || snapshot.session_id != session_id {
        return Err(AppError::blocked(
            "compaction boundary current project/session binding 불일치",
        ));
    }
    Ok(snapshot.compaction_boundary)
}

pub(crate) fn record_compaction_boundary(
    artifact_path: &str,
    artifact_hash: &str,
    boundary_record_id: &str,
    expected_previous_artifact_path: Option<String>,
) -> Result<String, AppError> {
    ensure_layout()?;
    if !paths::current_state_file().exists() {
        initialize()?;
    }
    let identity = ledger::validated_current_identity()?;
    let expected_prefix = format!(
        "state/compactions/{}/{}/",
        identity.project_id, identity.session_id
    );
    if !artifact_path.starts_with(&expected_prefix)
        || !artifact_path.ends_with(".json")
        || artifact_path
            .split('/')
            .any(|part| part.is_empty() || part == "..")
        || artifact_path.bytes().any(|byte| byte.is_ascii_whitespace())
        || artifact_hash.len() != 64
        || !artifact_hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || boundary_record_id.is_empty()
        || boundary_record_id
            .bytes()
            .any(|byte| byte.is_ascii_whitespace())
    {
        return Err(AppError::blocked(
            "compaction boundary artifact path/hash/record binding 불일치",
        ));
    }
    let event = ledger::new_event_for(
        &identity,
        "context.compacted",
        "context compaction checkpoint committed",
        &format!(
            "artifact_path={artifact_path} artifact_hash={artifact_hash} boundary_record_id={boundary_record_id}"
        ),
    );
    let event_id = event.event_id.clone();
    let active_workflow = read_valid_current_for_transition()?
        .and_then(|snapshot| snapshot.active_workflow)
        .map(|binding| binding.workflow_id);
    let intent_id = internal_transition_intent_id(&event);
    commit_state_event(
        &intent_id,
        transition::CurrentStateIntent::RecordEvent,
        &identity,
        &event,
        Some("context-compaction"),
        active_workflow.as_deref(),
        CompactionBoundaryCommit::set(artifact_path, expected_previous_artifact_path.as_deref()),
    )?;
    Ok(event_id)
}
