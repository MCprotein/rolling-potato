use crate::foundation::error::AppError;
use crate::runtime_core::inference::cancellation::RequestCancellationToken;
use crate::runtime_core::inference::generation_policy::GenerationIntent;
use crate::surfaces::tui::runtime_bridge::TuiConversationTurn;

use super::super::super::session_memory::ConversationToolActivity;
use super::prompt::assemble_plain_prompt_with_runtime_evidence;

pub(in crate::app::tui_adapter) fn reply_with_context_and_cancel_bounded(
    user_request: &str,
    local_context: &str,
    runtime_evidence: &str,
    history: &[TuiConversationTurn],
    tool_activities: &[ConversationToolActivity],
    context_limit_tokens: u32,
    timeout_ms: u32,
    cancellation: &RequestCancellationToken,
) -> Result<String, AppError> {
    cancellation.check()?;
    let prompt = assemble_plain_prompt_with_runtime_evidence(
        user_request,
        local_context,
        runtime_evidence,
        history,
        tool_activities,
        context_limit_tokens,
    )?
    .text;
    crate::app::inference_adapter::answer::generate_for_user_with_cancel_bounded(
        &prompt,
        user_request,
        GenerationIntent::InteractiveAnswer,
        timeout_ms,
        cancellation,
    )
}
