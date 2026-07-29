use super::super::*;

pub(crate) fn prepare_state_transition_bundle(
    intent_id: &str,
    intent: CurrentStateIntent,
    identity: &crate::app::workflow_adapter::ledger::RuntimeIdentity,
    workflow_id: Option<&str>,
    current_revision: u64,
    current_artifact_hash: &str,
    ledger_binding: crate::app::workflow_adapter::ledger::LedgerBinding,
) -> Result<PreparedSourceBundle, AppError> {
    validate_ascii_id(intent_id, "intent")?;
    validate_ascii_id(&identity.project_id, "project")?;
    validate_ascii_id(&identity.session_id, "session")?;
    if let Some(workflow_id) = workflow_id {
        validate_ascii_id(workflow_id, "workflow")?;
    }
    let intent_kind = intent.as_str();
    if !is_state_transition_intent_kind(intent_kind) {
        return Err(AppError::blocked(
            "prepared state transition intent kind 불일치",
        ));
    }
    Ok(PreparedSourceBundle {
        intent_id: intent_id.to_string(),
        intent_kind: intent_kind.to_string(),
        project_id: identity.project_id.clone(),
        session_id: identity.session_id.clone(),
        workflow_id: workflow_id.map(str::to_string),
        prepared_at_ms: now_ms(),
        current_revision,
        current_artifact_hash: current_artifact_hash.to_string(),
        ledger_binding,
        source_install: None,
        before_bytes: None,
        proposed_bytes: None,
        additional_members: Vec::new(),
        semantic_events: Vec::new(),
        event_chain_plan: Vec::new(),
        projection_lag_member_index: None,
    })
}

pub(crate) fn prepare_source_bundle(
    intent_id: &str,
    workflow_id: Option<&str>,
    source_install: SourceInstallV1,
    before: &[u8],
    proposed: &[u8],
) -> Result<PreparedSourceBundle, AppError> {
    let identity = crate::app::workflow_adapter::ledger::validated_current_identity()?;
    let lease = crate::app::workflow_adapter::state::current_state_lease_view()?;
    let ledger_binding = crate::app::workflow_adapter::ledger::validated_ledger_binding()?;
    prepare_source_bundle_with_context(
        intent_id,
        workflow_id,
        source_install,
        before,
        proposed,
        PreparedBundleContext {
            identity: &identity,
            lease: &lease,
            ledger_binding,
        },
    )
}

pub(crate) fn prepare_source_bundle_with_context(
    intent_id: &str,
    workflow_id: Option<&str>,
    source_install: SourceInstallV1,
    before: &[u8],
    proposed: &[u8],
    context: PreparedBundleContext<'_>,
) -> Result<PreparedSourceBundle, AppError> {
    validate_ascii_id(intent_id, "intent")?;
    if let Some(workflow_id) = workflow_id {
        validate_ascii_id(workflow_id, "workflow")?;
    }
    validate_source_install_v1(&source_install)?;
    enforce_byte_limit(
        before.len(),
        MAX_SOURCE_BLOB_BYTES,
        "prepared source blob byte limit 초과",
    )?;
    enforce_byte_limit(
        proposed.len(),
        MAX_SOURCE_BLOB_BYTES,
        "prepared source blob byte limit 초과",
    )?;
    let before_bytes = std::str::from_utf8(before)
        .map_err(|_| AppError::blocked("prepared before blob는 UTF-8이어야 합니다."))?
        .to_string();
    let proposed_bytes = std::str::from_utf8(proposed)
        .map_err(|_| AppError::blocked("prepared proposed blob는 UTF-8이어야 합니다."))?
        .to_string();
    if sha256_bytes(before) != source_install.before_sha256
        || sha256_bytes(proposed) != source_install.proposed_sha256
    {
        return Err(AppError::blocked(
            "prepared source blob hash binding 불일치",
        ));
    }
    Ok(PreparedSourceBundle {
        intent_id: intent_id.to_string(),
        intent_kind: "approve-patch".to_string(),
        project_id: context.identity.project_id.clone(),
        session_id: context.identity.session_id.clone(),
        workflow_id: workflow_id.map(str::to_string),
        prepared_at_ms: now_ms(),
        current_revision: context.lease.revision,
        current_artifact_hash: context.lease.artifact_hash.clone(),
        ledger_binding: context.ledger_binding,
        source_install: Some(source_install),
        before_bytes: Some(before_bytes),
        proposed_bytes: Some(proposed_bytes),
        additional_members: Vec::new(),
        semantic_events: Vec::new(),
        event_chain_plan: Vec::new(),
        projection_lag_member_index: None,
    })
}

