use crate::foundation::error::AppError;
use crate::runtime_core::inference::cancellation::RequestCancellationToken;
use crate::runtime_core::inference::generation_policy::GenerationIntent;
use crate::surfaces::tui::runtime_bridge::TuiConversationTurn;

use super::super::super::session_memory::ConversationToolActivity;
use super::prompt::assemble_plain_prompt_with_runtime_evidence;

pub(in crate::app::tui_adapter) struct BoundedReplyRequest<'a> {
    pub(in crate::app::tui_adapter) user_request: &'a str,
    pub(in crate::app::tui_adapter) local_context: &'a str,
    pub(in crate::app::tui_adapter) runtime_evidence: &'a str,
    pub(in crate::app::tui_adapter) history: &'a [TuiConversationTurn],
    pub(in crate::app::tui_adapter) tool_activities: &'a [ConversationToolActivity],
    pub(in crate::app::tui_adapter) context_limit_tokens: u32,
    pub(in crate::app::tui_adapter) timeout_ms: u32,
    pub(in crate::app::tui_adapter) cancellation: &'a RequestCancellationToken,
}

pub(in crate::app::tui_adapter) fn reply_with_context_and_cancel_bounded(
    request: BoundedReplyRequest<'_>,
) -> Result<String, AppError> {
    request.cancellation.check()?;
    let prompt = assemble_plain_prompt_with_runtime_evidence(
        request.user_request,
        request.local_context,
        request.runtime_evidence,
        request.history,
        request.tool_activities,
        request.context_limit_tokens,
    )?
    .text;
    crate::app::inference_adapter::answer::generate_for_user_with_cancel_bounded(
        &prompt,
        request.user_request,
        GenerationIntent::InteractiveAnswer,
        request.timeout_ms,
        request.cancellation,
    )
}
