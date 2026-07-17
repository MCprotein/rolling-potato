use super::*;

pub(in crate::app::workflow_adapter::state) fn parse_current_state(
    body: &str,
    context: &str,
) -> Result<CurrentStateSnapshot, AppError> {
    let value = strict_json::parse_value(body, context)?;
    let strict_json::Value::Object(root) = &value else {
        return Err(AppError::blocked(format!(
            "{context} 차단\n- 이유: root must be object"
        )));
    };
    let schema = strict_json::number(root, "schema_version", context)?;
    match schema {
        1 => parse_current_state_v1(body, value, context),
        2 => parse_current_state_v2(body, context),
        _ => Err(AppError::blocked(format!(
            "{context} 차단\n- 이유: unsupported schema version"
        ))),
    }
}

fn parse_current_state_v1(
    body: &str,
    value: strict_json::Value,
    context: &str,
) -> Result<CurrentStateSnapshot, AppError> {
    let object = strict_json::parse_object(body, CURRENT_STATE_V1_KEYS, context)?;
    require_exact_key_set(&object, CURRENT_STATE_V1_KEYS, context)?;
    validate_terminal_states(object.get("terminal_states"), context)?;
    let active_workflow = match object.get("active_workflow") {
        Some(strict_json::Value::Null) => None,
        Some(strict_json::Value::String(workflow_id)) => {
            validate_current_id(workflow_id, "workflow_id", context)?;
            Some(CurrentWorkflowBinding {
                workflow_id: workflow_id.clone(),
                revision: 0,
                artifact_hash: String::new(),
            })
        }
        _ => return Err(current_state_field_error(context, "active_workflow")),
    };
    let project_id = strict_json::string(&object, "project_id", context)?;
    let session_id = strict_json::string(&object, "session_id", context)?;
    validate_current_id(&project_id, "project_id", context)?;
    validate_current_id(&session_id, "session_id", context)?;
    let canonical = strict_json::render_compact(&value);
    Ok(CurrentStateSnapshot {
        schema_version: 1,
        revision: 0,
        previous_artifact_hash: String::new(),
        project_id,
        project_root: strict_json::string(&object, "project_root", context)?,
        session_id,
        active_workflow,
        parent_session_id: optional_string(&object, "parent_session_id", context)?,
        branch_from_event_id: optional_string(&object, "branch_from_event_id", context)?,
        compaction_boundary: optional_string(&object, "compaction_boundary", context)?,
        resume_source: optional_string(&object, "resume_source", context)?,
        ledger_binding: ledger::validated_ledger_binding()?,
        artifact_hash: String::new(),
        legacy_canonical_hash: Some(sha256_text(&canonical)),
    })
}

pub(in crate::app::workflow_adapter::state) fn parse_current_state_v2(
    body: &str,
    context: &str,
) -> Result<CurrentStateSnapshot, AppError> {
    let canonical = strict_json::parse_canonical_object(body, CURRENT_STATE_V2_KEYS, context)?;
    if strict_json::canonical_u64(&canonical, "schema_version", context)? != 2 {
        return Err(current_state_field_error(context, "schema_version"));
    }
    let canonical_revision = strict_json::canonical_u64(&canonical, "revision", context)?;
    let object = strict_json::parse_object_exact_order(body, CURRENT_STATE_V2_KEYS, context)?;
    let revision = strict_json::number(&object, "revision", context)?;
    if revision == 0 || revision != canonical_revision {
        return Err(current_state_field_error(context, "revision"));
    }
    let previous_artifact_hash = strict_json::string(&object, "previous_artifact_hash", context)?;
    if previous_artifact_hash != "none" && !is_sha256(&previous_artifact_hash) {
        return Err(current_state_field_error(context, "previous_artifact_hash"));
    }
    let project_id = strict_json::string(&object, "project_id", context)?;
    let session_id = strict_json::string(&object, "session_id", context)?;
    validate_current_id(&project_id, "project_id", context)?;
    validate_current_id(&session_id, "session_id", context)?;
    let active_workflow = parse_current_workflow(object.get("active_workflow"), context)?;
    validate_terminal_states(object.get("terminal_states"), context)?;
    let ledger_binding = parse_current_ledger_binding(object.get("ledger_binding"), context)?;
    let artifact_hash = strict_json::string(&object, "artifact_hash", context)?;
    if !is_sha256(&artifact_hash) {
        return Err(current_state_field_error(context, "artifact_hash"));
    }
    let snapshot = CurrentStateSnapshot {
        schema_version: 2,
        revision,
        previous_artifact_hash,
        project_id,
        project_root: strict_json::string(&object, "project_root", context)?,
        session_id,
        active_workflow,
        parent_session_id: optional_string(&object, "parent_session_id", context)?,
        branch_from_event_id: optional_string(&object, "branch_from_event_id", context)?,
        compaction_boundary: optional_string(&object, "compaction_boundary", context)?,
        resume_source: optional_string(&object, "resume_source", context)?,
        ledger_binding,
        artifact_hash,
        legacy_canonical_hash: None,
    };
    let payload = render_current_state_v2_payload(&snapshot);
    if sha256_text(&payload) != snapshot.artifact_hash || render_current_state_v2(&snapshot) != body
    {
        return Err(AppError::blocked(format!(
            "{context} 차단\n- 이유: artifact hash 또는 canonical re-render 불일치"
        )));
    }
    Ok(snapshot)
}

