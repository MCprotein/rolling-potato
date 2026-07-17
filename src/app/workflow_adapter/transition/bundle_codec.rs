use super::*;

pub(super) fn render_source_members(bundle: &PreparedSourceBundle) -> Result<String, AppError> {
    let Some(source) = bundle.source_install.as_ref() else {
        let members = bundle
            .additional_members
            .iter()
            .map(|member| render_additional_member(bundle, member))
            .collect::<Vec<_>>();
        return Ok(format!("[{}]", members.join(",")));
    };
    let before_bytes = bundle
        .before_bytes
        .as_deref()
        .ok_or_else(|| AppError::blocked("prepared source before bytes 누락"))?;
    let proposed_bytes = bundle
        .proposed_bytes
        .as_deref()
        .ok_or_else(|| AppError::blocked("prepared source proposed bytes 누락"))?;
    let mode = source.unix_metadata.before_mode;
    let owner = bundle.workflow_id.as_deref();
    let common_owner = |workflow_id: Option<&str>| {
        format!(
            "{{\"project_id\":\"{}\",\"session_id\":\"{}\",\"workflow_id\":{},\"intent_id\":\"{}\"}}",
            bundle.project_id,
            bundle.session_id,
            render_optional_string(workflow_id),
            bundle.intent_id
        )
    };
    let binding = |artifact_id: &str| {
        format!(
            "{{\"artifact_id\":\"{}\",\"causal_id\":null,\"source_key\":\"{}\",\"event_id\":null}}",
            artifact_id, source.source_key
        )
    };
    let member = |kind: &str,
                  path: &str,
                  artifact_id: &str,
                  bytes: Option<&str>,
                  byte_length: u64,
                  sha256: &str,
                  expected_type: &str,
                  expected_identity: Option<&str>,
                  ownership: Option<&str>| {
        format!(
            "{{\"member_kind\":\"{}\",\"path\":\"{}\",\"schema_version\":null,\"owner\":{},\"binding\":{},\"prepared_at_ms\":{},\"bytes_utf8\":{},\"byte_length\":{},\"sha256\":\"{}\",\"expected_type\":\"{}\",\"expected_identity\":{},\"permissions\":{{\"readonly\":false,\"mode\":{}}},\"ownership\":{}}}",
            kind,
            crate::app::workflow_adapter::ledger::json_string(path),
            common_owner(owner),
            binding(artifact_id),
            bundle.prepared_at_ms,
            render_optional_string(bytes),
            byte_length,
            sha256,
            expected_type,
            render_optional_string(expected_identity),
            mode,
            render_optional_string(ownership)
        )
    };
    let before = member(
        "before_blob",
        &source.before_blob.member_path,
        &source.before_blob.blob_id,
        Some(before_bytes),
        source.before_byte_length,
        &source.before_sha256,
        "file",
        source.target.expected_identity.as_deref(),
        Some(&source.ownership.before_owner),
    );
    let proposed = member(
        "proposed_blob",
        &source.proposed_blob.member_path,
        &source.proposed_blob.blob_id,
        Some(proposed_bytes),
        source.proposed_byte_length,
        &source.proposed_sha256,
        "file",
        None,
        Some(&source.ownership.install_owner),
    );
    let rollback = member(
        "rollback_ref",
        &source.rollback_final.path,
        &format!("rollback-ref-{}", source.source_key),
        None,
        source.before_byte_length,
        &source.before_sha256,
        "content-addressed-reference",
        Some(&source.before_sha256),
        Some(&source.ownership.before_owner),
    );
    let mut members = vec![before, proposed, rollback];
    members.extend(
        bundle
            .additional_members
            .iter()
            .map(|member| render_additional_member(bundle, member)),
    );
    Ok(format!("[{}]", members.join(",")))
}

