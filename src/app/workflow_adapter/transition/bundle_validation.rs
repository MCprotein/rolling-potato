use super::*;

mod event_chain;
mod members;
pub(super) use event_chain::validate_event_chain;
use members::validate_additional_members;

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
