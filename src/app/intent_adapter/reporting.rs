use crate::app::context_adapter::{ContextPack, ResumeContext};
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::runtime_core::patch::intent::{IntentDecision, ParsedModelAction};

pub(super) fn is_non_mutating_action(kind: &str) -> bool {
    matches!(
        kind,
        "answer-only" | "inspect-sources" | "generated-artifact-plan"
    )
}

pub(super) fn render_non_mutating_report(
    request: &str,
    decision: &IntentDecision,
    context_pack: &ContextPack,
    resume_context: &ResumeContext,
    model_action: &ParsedModelAction,
    answer: &str,
    workflow: &state::WorkflowRecord,
) -> String {
    format!(
        "run 결과\n- 상태: 완료\n- 요청: {}\n- 선택한 skill: {}\n- mode: {}\n- workflow id: {}\n- workflow kind: {}\n- action id: {}\n- action kind: {}\n- resumed context: {}\n- context origin: {}\n- ontology records selected: {}\n- ontology stale rejected: {}\n- source pointers: {}\n- context files read: {}\n- side effect: 없음\n- approval: 불필요\n- 답변:\n{}",
        request,
        decision.skill_id,
        decision.mode,
        workflow.workflow_id,
        workflow.workflow_kind,
        workflow.action_id,
        workflow.action_kind,
        resume_context.summary(),
        context_pack.origin,
        context_pack.ontology_records_selected,
        context_pack.ontology_stale_rejected,
        model_action.source_pointers,
        context_pack.files_read,
        answer
    )
}

pub(super) fn model_transcript_content(
    response: &str,
    action: &ParsedModelAction,
) -> Result<String, AppError> {
    if is_non_mutating_action(&action.kind) {
        return model_answer(response);
    }
    Ok(format!(
        "status={} kind={} source_pointers={} path={} find_sha256={} replace_sha256={} verification_sha256={} next_gate={} requested_side_effects={}",
        action.status,
        action.kind,
        action.source_pointers,
        action.target_path,
        state::sha256_text(&action.find_text),
        state::sha256_text(&action.replace_text),
        state::sha256_text(&action.verification_command),
        action.next_gate,
        action.requested_side_effects
    ))
}

pub(super) fn model_answer(response: &str) -> Result<String, AppError> {
    crate::app::inference_adapter::answer::validate_existing(response).map_err(|error| {
        AppError::blocked(format!(
            "run agent loop 차단\n- 이유: {}\n- 성공 보고: 생성하지 않음",
            error.message
        ))
    })
}
