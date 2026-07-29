use crate::foundation::error::AppError;
use crate::foundation::serialization as strict_json;

use super::super::manifest::DefaultSelection;

pub(crate) fn parse_default_selection(text: &str) -> Result<DefaultSelection, AppError> {
    let context = "default model selection";
    let object = strict_json::parse_object(
        text,
        &["schemaVersion", "modelId", "artifactSha256", "selectedAtMs"],
        context,
    )?;
    if strict_json::number(&object, "schemaVersion", context)? != 1 {
        return Err(AppError::blocked("default model schemaVersion 불일치"));
    }
    Ok(DefaultSelection {
        model_id: strict_json::string(&object, "modelId", context)?,
        artifact_sha256: strict_json::string(&object, "artifactSha256", context)?,
        selected_at_ms: strict_json::number(&object, "selectedAtMs", context)?,
    })
}