pub(crate) fn prepare_workflow_bundle_with_context(
    intent_id: &str,
    intent_kind: &str,
    workflow_id: &str,
    context: PreparedBundleContext<'_>,
) -> Result<PreparedSourceBundle, AppError> {
    validate_ascii_id(intent_id, "intent")?;
    validate_ascii_id(workflow_id, "workflow")?;
    if intent_kind != "approve-verification" {
        return Err(AppError::blocked("prepared workflow intent kind 불일치"));
    }
    Ok(PreparedSourceBundle {
        intent_id: intent_id.to_string(),
        intent_kind: intent_kind.to_string(),
        project_id: context.identity.project_id.clone(),
        session_id: context.identity.session_id.clone(),
        workflow_id: Some(workflow_id.to_string()),
        prepared_at_ms: now_ms(),
        current_revision: context.lease.revision,
        current_artifact_hash: context.lease.artifact_hash.clone(),
        ledger_binding: context.ledger_binding,
        source_install: None,
        before_bytes: None,
        proposed_bytes: None,
        additional_members: Vec::new(),
        semantic_events: Vec::new(),
        event_chain_plan: Vec::new(),
        projection_lag_member_index: None,
    })
}

pub(crate) fn prepare_terminal_action_bundle_with_context(
    intent_id: &str,
    intent_kind: &str,
    workflow_id: &str,
    source: Option<(SourceInstallV1, &[u8], &[u8])>,
    context: PreparedBundleContext<'_>,
) -> Result<PreparedSourceBundle, AppError> {
    validate_ascii_id(intent_id, "intent")?;
    validate_ascii_id(workflow_id, "workflow")?;
    if !is_terminal_action_intent_kind(intent_kind) {
        return Err(AppError::blocked(
            "prepared terminal action intent kind 불일치",
        ));
    }
    let (source_install, before_bytes, proposed_bytes) = match source {
        Some((plan, before, proposed)) => {
            validate_source_install_v1(&plan)?;
            let before = std::str::from_utf8(before)
                .map_err(|_| AppError::blocked("terminal source before UTF-8 불일치"))?
                .to_string();
            let proposed = std::str::from_utf8(proposed)
                .map_err(|_| AppError::blocked("terminal source proposed UTF-8 불일치"))?
                .to_string();
            if sha256_bytes(before.as_bytes()) != plan.before_sha256
                || sha256_bytes(proposed.as_bytes()) != plan.proposed_sha256
            {
                return Err(AppError::blocked(
                    "prepared terminal source hash binding 불일치",
                ));
            }
            (Some(plan), Some(before), Some(proposed))
        }
        None => (None, None, None),
    };
    if intent_kind == "deny-patch" && source_install.is_some()
        || intent_kind == "deny-verification" && source_install.is_none()
    {
        return Err(AppError::blocked(
            "prepared terminal source intent/nullability 불일치",
        ));
    }
    Ok(PreparedSourceBundle {
        intent_id: intent_id.to_string(),
        intent_kind: intent_kind.to_string(),
        project_id: context.identity.project_id.clone(),
        session_id: context.identity.session_id.clone(),
        workflow_id: Some(workflow_id.to_string()),
        prepared_at_ms: now_ms(),
        current_revision: context.lease.revision,
        current_artifact_hash: context.lease.artifact_hash.clone(),
        ledger_binding: context.ledger_binding,
        source_install,
        before_bytes,
        proposed_bytes,
        additional_members: Vec::new(),
        semantic_events: Vec::new(),
        event_chain_plan: Vec::new(),
        projection_lag_member_index: None,
    })
}
