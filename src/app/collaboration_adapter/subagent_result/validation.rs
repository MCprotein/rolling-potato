use super::super::subagent::SubagentRecordV1;
use crate::app::context_adapter::ContextPack;
use crate::app::workflow_adapter::ledger;
use crate::foundation::error::AppError;
use crate::runtime_core::collaboration::subagent_result::{
    self as result_policy, validate_context_binding, ResultBinding, SourcePointerBinding,
    SubagentResultV1,
};

pub(super) fn parse_result(
    record: &SubagentRecordV1,
    context: &ContextPack,
    body: &str,
) -> Result<SubagentResultV1, AppError> {
    let result = parse_result_shape(record, body)?;
    validate_context_binding(record, &result, &source_pointer_bindings(context))?;
    Ok(result)
}

pub(super) fn source_pointer_bindings(context: &ContextPack) -> Vec<SourcePointerBinding<'_>> {
    context
        .source_pointers
        .iter()
        .map(|pointer| SourcePointerBinding {
            path: &pointer.path,
            stable_ref: &pointer.stable_ref,
            fingerprint: &pointer.fingerprint,
        })
        .collect()
}

pub(super) fn parse_result_shape(
    record: &SubagentRecordV1,
    body: &str,
) -> Result<SubagentResultV1, AppError> {
    let result = result_policy::parse_result_shape(
        &ResultBinding {
            subagent_id: &record.subagent_id,
            parent_workflow_id: &record.parent_workflow_id,
            role: record.role,
        },
        body,
    )?;
    if result_text_fields(&result).any(ledger::contains_sensitive_text) {
        return Err(AppError::blocked("subagent result sensitive output 차단"));
    }
    Ok(result)
}

fn result_text_fields(result: &SubagentResultV1) -> impl Iterator<Item = &str> {
    std::iter::once(result.summary.as_str())
        .chain(result.findings.iter().map(String::as_str))
        .chain(result.validation_gaps.iter().map(String::as_str))
        .chain(std::iter::once(result.suggested_next_action.as_str()))
        .chain(
            result
                .patch_proposal
                .iter()
                .flat_map(|patch| [patch.find_text.as_str(), patch.replacement_text.as_str()]),
        )
}