fn render_additional_member(bundle: &PreparedSourceBundle, member: &PreparedMember) -> String {
    let binding = &member.binding;
    let byte_length = member.bytes_utf8.len();
    let hash = sha256_bytes(member.bytes_utf8.as_bytes());
    format!(
        "{{\"member_kind\":\"{}\",\"path\":\"{}\",\"schema_version\":{},\"owner\":{{\"project_id\":\"{}\",\"session_id\":\"{}\",\"workflow_id\":{},\"intent_id\":\"{}\"}},\"binding\":{{\"artifact_id\":{},\"causal_id\":{},\"source_key\":{},\"event_id\":{}}},\"prepared_at_ms\":{},\"bytes_utf8\":\"{}\",\"byte_length\":{},\"sha256\":\"{}\",\"expected_type\":\"{}\",\"expected_identity\":{},\"permissions\":{{\"readonly\":{},\"mode\":{}}},\"ownership\":{}}}",
        member.kind.as_str(),
        crate::app::workflow_adapter::ledger::json_string(&member.path),
        member.schema_version,
        bundle.project_id,
        bundle.session_id,
        render_optional_string(bundle.workflow_id.as_deref()),
        bundle.intent_id,
        render_optional_string(binding.artifact_id.as_deref()),
        render_optional_string(binding.causal_id.as_deref()),
        render_optional_string(binding.source_key.as_deref()),
        render_optional_string(binding.event_id.as_deref()),
        bundle.prepared_at_ms,
        crate::app::workflow_adapter::ledger::json_string(&member.bytes_utf8),
        byte_length,
        hash,
        member.expected_type,
        render_optional_string(member.expected_identity.as_deref()),
        member.readonly,
        member.mode,
        render_optional_string(member.ownership.as_deref()),
    )
}

pub(super) fn parse_source_members(
    root: &CanonicalObject,
    source: &SourceInstallV1,
    context: &PreparedMemberParseContext<'_>,
) -> Result<(String, String, Vec<PreparedMember>), AppError> {
    let Some(CanonicalValue::Array(members)) = root.get("members") else {
        return Err(AppError::blocked("prepared source members 누락"));
    };
    if members.len() < 3 {
        return Err(AppError::blocked("prepared source members count 불일치"));
    }
    let expected = [
        (
            "before_blob",
            source.before_blob.member_path.as_str(),
            source.before_blob.blob_id.as_str(),
            source.before_sha256.as_str(),
            source.before_byte_length,
            true,
        ),
        (
            "proposed_blob",
            source.proposed_blob.member_path.as_str(),
            source.proposed_blob.blob_id.as_str(),
            source.proposed_sha256.as_str(),
            source.proposed_byte_length,
            true,
        ),
        (
            "rollback_ref",
            source.rollback_final.path.as_str(),
            "",
            source.before_sha256.as_str(),
            source.before_byte_length,
            false,
        ),
    ];
    let mut decoded = Vec::new();
    for (index, value) in members.iter().take(3).enumerate() {
        let CanonicalValue::Object(member) = value else {
            return Err(AppError::blocked("prepared source member type 불일치"));
        };
        require_keys(member, MEMBER_KEYS)?;
        let owner = required_object(member, "owner")?;
        require_keys(owner, OWNER_KEYS)?;
        if required_string(owner, "project_id")? != context.project_id
            || required_string(owner, "session_id")? != context.session_id
            || optional_string(owner, "workflow_id")?.as_deref() != context.workflow_id
            || required_string(owner, "intent_id")? != context.intent_id
        {
            return Err(AppError::blocked("prepared source member owner 불일치"));
        }
        let binding = required_object(member, "binding")?;
        require_keys(binding, BINDING_KEYS)?;
        let artifact_id = required_string(binding, "artifact_id")?;
        if optional_string(binding, "causal_id")?.is_some()
            || optional_string(binding, "source_key")?.as_deref()
                != Some(source.source_key.as_str())
            || optional_string(binding, "event_id")?.is_some()
        {
            return Err(AppError::blocked("prepared source member binding 불일치"));
        }
        let (kind, path, expected_artifact, hash, length, has_bytes) = expected[index];
        if required_string(member, "member_kind")? != kind
            || required_string(member, "path")? != path
            || (index < 2 && artifact_id != expected_artifact)
            || !matches!(member.get("schema_version"), Some(CanonicalValue::Null))
            || required_u128(member, "prepared_at_ms")? != context.prepared_at_ms
            || strict_json::canonical_u64(member, "byte_length", "prepared source member")?
                != length
            || required_string(member, "sha256")? != hash
        {
            return Err(AppError::blocked(
                "prepared source member scalar binding 불일치",
            ));
        }
        let permissions = required_object(member, "permissions")?;
        require_keys(permissions, MEMBER_PERMISSION_KEYS)?;
        let _ = required_bool(permissions, "readonly")?;
        let _ = required_u32(permissions, "mode")?;
        let bytes = optional_string(member, "bytes_utf8")?;
        if has_bytes != bytes.is_some() {
            return Err(AppError::blocked(
                "prepared source member bytes nullability 불일치",
            ));
        }
        if let Some(bytes) = bytes {
            if bytes.len() > MAX_SOURCE_BLOB_BYTES
                || sha256_bytes(bytes.as_bytes()) != hash
                || u64::try_from(bytes.len()).ok() != Some(length)
            {
                return Err(AppError::blocked(
                    "prepared source member embedded bytes 불일치",
                ));
            }
            decoded.push(bytes);
        }
    }
    let mut additional = Vec::with_capacity(members.len().saturating_sub(3));
    for value in members.iter().skip(3) {
        additional.push(parse_additional_member(value, context)?);
    }
    Ok((decoded.remove(0), decoded.remove(0), additional))
}

