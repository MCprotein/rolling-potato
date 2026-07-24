//! Model-window-aware prompt budgeting and deterministic layer assembly.

use crate::foundation::error::AppError;

use super::compaction::{
    estimate_tokens, truncate_head_to_tokens, truncate_tail_to_estimated_tokens,
};

const MIN_RUNTIME_RESERVE_TOKENS: usize = 512;
const MAX_RUNTIME_RESERVE_TOKENS: usize = 4_096;
const LAYER_SEPARATOR_RESERVE_TOKENS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PromptBudget {
    pub(crate) context_limit_tokens: usize,
    pub(crate) output_reserve_tokens: usize,
    pub(crate) runtime_reserve_tokens: usize,
    pub(crate) input_limit_tokens: usize,
    pub(crate) typed_memory_target_tokens: usize,
    pub(crate) recall_target_tokens: usize,
    pub(crate) recent_target_tokens: usize,
}

impl PromptBudget {
    pub(crate) fn for_context_limit(
        context_limit_tokens: usize,
        output_reserve_tokens: usize,
    ) -> Result<Self, AppError> {
        let runtime_reserve_tokens = (context_limit_tokens / 32)
            .clamp(MIN_RUNTIME_RESERVE_TOKENS, MAX_RUNTIME_RESERVE_TOKENS);
        let reserved = output_reserve_tokens.saturating_add(runtime_reserve_tokens);
        if context_limit_tokens <= reserved {
            return Err(AppError::blocked(format!(
                "선택한 모델의 context length가 prompt를 조립하기에 너무 작습니다.\n- context: {context_limit_tokens} tokens\n- output reserve: {output_reserve_tokens} tokens\n- runtime reserve: {runtime_reserve_tokens} tokens"
            )));
        }
        let input_limit_tokens = context_limit_tokens - reserved;
        Ok(Self {
            context_limit_tokens,
            output_reserve_tokens,
            runtime_reserve_tokens,
            input_limit_tokens,
            typed_memory_target_tokens: (input_limit_tokens / 8).clamp(128, 8_192),
            recall_target_tokens: (input_limit_tokens / 4).clamp(256, 32_768),
            recent_target_tokens: (input_limit_tokens / 4).clamp(256, 16_384),
        })
    }
}

pub(crate) struct PromptParts<'a> {
    pub(crate) instructions: &'a str,
    pub(crate) typed_memory: &'a str,
    pub(crate) recalled_history: &'a str,
    pub(crate) recent_history: &'a str,
    pub(crate) attachment_context: &'a str,
    pub(crate) current_user: &'a str,
    pub(crate) response_cue: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AssembledPrompt {
    pub(crate) text: String,
    pub(crate) estimated_tokens: usize,
    pub(crate) context_limit_tokens: usize,
    pub(crate) input_limit_tokens: usize,
}

pub(crate) fn assemble(
    budget: PromptBudget,
    parts: PromptParts<'_>,
) -> Result<AssembledPrompt, AppError> {
    let current_section = format!(
        "<CURRENT_USER_REQUEST>\n{}\n</CURRENT_USER_REQUEST>\n\n{}",
        parts.current_user, parts.response_cue
    );
    let mandatory_tokens = estimate_tokens(parts.instructions)
        .saturating_add(estimate_tokens(&current_section))
        .saturating_add(LAYER_SEPARATOR_RESERVE_TOKENS);
    if mandatory_tokens > budget.input_limit_tokens {
        return Err(AppError::blocked(format!(
            "현재 요청이 선택한 모델의 prompt budget을 초과했습니다.\n- input limit: {} tokens\n- mandatory input: {mandatory_tokens} tokens",
            budget.input_limit_tokens
        )));
    }

    let mut remaining = budget.input_limit_tokens - mandatory_tokens;
    let typed_memory = bounded_head(
        parts.typed_memory,
        budget.typed_memory_target_tokens.min(remaining),
    );
    remaining = remaining.saturating_sub(estimate_tokens(&typed_memory));
    let recalled_history = bounded_head(
        parts.recalled_history,
        budget.recall_target_tokens.min(remaining),
    );
    remaining = remaining.saturating_sub(estimate_tokens(&recalled_history));
    let recent_history = bounded_tail(
        parts.recent_history,
        budget.recent_target_tokens.min(remaining),
    );
    remaining = remaining.saturating_sub(estimate_tokens(&recent_history));
    let attachment_context = bounded_head(parts.attachment_context, remaining);

    let mut sections = vec![parts.instructions.trim().to_string()];
    push_nonempty(&mut sections, typed_memory);
    push_nonempty(&mut sections, recalled_history);
    push_nonempty(&mut sections, recent_history);
    push_nonempty(&mut sections, attachment_context);
    sections.push(current_section);
    let text = sections.join("\n\n");
    let estimated_tokens = estimate_tokens(&text);
    if estimated_tokens > budget.input_limit_tokens {
        return Err(AppError::blocked(format!(
            "조립된 prompt가 모델 입력 상한을 초과했습니다.\n- estimated: {estimated_tokens} tokens\n- input limit: {} tokens",
            budget.input_limit_tokens
        )));
    }
    Ok(AssembledPrompt {
        text,
        estimated_tokens,
        context_limit_tokens: budget.context_limit_tokens,
        input_limit_tokens: budget.input_limit_tokens,
    })
}

fn bounded_head(value: &str, budget: usize) -> String {
    if value.trim().is_empty() || budget == 0 {
        String::new()
    } else {
        truncate_head_to_tokens(value, budget)
    }
}

fn bounded_tail(value: &str, budget: usize) -> String {
    if value.trim().is_empty() || budget == 0 {
        String::new()
    } else {
        truncate_tail_to_estimated_tokens(value, budget)
    }
}

fn push_nonempty(sections: &mut Vec<String>, value: String) {
    if !value.trim().is_empty() {
        sections.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_model_uses_its_real_window_without_forcing_every_prompt_to_fill_it() {
        let budget = PromptBudget::for_context_limit(131_072, 384).unwrap();

        assert_eq!(budget.context_limit_tokens, 131_072);
        assert_eq!(budget.runtime_reserve_tokens, 4_096);
        assert_eq!(budget.input_limit_tokens, 126_592);
        assert!(budget.recent_target_tokens < budget.input_limit_tokens);
        assert!(budget.recall_target_tokens < budget.input_limit_tokens);
    }

    #[test]
    fn assembly_keeps_current_user_last_and_stays_inside_the_model_window() {
        let budget = PromptBudget::for_context_limit(4_096, 384).unwrap();
        let assembled = assemble(
            budget,
            PromptParts {
                instructions: "stable instructions",
                typed_memory: &"memory ".repeat(2_000),
                recalled_history: &"recall ".repeat(2_000),
                recent_history: &"recent ".repeat(2_000),
                attachment_context: &"attachment ".repeat(2_000),
                current_user: "지금 질문",
                response_cue: "답변:",
            },
        )
        .unwrap();

        assert!(assembled.estimated_tokens <= assembled.input_limit_tokens);
        assert!(assembled
            .text
            .ends_with("<CURRENT_USER_REQUEST>\n지금 질문\n</CURRENT_USER_REQUEST>\n\n답변:"));
    }
}
