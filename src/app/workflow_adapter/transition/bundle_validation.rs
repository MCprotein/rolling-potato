use super::*;

pub(super) fn validate_prepared_source_bundle(
    bundle: &PreparedSourceBundle,
) -> Result<(), AppError> {
    validate_ascii_id(&bundle.intent_id, "intent")?;
    validate_ascii_id(&bundle.project_id, "project")?;
    validate_ascii_id(&bundle.session_id, "session")?;
    if let Some(workflow_id) = bundle.workflow_id.as_deref() {
        validate_ascii_id(workflow_id, "workflow")?;
    }
    if !matches!(
        bundle.intent_kind.as_str(),
        "approve-patch" | "approve-verification"
    ) && !is_state_transition_intent_kind(&bundle.intent_kind)
        && !is_terminal_action_intent_kind(&bundle.intent_kind)
    {
        return Err(AppError::blocked("prepared bundle intent kind 불일치"));
    }
    let missing_current = bundle.current_revision == 0
        && bundle.current_artifact_hash == "missing"
        && matches!(
            bundle.intent_kind.as_str(),
            "bootstrap" | "repair-workflow-pointer" | "reconcile" | "start-session"
        );
    let preserved_invalid_current = bundle.current_revision == 0
        && is_sha256(&bundle.current_artifact_hash)
        && bundle.intent_kind == "reconcile";
    if (!missing_current
        && !preserved_invalid_current
        && (bundle.current_revision == 0 || !is_sha256(&bundle.current_artifact_hash)))
        || (bundle.ledger_binding.event_count == 0
            && (bundle.ledger_binding.event_id.is_some()
                || bundle.ledger_binding.event_hash != "root"))
        || (bundle.ledger_binding.event_count > 0
            && (bundle.ledger_binding.event_id.is_none()
                || !is_sha256(&bundle.ledger_binding.event_hash)))
    {
        return Err(AppError::blocked("prepared source bundle binding 불일치"));
    }
    match (
        bundle.intent_kind.as_str(),
        bundle.source_install.as_ref(),
        bundle.before_bytes.as_deref(),
        bundle.proposed_bytes.as_deref(),
    ) {
        ("approve-patch", Some(source), Some(before), Some(proposed)) => {
            validate_source_install_v1(source)?;
            if sha256_bytes(before.as_bytes()) != source.before_sha256
                || sha256_bytes(proposed.as_bytes()) != source.proposed_sha256
            {
                return Err(AppError::blocked(
                    "prepared source bundle hash binding 불일치",
                ));
            }
        }
        ("approve-verification", None, None, None) => {}
        (kind, Some(source), Some(before), Some(proposed))
            if is_terminal_action_intent_kind(kind) =>
        {
            validate_source_install_v1(source)?;
            if sha256_bytes(before.as_bytes()) != source.before_sha256
                || sha256_bytes(proposed.as_bytes()) != source.proposed_sha256
                || kind == "deny-patch"
            {
                return Err(AppError::blocked(
                    "prepared terminal source bundle hash/intent 불일치",
                ));
            }
        }
        (kind, None, None, None) if is_terminal_action_intent_kind(kind) => {
            if kind == "deny-verification" {
                return Err(AppError::blocked("prepared denial rollback source 누락"));
            }
        }
        (kind, None, None, None) if is_state_transition_intent_kind(kind) => {}
        _ => {
            return Err(AppError::blocked(
                "prepared bundle source nullability 불일치",
            ))
        }
    }
    validate_event_chain(bundle)?;
    validate_additional_members(bundle)?;
    Ok(())
}