pub(super) struct PreparedMemberParseContext<'a> {
    pub(super) prepared_at_ms: u128,
    pub(super) project_id: &'a str,
    pub(super) session_id: &'a str,
    pub(super) workflow_id: Option<&'a str>,
    pub(super) intent_id: &'a str,
    pub(super) intent_kind: &'a str,
    pub(super) semantic_events: &'a [crate::app::workflow_adapter::ledger::LedgerEvent],
}

pub(super) fn parse_additional_members(
    root: &CanonicalObject,
    context: &PreparedMemberParseContext<'_>,
) -> Result<Vec<PreparedMember>, AppError> {
    let Some(CanonicalValue::Array(members)) = root.get("members") else {
        return Err(AppError::blocked("prepared members type 불일치"));
    };
    members
        .iter()
        .map(|value| parse_additional_member(value, context))
        .collect()
}

fn parse_additional_member(
    value: &CanonicalValue,
    context: &PreparedMemberParseContext<'_>,
) -> Result<PreparedMember, AppError> {
    let CanonicalValue::Object(member) = value else {
        return Err(AppError::blocked("prepared member type 불일치"));
    };
    require_keys(member, MEMBER_KEYS)?;
    let owner = required_object(member, "owner")?;
    require_keys(owner, OWNER_KEYS)?;
    if required_string(owner, "project_id")? != context.project_id
        || required_string(owner, "session_id")? != context.session_id
        || optional_string(owner, "workflow_id")?.as_deref() != context.workflow_id
        || required_string(owner, "intent_id")? != context.intent_id
    {
        return Err(AppError::blocked("prepared member owner 불일치"));
    }
    let binding = required_object(member, "binding")?;
    require_keys(binding, BINDING_KEYS)?;
    let permissions = required_object(member, "permissions")?;
    require_keys(permissions, MEMBER_PERMISSION_KEYS)?;
    if required_u128(member, "prepared_at_ms")? != context.prepared_at_ms {
        return Err(AppError::blocked(
            "prepared member timestamp binding 불일치",
        ));
    }
    let bytes_utf8 = optional_string(member, "bytes_utf8")?
        .ok_or_else(|| AppError::blocked("prepared non-reference member bytes 누락"))?;
    let byte_length = strict_json::canonical_u64(member, "byte_length", "prepared member")?;
    if u64::try_from(bytes_utf8.len()).ok() != Some(byte_length)
        || sha256_bytes(bytes_utf8.as_bytes()) != required_string(member, "sha256")?
    {
        return Err(AppError::blocked(
            "prepared member byte/hash binding 불일치",
        ));
    }
    let kind = PreparedMemberKind::parse(&required_string(member, "member_kind")?)?;
    let event_id = optional_string(binding, "event_id")?;
    let semantic_role_rank = match kind {
        PreparedMemberKind::WorkflowSnapshot | PreparedMemberKind::WorkflowPointer => {
            derive_workflow_role_rank(
                event_id.as_deref(),
                context.intent_kind,
                context.semantic_events,
            )?
        }
        _ => 0,
    };
    Ok(PreparedMember {
        kind,
        path: required_string(member, "path")?,
        schema_version: strict_json::canonical_u64(member, "schema_version", "prepared member")?,
        binding: PreparedMemberBinding {
            artifact_id: optional_string(binding, "artifact_id")?,
            causal_id: optional_string(binding, "causal_id")?,
            source_key: optional_string(binding, "source_key")?,
            event_id,
        },
        bytes_utf8,
        expected_type: required_string(member, "expected_type")?,
        expected_identity: optional_string(member, "expected_identity")?,
        readonly: required_bool(permissions, "readonly")?,
        mode: required_u32(permissions, "mode")?,
        ownership: optional_string(member, "ownership")?,
        semantic_role_rank,
    })
}

