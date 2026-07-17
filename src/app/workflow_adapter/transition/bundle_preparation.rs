use super::*;

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

pub(crate) fn bind_additional_members(
    bundle: &mut PreparedSourceBundle,
    mut members: Vec<PreparedMember>,
) -> Result<(), AppError> {
    members.sort_by(prepared_member_order);
    let source_member_count = if bundle.source_install.is_some() {
        3
    } else {
        0
    };
    bundle.projection_lag_member_index = members
        .iter()
        .position(|member| member.kind == PreparedMemberKind::ProjectionLag)
        .map(|index| {
            u64::try_from(index + source_member_count)
                .map_err(|_| AppError::blocked("prepared projection lag index overflow"))
        })
        .transpose()?;
    bundle.additional_members = members;
    validate_prepared_source_bundle(bundle)
}

pub(crate) fn prepare_projection_lag_member(
    intent_id: &str,
    planned: &[crate::app::workflow_adapter::ledger::PlannedEvent],
) -> Result<PreparedMember, AppError> {
    validate_ascii_id(intent_id, "intent")?;
    if planned.len() != 10 {
        return Err(AppError::blocked(
            "projection lag는 exact E0..E9 plan이 필요합니다.",
        ));
    }
    let final_event = &planned[9];
    let required_event_ids = planned
        .iter()
        .map(|entry| {
            format!(
                "\"{}\"",
                crate::app::workflow_adapter::ledger::json_string(&entry.event.event_id)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let bytes_utf8 = format!(
        "{{\"schema_version\":1,\"intent_id\":\"{}\",\"event_id\":\"{}\",\"event_ordinal\":{},\"event_hash\":\"{}\",\"required_outputs\":[\"project-session-ledger\",\"global-operation-log\",\"sqlite\"],\"required_event_ids\":[{}]}}",
        crate::app::workflow_adapter::ledger::json_string(intent_id),
        crate::app::workflow_adapter::ledger::json_string(&final_event.event.event_id),
        final_event.ordinal,
        final_event.event_hash,
        required_event_ids,
    );
    let hash = sha256_bytes(bytes_utf8.as_bytes());
    Ok(PreparedMember {
        kind: PreparedMemberKind::ProjectionLag,
        path: format!(
            "state/projection-lag/{}-{}.json",
            intent_id, final_event.event.event_id
        ),
        schema_version: 1,
        binding: PreparedMemberBinding {
            artifact_id: Some(format!("projection-lag-{hash}")),
            causal_id: None,
            source_key: None,
            event_id: Some(final_event.event.event_id.clone()),
        },
        bytes_utf8,
        expected_type: "absent".to_string(),
        expected_identity: None,
        readonly: false,
        mode: 0o600,
        ownership: None,
        semantic_role_rank: 0,
    })
}

pub(crate) fn install_projection_lag(bundle: &PreparedSourceBundle) -> Result<PathBuf, AppError> {
    validate_prepared_source_bundle(bundle)?;
    let member = bundle
        .additional_members
        .iter()
        .find(|member| member.kind == PreparedMemberKind::ProjectionLag)
        .ok_or_else(|| AppError::blocked("prepared projection lag member 누락"))?;
    let event_id = member
        .binding
        .event_id
        .as_deref()
        .ok_or_else(|| AppError::blocked("prepared projection lag event binding 누락"))?;
    let path = paths::projection_lag_file(&bundle.intent_id, event_id);
    let expected_stored = format!(
        "state/projection-lag/{}-{}.json",
        bundle.intent_id, event_id
    );
    if member.path != expected_stored {
        return Err(AppError::blocked(
            "prepared projection lag path binding 불일치",
        ));
    }
    if path.exists() {
        let existing = fs::read_to_string(&path)
            .map_err(|err| AppError::blocked(format!("projection lag reread 실패: {err}")))?;
        if existing != member.bytes_utf8 {
            return Err(AppError::blocked("projection lag immutable conflict"));
        }
        return Ok(path);
    }
    let parent = path
        .parent()
        .ok_or_else(|| AppError::blocked("projection lag parent 누락"))?;
    fs::create_dir_all(parent)
        .map_err(|err| AppError::runtime(format!("projection lag directory 생성 실패: {err}")))?;
    let temporary = path.with_extension("json.tmp");
    if temporary.exists() {
        let existing = fs::read_to_string(&temporary)
            .map_err(|err| AppError::blocked(format!("projection lag temp reread 실패: {err}")))?;
        if existing != member.bytes_utf8 {
            return Err(AppError::blocked("projection lag temp immutable conflict"));
        }
    } else {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        use std::io::Write;
        let mut file = options
            .open(&temporary)
            .map_err(|err| AppError::runtime(format!("projection lag temp 생성 실패: {err}")))?;
        projection_lag_fault("temp-create")?;
        file.write_all(member.bytes_utf8.as_bytes())
            .map_err(|err| AppError::runtime(format!("projection lag temp write 실패: {err}")))?;
        projection_lag_fault("temp-write")?;
        file.sync_all()
            .map_err(|err| AppError::runtime(format!("projection lag temp fsync 실패: {err}")))?;
        projection_lag_fault("temp-fsync")?;
    }
    fs::rename(&temporary, &path)
        .map_err(|err| AppError::runtime(format!("projection lag rename 실패: {err}")))?;
    projection_lag_fault("rename")?;
    projection_lag_fault("parent-fsync")?;
    sync_parent(&path)?;
    Ok(path)
}

pub(crate) fn projection_lag_path(bundle: &PreparedSourceBundle) -> Result<PathBuf, AppError> {
    validate_prepared_source_bundle(bundle)?;
    let member = bundle
        .additional_members
        .iter()
        .find(|member| member.kind == PreparedMemberKind::ProjectionLag)
        .ok_or_else(|| AppError::blocked("prepared projection lag member 누락"))?;
    let event_id = member
        .binding
        .event_id
        .as_deref()
        .ok_or_else(|| AppError::blocked("prepared projection lag event binding 누락"))?;
    Ok(paths::projection_lag_file(&bundle.intent_id, event_id))
}

pub(crate) fn remove_projection_lag(bundle: &PreparedSourceBundle) -> Result<(), AppError> {
    validate_prepared_source_bundle(bundle)?;
    let member = bundle
        .additional_members
        .iter()
        .find(|member| member.kind == PreparedMemberKind::ProjectionLag)
        .ok_or_else(|| AppError::blocked("prepared projection lag member 누락"))?;
    let path = projection_lag_path(bundle)?;
    let temporary = path.with_extension("json.tmp");
    if temporary.exists() {
        let existing = fs::read_to_string(&temporary).map_err(|err| {
            AppError::blocked(format!("projection lag temp cleanup read 실패: {err}"))
        })?;
        if existing != member.bytes_utf8 {
            return Err(AppError::blocked("projection lag temp cleanup conflict"));
        }
        fs::remove_file(&temporary)
            .map_err(|err| AppError::runtime(format!("projection lag temp cleanup 실패: {err}")))?;
        sync_parent(&temporary)?;
    }
    if !path.exists() {
        return Ok(());
    }
    let existing = fs::read_to_string(&path)
        .map_err(|err| AppError::blocked(format!("projection lag cleanup read 실패: {err}")))?;
    if existing != member.bytes_utf8 {
        return Err(AppError::blocked(
            "projection lag cleanup immutable conflict",
        ));
    }
    fs::remove_file(&path)
        .map_err(|err| AppError::runtime(format!("projection lag cleanup 실패: {err}")))?;
    let cleanup = projection_lag_fault("lag-remove")
        .and_then(|_| projection_lag_fault("lag-parent-fsync"))
        .and_then(|_| sync_parent(&path));
    if let Err(error) = cleanup {
        restore_removed_file(&path, member.bytes_utf8.as_bytes(), "projection lag")?;
        return Err(error);
    }
    Ok(())
}

pub(crate) fn planned_events(
    bundle: &PreparedSourceBundle,
) -> Result<Vec<crate::app::workflow_adapter::ledger::PlannedEvent>, AppError> {
    validate_prepared_source_bundle(bundle)?;
    Ok(bundle
        .semantic_events
        .iter()
        .cloned()
        .zip(bundle.event_chain_plan.iter())
        .map(
            |(event, chain)| crate::app::workflow_adapter::ledger::PlannedEvent {
                event,
                ordinal: chain.ordinal,
                previous_event_hash: chain.previous_event_hash.clone(),
                event_hash: chain.event_hash.clone(),
            },
        )
        .collect())
}

pub(crate) fn bind_planned_events(
    bundle: &mut PreparedSourceBundle,
    planned: &[crate::app::workflow_adapter::ledger::PlannedEvent],
) -> Result<(), AppError> {
    bundle.semantic_events = planned.iter().map(|entry| entry.event.clone()).collect();
    bundle.event_chain_plan = planned
        .iter()
        .map(|entry| PreparedEventChain {
            event_id: entry.event.event_id.clone(),
            ordinal: entry.ordinal,
            previous_event_hash: entry.previous_event_hash.clone(),
            event_hash: entry.event_hash.clone(),
        })
        .collect();
    validate_event_chain(bundle)
}