pub(super) fn validate_event_chain(bundle: &PreparedSourceBundle) -> Result<(), AppError> {
    if bundle.semantic_events.len() != bundle.event_chain_plan.len()
        || bundle.semantic_events.len() > 10
    {
        return Err(AppError::blocked(
            "prepared semantic event/chain cardinality 불일치",
        ));
    }
    let mut aggregate_event_bytes = 0_usize;
    for event in &bundle.semantic_events {
        let rendered = render_semantic_event(event);
        enforce_byte_limit(
            rendered.len(),
            MAX_PREPARED_EVENT_BYTES,
            "prepared semantic event byte limit 초과",
        )?;
        aggregate_event_bytes = checked_add_bytes(
            aggregate_event_bytes,
            rendered.len(),
            MAX_PREPARED_EVENTS_BYTES,
            "prepared semantic event byte count overflow",
            "prepared semantic events aggregate byte limit 초과",
        )?;
    }
    let mut previous = bundle.ledger_binding.event_hash.clone();
    let mut ids = std::collections::BTreeSet::new();
    for (index, (event, chain)) in bundle
        .semantic_events
        .iter()
        .zip(&bundle.event_chain_plan)
        .enumerate()
    {
        validate_ascii_id(&event.event_id, "event")?;
        if event.event_type.is_empty()
            || event.project_id != bundle.project_id
            || event.session_id != bundle.session_id
            || !ids.insert(event.event_id.as_str())
        {
            return Err(AppError::blocked(
                "prepared semantic event owner/id binding 불일치",
            ));
        }
        let expected_ordinal = bundle
            .ledger_binding
            .event_count
            .checked_add(
                u64::try_from(index + 1)
                    .map_err(|_| AppError::blocked("prepared event ordinal overflow"))?,
            )
            .ok_or_else(|| AppError::blocked("prepared event ordinal overflow"))?;
        let expected_hash =
            crate::app::workflow_adapter::ledger::planned_event_hash(event, &previous);
        if chain.event_id != event.event_id
            || chain.ordinal != expected_ordinal
            || chain.previous_event_hash != previous
            || chain.event_hash != expected_hash
            || !is_sha256(&chain.event_hash)
        {
            return Err(AppError::blocked(
                "prepared semantic event chain binding 불일치",
            ));
        }
        previous = chain.event_hash.clone();
    }
    Ok(())
}