pub(in crate::app::workflow_adapter::state) fn render_current_state_v2(
    snapshot: &CurrentStateSnapshot,
) -> String {
    let payload = render_current_state_v2_payload(snapshot);
    format!(
        "{},\"artifact_hash\":\"{}\"}}",
        payload
            .strip_suffix('}')
            .expect("current-state payload object"),
        snapshot.artifact_hash
    )
}

pub(in crate::app::workflow_adapter::state) fn render_current_state_v2_payload(
    snapshot: &CurrentStateSnapshot,
) -> String {
    let active_workflow = snapshot
        .active_workflow
        .as_ref()
        .map(|workflow| {
            format!(
                "{{\"workflow_id\":\"{}\",\"revision\":{},\"artifact_hash\":\"{}\"}}",
                ledger::json_string(&workflow.workflow_id),
                workflow.revision,
                workflow.artifact_hash
            )
        })
        .unwrap_or_else(|| "null".to_string());
    let event_id = snapshot
        .ledger_binding
        .event_id
        .as_ref()
        .map(|value| format!("\"{}\"", ledger::json_string(value)))
        .unwrap_or_else(|| "null".to_string());
    format!(
        "{{\"schema_version\":2,\"revision\":{},\"previous_artifact_hash\":\"{}\",\"project_id\":\"{}\",\"project_root\":\"{}\",\"session_id\":\"{}\",\"active_workflow\":{},\"parent_session_id\":{},\"branch_from_event_id\":{},\"compaction_boundary\":{},\"resume_source\":{},\"terminal_states\":[\"complete\",\"failed\",\"cancelled\"],\"ledger_binding\":{{\"event_count\":{},\"event_id\":{},\"event_hash\":\"{}\"}}}}",
        snapshot.revision,
        snapshot.previous_artifact_hash,
        ledger::json_string(&snapshot.project_id),
        ledger::json_string(&snapshot.project_root),
        ledger::json_string(&snapshot.session_id),
        active_workflow,
        render_optional_string(snapshot.parent_session_id.as_deref()),
        render_optional_string(snapshot.branch_from_event_id.as_deref()),
        render_optional_string(snapshot.compaction_boundary.as_deref()),
        render_optional_string(snapshot.resume_source.as_deref()),
        snapshot.ledger_binding.event_count,
        event_id,
        snapshot.ledger_binding.event_hash,
    )
}

fn render_optional_string(value: Option<&str>) -> String {
    value
        .map(|value| format!("\"{}\"", ledger::json_string(value)))
        .unwrap_or_else(|| "null".to_string())
}

