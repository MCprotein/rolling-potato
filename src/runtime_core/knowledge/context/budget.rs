use crate::foundation::error::AppError;

const MIN_RESUME_TRANSCRIPT_TOKENS: usize = 512;
const MAX_RESUME_TRANSCRIPT_TOKENS: usize = 16_384;
const MIN_RESUME_TURNS: usize = 8;
const MAX_RESUME_TURNS: usize = 64;
const MIN_RESUME_TURN_TOKENS: usize = 256;
const MAX_RESUME_TURN_TOKENS: usize = 4_096;
const MIN_AGENT_RUNTIME_RESERVE_TOKENS: usize = 64;
const MAX_AGENT_RUNTIME_RESERVE_TOKENS: usize = 2_048;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResumeContextBudget {
    pub(crate) context_limit_tokens: usize,
    pub(crate) transcript_budget_tokens: usize,
    pub(crate) per_turn_budget_tokens: usize,
    pub(crate) max_turns: usize,
}

impl ResumeContextBudget {
    pub(crate) fn for_context_limit(context_limit_tokens: usize) -> Self {
        let context_limit_tokens = context_limit_tokens.max(1);
        let transcript_budget_tokens = (context_limit_tokens / 8)
            .clamp(MIN_RESUME_TRANSCRIPT_TOKENS, MAX_RESUME_TRANSCRIPT_TOKENS)
            .min(context_limit_tokens);
        let per_turn_budget_tokens = (transcript_budget_tokens / 2)
            .clamp(MIN_RESUME_TURN_TOKENS, MAX_RESUME_TURN_TOKENS)
            .min(transcript_budget_tokens);
        let max_turns = (context_limit_tokens / 2_048).clamp(MIN_RESUME_TURNS, MAX_RESUME_TURNS);
        Self {
            context_limit_tokens,
            transcript_budget_tokens,
            per_turn_budget_tokens,
            max_turns,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AgentPromptBudget {
    pub(crate) context_limit_tokens: usize,
    pub(crate) output_reserve_tokens: usize,
    pub(crate) runtime_reserve_tokens: usize,
    pub(crate) input_limit_tokens: usize,
}

impl AgentPromptBudget {
    pub(crate) fn for_context_limit(
        context_limit_tokens: usize,
        output_reserve_tokens: usize,
    ) -> Result<Self, AppError> {
        let runtime_reserve_tokens = (context_limit_tokens / 32).clamp(
            MIN_AGENT_RUNTIME_RESERVE_TOKENS,
            MAX_AGENT_RUNTIME_RESERVE_TOKENS,
        );
        let reserved = output_reserve_tokens.saturating_add(runtime_reserve_tokens);
        if context_limit_tokens <= reserved {
            return Err(AppError::blocked(format!(
                "활성 runtime의 context length가 agent prompt를 조립하기에 너무 작습니다.\n- context: {context_limit_tokens} tokens\n- output reserve: {output_reserve_tokens} tokens\n- runtime reserve: {runtime_reserve_tokens} tokens"
            )));
        }
        Ok(Self {
            context_limit_tokens,
            output_reserve_tokens,
            runtime_reserve_tokens,
            input_limit_tokens: context_limit_tokens - reserved,
        })
    }
}