fn validate_additional_members(bundle: &PreparedSourceBundle) -> Result<(), AppError> {
    if bundle.additional_members.is_empty() {
        if bundle.intent_kind != "approve-patch"
            || bundle.source_install.is_none()
            || bundle.projection_lag_member_index.is_some()
        {
            return Err(AppError::blocked(
                "prepared source-only bundle에는 projection lag reference가 없어야 합니다.",
            ));
        }
        return Ok(());
    }
    if bundle.intent_kind == "approve-verification" {
        return validate_verification_members(bundle);
    }
    if is_terminal_action_intent_kind(&bundle.intent_kind) {
        return validate_verification_members(bundle);
    }
    if is_state_transition_intent_kind(&bundle.intent_kind) {
        return validate_state_transition_members(bundle);
    }
    if bundle.intent_kind != "approve-patch" || bundle.source_install.is_none() {
        return Err(AppError::blocked(
            "prepared approval member intent binding 불일치",
        ));
    }
    if bundle.additional_members.len() != 8
        || bundle.semantic_events.len() != 10
        || bundle.workflow_id.is_none()
        || bundle.projection_lag_member_index != Some(10)
    {
        return Err(AppError::blocked(
            "prepared production approval exact-11 cardinality 불일치",
        ));
    }
    let expected_kinds = [
        PreparedMemberKind::ToolOutput,
        PreparedMemberKind::TranscriptV2,
        PreparedMemberKind::WorkflowSnapshot,
        PreparedMemberKind::WorkflowSnapshot,
        PreparedMemberKind::WorkflowPointer,
        PreparedMemberKind::WorkflowPointer,
        PreparedMemberKind::CurrentImage,
        PreparedMemberKind::ProjectionLag,
    ];
    let mut artifact_ids = std::collections::BTreeSet::new();
    let mut paths = std::collections::BTreeMap::<&str, Vec<&PreparedMember>>::new();
    for (index, member) in bundle.additional_members.iter().enumerate() {
        if member.kind != expected_kinds[index]
            || member.semantic_role_rank
                != match member.kind {
                    PreparedMemberKind::WorkflowSnapshot | PreparedMemberKind::WorkflowPointer => {
                        u8::try_from(index % 2)
                            .map_err(|_| AppError::blocked("prepared member role overflow"))?
                    }
                    _ => 0,
                }
            || member.binding.source_key.is_some()
        {
            return Err(AppError::blocked(
                "prepared production member kind/role/source binding 불일치",
            ));
        }
        let expected_schema = match member.kind {
            PreparedMemberKind::ToolOutput => 1,
            PreparedMemberKind::TranscriptV2 => 2,
            PreparedMemberKind::WorkflowSnapshot | PreparedMemberKind::WorkflowPointer => 4,
            PreparedMemberKind::CurrentImage => 2,
            PreparedMemberKind::ProjectionLag => 1,
        };
        if member.schema_version != expected_schema
            || member.bytes_utf8.is_empty()
            || member.expected_type == "content-addressed-reference"
            || member.readonly
            || member.mode != 0o600
            || member.ownership.is_some()
            || member
                .binding
                .artifact_id
                .as_deref()
                .is_none_or(|id| validate_ascii_id(id, "member artifact").is_err())
        {
            return Err(AppError::blocked(
                "prepared production member schema/bytes/binding 불일치",
            ));
        }
        validate_stored_path(&member.path)?;
        if member.kind == PreparedMemberKind::WorkflowPointer
            && member.path
                != format!(
                    ".rpotato/workflows/{}.json",
                    bundle.workflow_id.as_deref().expect("validated above")
                )
        {
            return Err(AppError::blocked(
                "prepared workflow pointer canonical path 불일치",
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
        let limit = match member.kind {
            PreparedMemberKind::ToolOutput => 262_144,
            PreparedMemberKind::TranscriptV2 => 131_072,
            PreparedMemberKind::WorkflowSnapshot => 65_536,
            PreparedMemberKind::WorkflowPointer => 16_384,
            PreparedMemberKind::CurrentImage => 65_536,
            PreparedMemberKind::ProjectionLag => 4_096,
        };
        enforce_byte_limit(
            member.bytes_utf8.len(),
            limit,
            "prepared member byte limit 초과",
        )?;
        let artifact_id = member
            .binding
            .artifact_id
            .as_deref()
            .expect("validated above");
        if !artifact_ids.insert(artifact_id) {
            return Err(AppError::blocked("prepared member artifact id 중복"));
        }
        paths.entry(member.path.as_str()).or_default().push(member);
        if index > 0
            && prepared_member_order(
                &bundle.additional_members[index - 1],
                &bundle.additional_members[index],
            ) != std::cmp::Ordering::Less
        {
            return Err(AppError::blocked(
                "prepared member total order/duplicate full key 불일치",
            ));
        }
    }
    for (path, members) in paths {
        if members.len() == 1 {
            continue;
        }
        let workflow_id = bundle.workflow_id.as_deref().expect("validated above");
        let expected_path = format!(".rpotato/workflows/{workflow_id}.json");
        if members.len() != 2
            || path != expected_path
            || members
                .iter()
                .any(|member| member.kind != PreparedMemberKind::WorkflowPointer)
            || members[0].semantic_role_rank != 0
            || members[1].semantic_role_rank != 1
        {
            return Err(AppError::blocked(
                "prepared member duplicate path는 exact R+1/R+2 pointer pair만 허용됩니다.",
            ));
        }
    }
    let lag = bundle
        .additional_members
        .last()
        .expect("exact eight members validated");
    if lag.kind != PreparedMemberKind::ProjectionLag
        || bundle.projection_lag_member_index != Some(10)
        || lag.binding.event_id.as_deref() != Some(bundle.semantic_events[9].event_id.as_str())
    {
        return Err(AppError::blocked(
            "prepared projection lag E9/index binding 불일치",
        ));
    }
    validate_projection_lag_member(bundle, lag)?;
    Ok(())
}

fn validate_state_transition_members(bundle: &PreparedSourceBundle) -> Result<(), AppError> {
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

fn validate_verification_members(bundle: &PreparedSourceBundle) -> Result<(), AppError> {
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

fn validate_projection_lag_member(
    bundle: &PreparedSourceBundle,
    lag: &PreparedMember,
) -> Result<(), AppError> {
    let object = strict_json::parse_canonical_object(
        &lag.bytes_utf8,
        PROJECTION_LAG_KEYS,
        "projection lag member",
    )?;
    let final_event = bundle
        .semantic_events
        .get(9)
        .ok_or_else(|| AppError::blocked("projection lag final event 누락"))?;
    let final_chain = bundle
        .event_chain_plan
        .get(9)
        .ok_or_else(|| AppError::blocked("projection lag final chain 누락"))?;
    let required_outputs = required_string_array(&object, "required_outputs")?;
    let required_event_ids = required_string_array(&object, "required_event_ids")?;
    let expected_event_ids = bundle
        .semantic_events
        .iter()
        .map(|event| event.event_id.clone())
        .collect::<Vec<_>>();
    let expected_path = format!(
        "state/projection-lag/{}-{}.json",
        bundle.intent_id, final_event.event_id
    );
    let hash = sha256_bytes(lag.bytes_utf8.as_bytes());
    if strict_json::canonical_u64(&object, "schema_version", "projection lag member")? != 1
        || required_string(&object, "intent_id")? != bundle.intent_id
        || required_string(&object, "event_id")? != final_event.event_id
        || strict_json::canonical_u64(&object, "event_ordinal", "projection lag member")?
            != final_chain.ordinal
        || required_string(&object, "event_hash")? != final_chain.event_hash
        || required_outputs
            != [
                "project-session-ledger".to_string(),
                "global-operation-log".to_string(),
                "sqlite".to_string(),
            ]
        || required_event_ids != expected_event_ids
        || lag.path != expected_path
        || lag.binding.artifact_id.as_deref() != Some(format!("projection-lag-{hash}").as_str())
        || lag.binding.causal_id.is_some()
        || lag.expected_type != "absent"
    {
        return Err(AppError::blocked(
            "prepared projection lag canonical/reference binding 불일치",
        ));
    }
    Ok(())
}
