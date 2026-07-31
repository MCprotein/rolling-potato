use crate::app::tui_adapter::conversation;
use crate::app::tui_adapter::session_memory::ConversationToolActivity;
use crate::foundation::error::AppError;
use crate::surfaces::tui::runtime_bridge::TuiConversationTurn;

pub(in crate::app::tui_adapter::runtime::request) fn web_conversation_context(
    history: &[TuiConversationTurn],
    tool_activities: &[ConversationToolActivity],
    user_request: &str,
    context_limit_tokens: Option<u32>,
) -> Result<String, AppError> {
    if history.is_empty() {
        return Ok(String::new());
    }
    conversation::render_web_conversation_context(
        history,
        tool_activities,
        user_request,
        required_context_limit(context_limit_tokens)?,
    )
}

pub(in crate::app::tui_adapter::runtime::request) fn required_context_limit(
    context_limit_tokens: Option<u32>,
) -> Result<u32, AppError> {
    context_limit_tokens.filter(|value| *value > 0).ok_or_else(|| {
        AppError::blocked(
            "선택한 모델의 context length를 확인하지 못했습니다. /model에서 모델을 다시 선택하거나 /doctor로 backend 상태를 확인하세요.",
        )
    })
}
