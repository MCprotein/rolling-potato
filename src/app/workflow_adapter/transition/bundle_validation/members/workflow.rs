use super::*;

pub(super) fn validate_state_transition_members(
    bundle: &PreparedSourceBundle,
) -> Result<(), AppError> {
    let checkpoint = bundle.intent_kind == "checkpoint-workflow";
    let preserved_reconcile = bundle.intent_kind == "reconcile"
        && bundle.current_revision == 0
        && bundle.current_artifact_hash != "missing";
    let expected_kinds: &[PreparedMemberKind] = if checkpoint {
        &[
            PreparedMemberKind::WorkflowSnapshot,
            PreparedMemberKind::WorkflowPointer,
            PreparedMemberKind::CurrentImage,
        ]
    } else if preserved_reconcile {
        &[
            PreparedMemberKind::ToolOutput,
            PreparedMemberKind::CurrentImage,
        ]
    } else {
        &[PreparedMemberKind::CurrentImage]
    };
    if bundle.source_install.is_some()
        || bundle.before_bytes.is_some()
        || bundle.proposed_bytes.is_some()
        || bundle.projection_lag_member_index.is_some()
        || bundle.semantic_events.len() != 1
        || bundle.additional_members.len() != expected_kinds.len()
        || (checkpoint && bundle.workflow_id.is_none())
    {
        return Err(AppError::blocked(
            "prepared state transition exact shape 불일치",
        ));
    }
    let event = &bundle.semantic_events[0];
    let event_type_matches = match bundle.intent_kind.as_str() {
        "bootstrap" => event.event_type == "runtime.init",
        "checkpoint-workflow" => event.event_type == "workflow.checkpoint",
        "repair-workflow-pointer" => event.event_type == "workflow.pointer.recovered",
        "clear-terminal-workflow" => event.event_type == "workflow.pointer.cleared",
        "reconcile" => event.event_type.starts_with("state.reconcile."),
        "resume" => event.event_type.starts_with("workflow.resume."),
        "cancel" => event.event_type.starts_with("workflow.cancel."),
        "start-session" => event.event_type == "session.new",
        "select-session" => event.event_type == "session.resume.selected",
        "record-event" => !event.event_type.is_empty(),
        _ => false,
    };
    if !event_type_matches {
        return Err(AppError::blocked(
            "prepared state transition semantic event type 불일치",
        ));
    }
    let mut artifact_ids = std::collections::BTreeSet::new();
    let mut paths = std::collections::BTreeSet::new();
    for (index, member) in bundle.additional_members.iter().enumerate() {
        if member.kind != expected_kinds[index]
            || member.semantic_role_rank != 0
            || member.binding.source_key.is_some()
            || member.readonly
            || member.mode != 0o600
            || member.ownership.is_some()
            || member.bytes_utf8.is_empty()
        {
            return Err(AppError::blocked(
                "prepared state transition member metadata 불일치",
            ));
        }
        let expected_schema = match member.kind {
            PreparedMemberKind::ToolOutput => 1,
            PreparedMemberKind::WorkflowSnapshot | PreparedMemberKind::WorkflowPointer => 4,
            PreparedMemberKind::CurrentImage => 2,
            _ => {
                return Err(AppError::blocked(
                    "prepared state transition member kind 불일치",
                ))
            }
        };
        let limit = match member.kind {
            PreparedMemberKind::ToolOutput => 65_536,
            PreparedMemberKind::WorkflowSnapshot | PreparedMemberKind::CurrentImage => 65_536,
            PreparedMemberKind::WorkflowPointer => 16_384,
            _ => unreachable!("state transition kind validated above"),
        };
        if member.schema_version != expected_schema || member.bytes_utf8.len() > limit {
            return Err(AppError::blocked(
                "prepared state transition member schema/byte limit 불일치",
            ));
        }
        validate_stored_path(&member.path)?;
        let artifact_id = member
            .binding
            .artifact_id
            .as_deref()
            .ok_or_else(|| AppError::blocked("prepared state transition artifact id 누락"))?;
        validate_ascii_id(artifact_id, "member artifact")?;
        if member.binding.event_id.as_deref() != Some(event.event_id.as_str())
            || !artifact_ids.insert(artifact_id)
            || !paths.insert(member.path.as_str())
        {
            return Err(AppError::blocked(
                "prepared state transition member event/id/path 불일치",
            ));
        }
        if index > 0
            && prepared_member_order(
                &bundle.additional_members[index - 1],
                &bundle.additional_members[index],
            ) != std::cmp::Ordering::Less
        {
            return Err(AppError::blocked(
                "prepared state transition member order 불일치",
            ));
        }
    }
    if preserved_reconcile {
        let backup = &bundle.additional_members[0];
        let reason = if bundle.semantic_events[0].event_type == "state.reconcile.corrupt_recovered"
        {
            "corrupt"
        } else if bundle.semantic_events[0].event_type == "state.reconcile.stale_recovered" {
            "stale"
        } else {
            return Err(AppError::blocked(
                "prepared reconcile preserved reason 불일치",
            ));
        };
        let expected_path = format!("state/current-state.json.{reason}.{}", bundle.intent_id);
        if backup.path != expected_path
            || backup.expected_type != "absent"
            || sha256_bytes(backup.bytes_utf8.as_bytes()) != bundle.current_artifact_hash
            || backup.binding.causal_id.is_some()
        {
            return Err(AppError::blocked(
                "prepared reconcile preserved member binding 불일치",
            ));
        }
    }
    let current = bundle
        .additional_members
        .last()
        .ok_or_else(|| AppError::blocked("prepared state transition current 누락"))?;
    crate::app::workflow_adapter::state::validate_prepared_state_current_member(bundle, current)?;
    if checkpoint {
        let workflow_id = bundle
            .workflow_id
            .as_deref()
            .ok_or_else(|| AppError::blocked("prepared checkpoint workflow id 누락"))?;
        let prepared = crate::app::workflow_adapter::state::decode_prepared_workflow_revision(
            workflow_id,
            &bundle.additional_members[0],
            &bundle.additional_members[1],
            event,
        )?;
        let final_chain = bundle
            .event_chain_plan
            .last()
            .ok_or_else(|| AppError::blocked("prepared checkpoint final chain 누락"))?;
        let final_binding = crate::app::workflow_adapter::ledger::LedgerBinding {
            event_count: final_chain.ordinal,
            event_id: Some(final_chain.event_id.clone()),
            event_hash: final_chain.event_hash.clone(),
        };
        crate::app::workflow_adapter::state::decode_prepared_current_image(
            current,
            &prepared.record,
            &final_binding,
            &prepared.snapshot_member_id,
            &event.event_id,
        )?;
    }
    Ok(())
}

