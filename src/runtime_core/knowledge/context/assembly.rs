use crate::foundation::error::AppError;

use super::super::compaction::{
    estimate_tokens, truncate_head_to_tokens, truncate_tail_to_estimated_tokens,
};
use super::budget::AgentPromptBudget;
use super::types::{AgentPromptParts, AssembledAgentPrompt};

const AGENT_SECTION_SEPARATOR_RESERVE_TOKENS: usize = 16;

pub(crate) fn assemble_agent_prompt(
    budget: AgentPromptBudget,
    parts: AgentPromptParts<'_>,
) -> Result<AssembledAgentPrompt, AppError> {
    let current_request = format!(
        "<CURRENT_USER_REQUEST>\n{}\n</CURRENT_USER_REQUEST>\n\n{}",
        parts.current_request, parts.response_cue
    );
    let mandatory_tokens = estimate_tokens(parts.instructions)
        .saturating_add(estimate_tokens(&current_request))
        .saturating_add(AGENT_SECTION_SEPARATOR_RESERVE_TOKENS);
    if mandatory_tokens > budget.input_limit_tokens {
        return Err(AppError::blocked(format!(
            "현재 agent 요청과 필수 instruction이 활성 runtime의 입력 예산을 초과했습니다.\n- input limit: {} tokens\n- mandatory input: {mandatory_tokens} tokens",
            budget.input_limit_tokens
        )));
    }

    let mut remaining = budget.input_limit_tokens - mandatory_tokens;
    let resume_context = bounded_untrusted_section(
        "RESUME_CONTEXT",
        parts.resume_context,
        remaining.saturating_mul(2) / 3,
        ContextEdge::Tail,
    );
    remaining = remaining.saturating_sub(estimate_tokens(&resume_context));
    let repository_context = bounded_untrusted_section(
        "REPOSITORY_CONTEXT",
        parts.repository_context,
        remaining,
        ContextEdge::Head,
    );

    let mut sections = vec![parts.instructions.trim().to_string()];
    push_nonempty(&mut sections, resume_context);
    push_nonempty(&mut sections, repository_context);
    sections.push(current_request);
    let text = sections.join("\n\n");
    let estimated_tokens = estimate_tokens(&text);
    if estimated_tokens > budget.input_limit_tokens {
        return Err(AppError::blocked(format!(
            "조립된 agent prompt가 활성 runtime의 입력 상한을 초과했습니다.\n- estimated: {estimated_tokens} tokens\n- input limit: {} tokens",
            budget.input_limit_tokens
        )));
    }
    Ok(AssembledAgentPrompt {
        text,
        estimated_tokens,
        input_limit_tokens: budget.input_limit_tokens,
    })
}

#[derive(Clone, Copy)]
enum ContextEdge {
    Head,
    Tail,
}

fn bounded_untrusted_section(
    label: &str,
    content: &str,
    budget_tokens: usize,
    edge: ContextEdge,
) -> String {
    if content.trim().is_empty() || budget_tokens == 0 {
        return String::new();
    }
    let opening = format!("<{label} trust=\"untrusted\">\n");
    let closing = format!("\n</{label}>");
    let wrapper_tokens = estimate_tokens(&opening).saturating_add(estimate_tokens(&closing));
    if wrapper_tokens >= budget_tokens {
        return String::new();
    }
    let content_budget = budget_tokens - wrapper_tokens;
    let bounded = match edge {
        ContextEdge::Head => truncate_head_to_tokens(content, content_budget),
        ContextEdge::Tail => truncate_tail_to_estimated_tokens(content, content_budget),
    };
    format!("{opening}{bounded}{closing}")
}

fn push_nonempty(sections: &mut Vec<String>, value: String) {
    if !value.trim().is_empty() {
        sections.push(value);
    }
}
