use super::*;

mod workflow;
use workflow::{validate_state_transition_members, validate_verification_members};

pub(super) fn validate_additional_members(bundle: &PreparedSourceBundle) -> Result<(), AppError> {
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
