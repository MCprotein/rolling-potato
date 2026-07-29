use super::super::*;
use super::validation::validate_source_install_v1;

pub(crate) fn render_source_install_v1(plan: &SourceInstallV1) -> Result<String, AppError> {
    validate_source_install_v1(plan)?;
    let operations = plan
        .operations
        .iter()
        .map(|operation| {
            format!(
                "\"{}\"",
                crate::app::workflow_adapter::ledger::json_string(operation)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let body = format!(
        "{{\"schema_version\":{},\"source_key\":\"{}\",\"target\":{},\"before_blob\":{},\"proposed_blob\":{},\"rollback_final\":{},\"install_temp\":{},\"guard_path\":{},\"before_sha256\":\"{}\",\"before_byte_length\":{},\"proposed_sha256\":\"{}\",\"proposed_byte_length\":{},\"permissions\":{},\"ownership\":{},\"platform\":\"{}\",\"unix_metadata\":{},\"operations\":[{}]}}",
        plan.schema_version,
        crate::app::workflow_adapter::ledger::json_string(&plan.source_key),
        render_path(&plan.target),
        render_blob(&plan.before_blob),
        render_blob(&plan.proposed_blob),
        render_path(&plan.rollback_final),
        render_path(&plan.install_temp),
        render_path(&plan.guard_path),
        plan.before_sha256,
        plan.before_byte_length,
        plan.proposed_sha256,
        plan.proposed_byte_length,
        render_permissions(&plan.permissions),
        render_ownership(&plan.ownership),
        plan.platform,
        render_unix_metadata(&plan.unix_metadata),
        operations
    );
    enforce_byte_limit(
        body.len(),
        MAX_SOURCE_INSTALL_BYTES,
        "source_install_v1 byte limit 초과",
    )?;
    Ok(body)
}

pub(crate) fn parse_source_install_v1(body: &str) -> Result<SourceInstallV1, AppError> {
    let object =
        strict_json::parse_canonical_object(body, SOURCE_INSTALL_KEYS, "source_install_v1")?;
    let plan = SourceInstallV1 {
        schema_version: strict_json::canonical_u64(&object, "schema_version", "source_install_v1")?,
        source_key: required_string(&object, "source_key")?,
        target: parse_path(required_object(&object, "target")?)?,
        before_blob: parse_blob(required_object(&object, "before_blob")?)?,
        proposed_blob: parse_blob(required_object(&object, "proposed_blob")?)?,
        rollback_final: parse_path(required_object(&object, "rollback_final")?)?,
        install_temp: parse_path(required_object(&object, "install_temp")?)?,
        guard_path: parse_path(required_object(&object, "guard_path")?)?,
        before_sha256: required_string(&object, "before_sha256")?,
        before_byte_length: strict_json::canonical_u64(
            &object,
            "before_byte_length",
            "source_install_v1",
        )?,
        proposed_sha256: required_string(&object, "proposed_sha256")?,
        proposed_byte_length: strict_json::canonical_u64(
            &object,
            "proposed_byte_length",
            "source_install_v1",
        )?,
        permissions: parse_permissions(required_object(&object, "permissions")?)?,
        ownership: parse_ownership(required_object(&object, "ownership")?)?,
        platform: required_string(&object, "platform")?,
        unix_metadata: parse_unix_metadata(required_object(&object, "unix_metadata")?)?,
        operations: required_string_array(&object, "operations")?,
    };
    validate_source_install_v1(&plan)?;
    if render_source_install_v1(&plan)? != body {
        return Err(AppError::blocked(
            "source_install_v1 canonical re-render 불일치",
        ));
    }
    Ok(plan)
}