pub(super) fn render_semantic_events(
    events: &[crate::app::workflow_adapter::ledger::LedgerEvent],
) -> String {
    let rows = events
        .iter()
        .map(render_semantic_event)
        .collect::<Vec<_>>()
        .join(",");
    format!("[{rows}]")
}

pub(super) fn render_semantic_event(
    event: &crate::app::workflow_adapter::ledger::LedgerEvent,
) -> String {
    format!(
        "{{\"schema_version\":1,\"event_id\":\"{}\",\"ts_ms\":{},\"event_type\":\"{}\",\"project_id\":\"{}\",\"session_id\":\"{}\",\"summary\":\"{}\",\"details\":\"{}\"}}",
        crate::app::workflow_adapter::ledger::json_string(&event.event_id),
        event.ts_ms,
        crate::app::workflow_adapter::ledger::json_string(&event.event_type),
        crate::app::workflow_adapter::ledger::json_string(&event.project_id),
        crate::app::workflow_adapter::ledger::json_string(&event.session_id),
        crate::app::workflow_adapter::ledger::json_string(&event.summary),
        crate::app::workflow_adapter::ledger::json_string(&event.details),
    )
}

pub(super) fn parse_semantic_events(
    object: &CanonicalObject,
) -> Result<Vec<crate::app::workflow_adapter::ledger::LedgerEvent>, AppError> {
    let Some(CanonicalValue::Array(values)) = object.get("semantic_events") else {
        return Err(AppError::blocked("prepared semantic_events type 불일치"));
    };
    values
        .iter()
        .map(|value| {
            let CanonicalValue::Object(event) = value else {
                return Err(AppError::blocked("prepared semantic event type 불일치"));
            };
            require_keys(event, SEMANTIC_EVENT_KEYS)?;
            if strict_json::canonical_u64(event, "schema_version", "semantic event")? != 1 {
                return Err(AppError::blocked("prepared semantic event schema 불일치"));
            }
            Ok(crate::app::workflow_adapter::ledger::LedgerEvent {
                event_id: required_string(event, "event_id")?,
                ts_ms: strict_json::canonical_u128(event, "ts_ms", "semantic event")?,
                event_type: required_string(event, "event_type")?,
                project_id: required_string(event, "project_id")?,
                session_id: required_string(event, "session_id")?,
                summary: required_string(event, "summary")?,
                details: required_string(event, "details")?,
            })
        })
        .collect()
}