pub(super) fn validate_verification_members(bundle: &PreparedSourceBundle) -> Result<(), AppError> {
    let expected_types = match bundle.intent_kind.as_str() {
        "approve-verification" => [
            "runtime.intent.accepted",
            "workflow.checkpoint",
            "patch.verification.approved",
        ],
        "deny-patch" => [
            "runtime.intent.accepted",
            "workflow.checkpoint",
            "patch.apply.denied",
        ],
        "deny-verification" => [
            "runtime.intent.accepted",
            "workflow.checkpoint",
            "patch.verification.denied",
        ],
        "cancel-workflow" => [
            "runtime.intent.accepted",
            "workflow.checkpoint",
            "workflow.user-cancelled",
        ],
        _ => return Err(AppError::blocked("prepared single revision intent 불일치")),
    };
    let expected_kinds = [
        PreparedMemberKind::WorkflowSnapshot,
        PreparedMemberKind::WorkflowPointer,
        PreparedMemberKind::CurrentImage,
    ];
    if bundle.additional_members.len() != expected_kinds.len()
        || bundle.semantic_events.len() != expected_types.len()
        || bundle.workflow_id.is_none()
        || bundle.projection_lag_member_index.is_some()
        || bundle
            .semantic_events
            .iter()
            .zip(expected_types)
            .any(|(event, expected)| event.event_type != expected)
    {
        return Err(AppError::blocked(
            "prepared verification approval exact shape 불일치",
        ));
    }
    let workflow_id = bundle.workflow_id.as_deref().expect("validated above");
    let mut artifact_ids = std::collections::BTreeSet::new();
    let mut paths = std::collections::BTreeSet::new();
    for (index, member) in bundle.additional_members.iter().enumerate() {
        if member.kind != expected_kinds[index]
            || member.semantic_role_rank != 0
            || member.binding.source_key.is_some()
            || member.readonly
            || member.mode != 0o600
            || member.ownership.is_some()
            || member.bytes_utf8.is_empty()
            || member.expected_type == "content-addressed-reference"
        {
            return Err(AppError::blocked(
                "prepared verification member kind/metadata 불일치",
            ));
        }
        let expected_schema = match member.kind {
            PreparedMemberKind::WorkflowSnapshot | PreparedMemberKind::WorkflowPointer => 4,
            PreparedMemberKind::CurrentImage => 2,
            _ => {
                return Err(AppError::blocked(
                    "prepared verification member kind 불일치",
                ))
            }
        };
        if member.schema_version != expected_schema
            || member
                .binding
                .artifact_id
                .as_deref()
                .is_none_or(|id| validate_ascii_id(id, "member artifact").is_err())
        {
            return Err(AppError::blocked(
                "prepared verification member schema/binding 불일치",
            ));
        }
        let limit = match member.kind {
            PreparedMemberKind::WorkflowSnapshot => 65_536,
            PreparedMemberKind::WorkflowPointer => 16_384,
            PreparedMemberKind::CurrentImage => 65_536,
            _ => unreachable!("verification member kind validated above"),
        };
        if member.bytes_utf8.len() > limit {
            return Err(AppError::blocked(
                "prepared verification member byte limit 초과",
            ));
        }
        validate_stored_path(&member.path)?;
        if member.kind == PreparedMemberKind::WorkflowPointer
            && member.path != format!(".rpotato/workflows/{workflow_id}.json")
        {
            return Err(AppError::blocked(
                "prepared verification workflow pointer path 불일치",
            ));
        }
        for (label, value) in [
            ("member causal", member.binding.causal_id.as_deref()),
            ("member event", member.binding.event_id.as_deref()),
        ] {
            if let Some(value) = value {
                validate_ascii_id(value, label)?;
            }
        }
        let artifact_id = member
            .binding
            .artifact_id
            .as_deref()
            .expect("validated above");
        if !artifact_ids.insert(artifact_id) || !paths.insert(member.path.as_str()) {
            return Err(AppError::blocked(
                "prepared verification member id/path 중복",
            ));
        }
        if index > 0
            && prepared_member_order(
                &bundle.additional_members[index - 1],
                &bundle.additional_members[index],
            ) != std::cmp::Ordering::Less
        {
            return Err(AppError::blocked(
                "prepared verification member total order 불일치",
            ));
        }
    }
    Ok(())
}
