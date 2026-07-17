use super::*;

pub(crate) fn render_prepared_source_bundle(
    bundle: &PreparedSourceBundle,
) -> Result<String, AppError> {
    validate_prepared_source_bundle(bundle)?;
    let source = bundle
        .source_install
        .as_ref()
        .map(render_source_install_v1)
        .transpose()?
        .unwrap_or_else(|| "null".to_string());
    let members = render_source_members(bundle)?;
    let semantic_events = render_semantic_events(&bundle.semantic_events);
    let event_chain_plan = render_event_chain_plan(&bundle.event_chain_plan);
    let projection_lag = bundle
        .projection_lag_member_index
        .map(|index| format!("{{\"member_kind\":\"projection_lag\",\"member_index\":{index}}}"))
        .unwrap_or_else(|| "null".to_string());
    let body = format!(
        "{{\"schema_version\":1,\"intent_id\":\"{}\",\"intent_kind\":\"{}\",\"project_id\":\"{}\",\"session_id\":\"{}\",\"workflow_id\":{},\"prepared_at_ms\":{},\"before_binding\":{{\"current_revision\":{},\"current_artifact_hash\":\"{}\",\"ledger_count\":{},\"ledger_event_id\":{},\"ledger_hash\":\"{}\"}},\"members\":{},\"semantic_events\":{},\"event_chain_plan\":{},\"source_install_v1\":{},\"projection_lag_v1\":{}}}",
        crate::app::workflow_adapter::ledger::json_string(&bundle.intent_id),
        bundle.intent_kind,
        crate::app::workflow_adapter::ledger::json_string(&bundle.project_id),
        crate::app::workflow_adapter::ledger::json_string(&bundle.session_id),
        render_optional_string(bundle.workflow_id.as_deref()),
        bundle.prepared_at_ms,
        bundle.current_revision,
        bundle.current_artifact_hash,
        bundle.ledger_binding.event_count,
        render_optional_string(bundle.ledger_binding.event_id.as_deref()),
        bundle.ledger_binding.event_hash,
        members,
        semantic_events,
        event_chain_plan,
        source,
        projection_lag,
    );
    enforce_byte_limit(
        body.len(),
        MAX_PREPARED_BUNDLE_BYTES,
        "prepared bundle byte limit 초과",
    )?;
    Ok(body)
}

pub(crate) fn parse_prepared_source_bundle(body: &str) -> Result<PreparedSourceBundle, AppError> {
    enforce_byte_limit(
        body.len(),
        MAX_PREPARED_BUNDLE_BYTES,
        "prepared bundle byte limit 초과",
    )?;
    let object =
        strict_json::parse_canonical_object(body, PREPARED_BUNDLE_KEYS, "prepared source bundle")?;
    if strict_json::canonical_u64(&object, "schema_version", "prepared source bundle")? != 1 {
        return Err(AppError::blocked(
            "prepared source bundle schema/kind 불일치",
        ));
    }
    let intent_kind = required_string(&object, "intent_kind")?;
    if !matches!(
        intent_kind.as_str(),
        "approve-patch" | "approve-verification"
    ) && !is_state_transition_intent_kind(&intent_kind)
        && !is_terminal_action_intent_kind(&intent_kind)
    {
        return Err(AppError::blocked(
            "prepared source bundle intent kind 불일치",
        ));
    }
    let workflow_id = optional_string(&object, "workflow_id")?;
    let before_binding = required_object(&object, "before_binding")?;
    require_keys(before_binding, BEFORE_BINDING_KEYS)?;
    let source_install = match object.get("source_install_v1") {
        Some(CanonicalValue::Object(source_object)) => Some(parse_source_install_v1(
            &strict_json::render_canonical_object(source_object),
        )?),
        Some(CanonicalValue::Null) => None,
        _ => return Err(AppError::blocked("prepared source_install_v1 type 불일치")),
    };
    let semantic_events = parse_semantic_events(&object)?;
    let prepared_at_ms = required_u128(&object, "prepared_at_ms")?;
    let project_id = required_string(&object, "project_id")?;
    let session_id = required_string(&object, "session_id")?;
    let intent_id = required_string(&object, "intent_id")?;
    let member_context = PreparedMemberParseContext {
        prepared_at_ms,
        project_id: &project_id,
        session_id: &session_id,
        workflow_id: workflow_id.as_deref(),
        intent_id: &intent_id,
        intent_kind: &intent_kind,
        semantic_events: &semantic_events,
    };
    let (before_bytes, proposed_bytes, additional_members) =
        if let Some(source) = source_install.as_ref() {
            let (before, proposed, additional) =
                parse_source_members(&object, source, &member_context)?;
            (Some(before), Some(proposed), additional)
        } else {
            (
                None,
                None,
                parse_additional_members(&object, &member_context)?,
            )
        };
    let event_chain_plan = parse_event_chain_plan(&object)?;
    let projection_lag_member_index = parse_projection_lag_reference(&object)?;
    let bundle = PreparedSourceBundle {
        intent_id,
        intent_kind,
        project_id,
        session_id,
        workflow_id,
        prepared_at_ms,
        current_revision: strict_json::canonical_u64(
            before_binding,
            "current_revision",
            "prepared source bundle",
        )?,
        current_artifact_hash: required_string(before_binding, "current_artifact_hash")?,
        ledger_binding: crate::app::workflow_adapter::ledger::LedgerBinding {
            event_count: strict_json::canonical_u64(
                before_binding,
                "ledger_count",
                "prepared source bundle",
            )?,
            event_id: optional_string(before_binding, "ledger_event_id")?,
            event_hash: required_string(before_binding, "ledger_hash")?,
        },
        source_install,
        before_bytes,
        proposed_bytes,
        additional_members,
        semantic_events,
        event_chain_plan,
        projection_lag_member_index,
    };
    validate_prepared_source_bundle(&bundle)?;
    if render_prepared_source_bundle(&bundle)? != body {
        return Err(AppError::blocked(
            "prepared source bundle canonical re-render 불일치",
        ));
    }
    Ok(bundle)
}