pub(super) fn render_event_chain_plan(plan: &[PreparedEventChain]) -> String {
    let rows = plan
        .iter()
        .map(|entry| {
            format!(
                "{{\"event_id\":\"{}\",\"ordinal\":{},\"previous_event_hash\":\"{}\",\"event_hash\":\"{}\"}}",
                crate::app::workflow_adapter::ledger::json_string(&entry.event_id),
                entry.ordinal,
                entry.previous_event_hash,
                entry.event_hash,
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("[{rows}]")
}

pub(super) fn parse_event_chain_plan(
    object: &CanonicalObject,
) -> Result<Vec<PreparedEventChain>, AppError> {
    let Some(CanonicalValue::Array(values)) = object.get("event_chain_plan") else {
        return Err(AppError::blocked("prepared event_chain_plan type 불일치"));
    };
    values
        .iter()
        .map(|value| {
            let CanonicalValue::Object(entry) = value else {
                return Err(AppError::blocked("prepared event chain entry type 불일치"));
            };
            require_keys(entry, EVENT_CHAIN_PLAN_KEYS)?;
            Ok(PreparedEventChain {
                event_id: required_string(entry, "event_id")?,
                ordinal: strict_json::canonical_u64(entry, "ordinal", "event chain plan")?,
                previous_event_hash: required_string(entry, "previous_event_hash")?,
                event_hash: required_string(entry, "event_hash")?,
            })
        })
        .collect()
}

pub(super) fn parse_projection_lag_reference(
    object: &CanonicalObject,
) -> Result<Option<u64>, AppError> {
    match object.get("projection_lag_v1") {
        Some(CanonicalValue::Null) => Ok(None),
        Some(CanonicalValue::Object(reference)) => {
            require_keys(reference, PROJECTION_LAG_REFERENCE_KEYS)?;
            if required_string(reference, "member_kind")? != "projection_lag" {
                return Err(AppError::blocked(
                    "prepared projection lag reference kind 불일치",
                ));
            }
            Ok(Some(strict_json::canonical_u64(
                reference,
                "member_index",
                "projection lag reference",
            )?))
        }
        _ => Err(AppError::blocked("prepared projection_lag_v1 type 불일치")),
    }
}

fn derive_workflow_role_rank(
    event_id: Option<&str>,
    intent_kind: &str,
    semantic_events: &[crate::app::workflow_adapter::ledger::LedgerEvent],
) -> Result<u8, AppError> {
    match intent_kind {
        "approve-patch" if semantic_events.len() == 10 => match event_id {
            Some(value) if value == semantic_events[1].event_id => Ok(0),
            Some(value) if value == semantic_events[9].event_id => Ok(1),
            _ => Err(AppError::blocked(
                "prepared workflow member event/role binding 불일치",
            )),
        },
        "approve-verification" if semantic_events.len() == 3 => match event_id {
            Some(value) if value == semantic_events[1].event_id => Ok(0),
            _ => Err(AppError::blocked(
                "prepared verification workflow member event binding 불일치",
            )),
        },
        kind if is_terminal_action_intent_kind(kind) && semantic_events.len() == 3 => {
            match event_id {
                Some(value) if value == semantic_events[1].event_id => Ok(0),
                _ => Err(AppError::blocked(
                    "prepared terminal workflow member event binding 불일치",
                )),
            }
        }
        "checkpoint-workflow" if semantic_events.len() == 1 => match event_id {
            Some(value) if value == semantic_events[0].event_id => Ok(0),
            _ => Err(AppError::blocked(
                "prepared checkpoint workflow member event binding 불일치",
            )),
        },
        _ => Err(AppError::blocked(
            "prepared workflow member event plan 불일치",
        )),
    }
}

pub(super) fn prepared_member_order(
    left: &PreparedMember,
    right: &PreparedMember,
) -> std::cmp::Ordering {
    (
        left.kind.rank(),
        left.path.as_bytes(),
        left.semantic_role_rank,
        left.binding.artifact_id.as_deref(),
        left.binding.causal_id.as_deref(),
        left.binding.source_key.as_deref(),
        left.binding.event_id.as_deref(),
    )
        .cmp(&(
            right.kind.rank(),
            right.path.as_bytes(),
            right.semantic_role_rank,
            right.binding.artifact_id.as_deref(),
            right.binding.causal_id.as_deref(),
            right.binding.source_key.as_deref(),
            right.binding.event_id.as_deref(),
        ))
}
