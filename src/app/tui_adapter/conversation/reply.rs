use crate::foundation::error::AppError;
use crate::runtime_core::inference::backend::BackendChatInput;
use crate::runtime_core::inference::cancellation::RequestCancellationToken;
use crate::runtime_core::inference::generation_policy::GenerationIntent;
use crate::surfaces::tui::runtime_bridge::TuiConversationTurn;

use super::super::session_memory::ConversationToolActivity;

mod bounded;
pub(super) mod prompt;

pub(in crate::app::tui_adapter) use bounded::reply_with_context_and_cancel_bounded;
pub(in crate::app::tui_adapter) use prompt::language_instruction;
use prompt::{assemble_plain_prompt, assemble_vision_prompt};

pub(in crate::app::tui_adapter) fn render_web_conversation_context(
    history: &[TuiConversationTurn],
    tool_activities: &[ConversationToolActivity],
    user_request: &str,
    context_limit_tokens: u32,
) -> Result<String, AppError> {
    super::super::prompt_context::ConversationPromptContext::build(
        history,
        tool_activities,
        user_request,
        context_limit_tokens,
        GenerationIntent::GroundedWebAnswer,
    )
    .map(|context| context.render_memory())
}

pub(in crate::app::tui_adapter) fn reply_with_context_and_cancel(
    user_request: &str,
    local_context: &str,
    history: &[TuiConversationTurn],
    tool_activities: &[ConversationToolActivity],
    context_limit_tokens: u32,
    cancellation: &RequestCancellationToken,
) -> Result<String, AppError> {
    cancellation.check()?;
    let prompt = assemble_plain_prompt(
        user_request,
        local_context,
        history,
        tool_activities,
        context_limit_tokens,
    )?
    .text;
    crate::app::inference_adapter::answer::generate_for_user_with_cancel(
        &prompt,
        user_request,
        GenerationIntent::InteractiveAnswer,
        cancellation,
    )
}

pub(in crate::app::tui_adapter) fn reply_with_images_and_cancel(
    input: &BackendChatInput,
    history: &[TuiConversationTurn],
    tool_activities: &[ConversationToolActivity],
    context_limit_tokens: u32,
    cancellation: &RequestCancellationToken,
) -> Result<String, AppError> {
    cancellation.check()?;
    let mut input = input.clone();
    input.text =
        assemble_vision_prompt(&input, history, tool_activities, context_limit_tokens)?.text;
    crate::app::inference_adapter::answer::generate_input_with_cancel(
        &input,
        GenerationIntent::VisionAnswer,
        cancellation,
    )
}

pub(in crate::app::tui_adapter) fn estimate_context_tokens(
    user_request: &str,
    input: &BackendChatInput,
    history: &[TuiConversationTurn],
    tool_activities: &[ConversationToolActivity],
    context_limit_tokens: u32,
) -> Result<u32, AppError> {
    let prompt = if input.images.is_empty() {
        assemble_plain_prompt(
            user_request,
            &input.text,
            history,
            tool_activities,
            context_limit_tokens,
        )?
    } else {
        assemble_vision_prompt(input, history, tool_activities, context_limit_tokens)?
    };
    Ok(u32::try_from(prompt.estimated_tokens)
        .unwrap_or(u32::MAX)
        .min(context_limit_tokens))
}