fn parse_current_workflow(
    value: Option<&strict_json::Value>,
    context: &str,
) -> Result<Option<CurrentWorkflowBinding>, AppError> {
    match value {
        Some(strict_json::Value::Null) => Ok(None),
        Some(strict_json::Value::Object(object)) => {
            let expected = ["workflow_id", "revision", "artifact_hash"];
            require_exact_key_order(object, &expected, context)?;
            let workflow_id = strict_json::string(object, "workflow_id", context)?;
            validate_current_id(&workflow_id, "workflow_id", context)?;
            let revision = strict_json::number(object, "revision", context)?;
            let artifact_hash = strict_json::string(object, "artifact_hash", context)?;
            if revision == 0 || !is_sha256(&artifact_hash) {
                return Err(current_state_field_error(context, "active_workflow"));
            }
            Ok(Some(CurrentWorkflowBinding {
                workflow_id,
                revision,
                artifact_hash,
            }))
        }
        _ => Err(current_state_field_error(context, "active_workflow")),
    }
}

fn parse_current_ledger_binding(
    value: Option<&strict_json::Value>,
    context: &str,
) -> Result<ledger::LedgerBinding, AppError> {
    let Some(strict_json::Value::Object(object)) = value else {
        return Err(current_state_field_error(context, "ledger_binding"));
    };
    let expected = ["event_count", "event_id", "event_hash"];
    require_exact_key_order(object, &expected, context)?;
    let event_count = strict_json::number(object, "event_count", context)?;
    let event_id = optional_string(object, "event_id", context)?;
    if let Some(event_id) = event_id.as_deref() {
        validate_current_id(event_id, "event_id", context)?;
    }
    let event_hash = strict_json::string(object, "event_hash", context)?;
    if (event_count == 0 && (event_id.is_some() || event_hash != "root"))
        || (event_count > 0 && (event_id.is_none() || !is_sha256(&event_hash)))
    {
        return Err(current_state_field_error(context, "ledger_binding"));
    }
    Ok(ledger::LedgerBinding {
        event_count,
        event_id,
        event_hash,
    })
}

fn optional_string(
    object: &strict_json::Object,
    key: &str,
    context: &str,
) -> Result<Option<String>, AppError> {
    match object.get(key) {
        Some(strict_json::Value::Null) => Ok(None),
        Some(strict_json::Value::String(value)) => Ok(Some(value.clone())),
        _ => Err(current_state_field_error(context, key)),
    }
}

fn validate_terminal_states(
    value: Option<&strict_json::Value>,
    context: &str,
) -> Result<(), AppError> {
    let Some(strict_json::Value::Array(values)) = value else {
        return Err(current_state_field_error(context, "terminal_states"));
    };
    let actual = values
        .iter()
        .map(|value| match value {
            strict_json::Value::String(value) => Some(value.as_str()),
            _ => None,
        })
        .collect::<Option<Vec<_>>>();
    if actual.as_deref() == Some(["complete", "failed", "cancelled"].as_slice()) {
        Ok(())
    } else {
        Err(current_state_field_error(context, "terminal_states"))
    }
}

fn require_exact_key_set(
    object: &strict_json::Object,
    keys: &[&str],
    context: &str,
) -> Result<(), AppError> {
    if object.len() == keys.len() && keys.iter().all(|key| object.contains_key(key)) {
        Ok(())
    } else {
        Err(AppError::blocked(format!(
            "{context} 차단\n- 이유: exact key set 불일치"
        )))
    }
}

fn require_exact_key_order(
    object: &strict_json::Object,
    keys: &[&str],
    context: &str,
) -> Result<(), AppError> {
    let actual = object.keys().map(String::as_str).collect::<Vec<_>>();
    if actual == keys {
        Ok(())
    } else {
        Err(AppError::blocked(format!(
            "{context} 차단\n- 이유: exact nested key order 불일치"
        )))
    }
}

fn validate_current_id(value: &str, field: &str, context: &str) -> Result<(), AppError> {
    let valid = !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'));
    if valid {
        Ok(())
    } else {
        Err(current_state_field_error(context, field))
    }
}

fn current_state_field_error(context: &str, field: &str) -> AppError {
    AppError::blocked(format!(
        "{context} 차단\n- 이유: invalid current-state field\n- field: {field}"
    ))
}
