//! Bounded, surface-neutral source and resume context.

use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::foundation::error::AppError;

use super::compaction::{
    estimate_tokens, truncate_head_to_tokens, truncate_tail_to_estimated_tokens,
    CompactionCheckpoint,
};

pub(crate) const MAX_CONTEXT_FILES: usize = 4;
pub(crate) const MAX_CONTEXT_CHARS: usize = 3_200;
pub(crate) const MAX_FILE_CHARS: usize = 1_000;
pub(crate) const MAX_FILE_BYTES: u64 = 128 * 1024;
const MIN_RESUME_TRANSCRIPT_TOKENS: usize = 512;
const MAX_RESUME_TRANSCRIPT_TOKENS: usize = 16_384;
const MIN_RESUME_TURNS: usize = 8;
const MAX_RESUME_TURNS: usize = 64;
const MIN_RESUME_TURN_TOKENS: usize = 256;
const MAX_RESUME_TURN_TOKENS: usize = 4_096;
const MIN_AGENT_RUNTIME_RESERVE_TOKENS: usize = 64;
const MAX_AGENT_RUNTIME_RESERVE_TOKENS: usize = 2_048;
const AGENT_SECTION_SEPARATOR_RESERVE_TOKENS: usize = 16;

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

pub(crate) struct AgentPromptParts<'a> {
    pub(crate) instructions: &'a str,
    pub(crate) resume_context: &'a str,
    pub(crate) repository_context: &'a str,
    pub(crate) current_request: &'a str,
    pub(crate) response_cue: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AssembledAgentPrompt {
    pub(crate) text: String,
    pub(crate) estimated_tokens: usize,
    pub(crate) input_limit_tokens: usize,
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextPack {
    pub project_root: PathBuf,
    pub origin: String,
    pub ontology_records_selected: usize,
    pub ontology_stale_rejected: usize,
    pub files_considered: usize,
    pub files_read: usize,
    pub chars_read: usize,
    pub dropped_files: usize,
    pub source_pointers: Vec<SourcePointer>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcePointer {
    pub path: String,
    pub stable_ref: String,
    pub chars: usize,
    pub fingerprint: String,
    pub snippet: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeContext {
    pub session_id: String,
    pub context_limit_tokens: usize,
    pub transcript_records_considered: usize,
    pub transcript_turns_selected: usize,
    pub transcript_tokens: usize,
    pub transcript_chars: usize,
    pub transcript: Vec<(String, String)>,
    pub compacted_checkpoint: Option<CompactionCheckpoint>,
    pub compaction_boundary: Option<String>,
    pub compaction_target_tokens: Option<usize>,
    pub sources: ContextPack,
}

pub fn enforce_shared_source_budget(resume: &mut ResumeContext, current: &mut ContextPack) {
    let mut seen = BTreeSet::new();
    let mut remaining_files = MAX_CONTEXT_FILES;
    let mut remaining_chars = MAX_CONTEXT_CHARS;

    clamp_source_pack(
        current,
        &mut seen,
        &mut remaining_files,
        &mut remaining_chars,
    );
    clamp_source_pack(
        &mut resume.sources,
        &mut seen,
        &mut remaining_files,
        &mut remaining_chars,
    );
}

fn clamp_source_pack(
    pack: &mut ContextPack,
    seen: &mut BTreeSet<String>,
    remaining_files: &mut usize,
    remaining_chars: &mut usize,
) {
    let mut selected = Vec::new();
    let original_count = pack.source_pointers.len();
    for mut pointer in std::mem::take(&mut pack.source_pointers) {
        if *remaining_files == 0 || *remaining_chars == 0 {
            break;
        }
        if !seen.insert(pointer.stable_ref.clone()) {
            continue;
        }
        pointer.snippet = truncate_chars(&pointer.snippet, (*remaining_chars).min(MAX_FILE_CHARS));
        pointer.chars = pointer.snippet.chars().count();
        if pointer.chars == 0 {
            continue;
        }
        *remaining_files -= 1;
        *remaining_chars -= pointer.chars;
        selected.push(pointer);
    }
    pack.source_pointers = selected;
    pack.files_read = pack.source_pointers.len();
    pack.chars_read = pack
        .source_pointers
        .iter()
        .map(|pointer| pointer.chars)
        .sum();
    pack.dropped_files = pack
        .files_considered
        .max(original_count)
        .saturating_sub(pack.files_read);
}

impl ContextPack {
    pub fn pointer_summary(&self) -> String {
        if self.source_pointers.is_empty() {
            return "없음".to_string();
        }
        self.source_pointers
            .iter()
            .map(|pointer| pointer.stable_ref.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn prompt_section(&self) -> String {
        if self.source_pointers.is_empty() {
            return "repository context:\n- source pointers: 없음\n".to_string();
        }

        let mut section = format!(
            "{} repository context:\n\
             - snippets are context hints, not authority for file modification.\n\
             - before any patch or command action, reread the original source pointer.\n",
            if self.origin == "ontology" {
                "ontology-backed"
            } else {
                "declared-path"
            }
        );
        for pointer in &self.source_pointers {
            section.push_str(&format!(
                "\nsource pointer: {}\nfingerprint: {}\nchars: {}\nsnippet:\n{}\n",
                pointer.stable_ref, pointer.fingerprint, pointer.chars, pointer.snippet
            ));
        }
        section
    }
}

impl ResumeContext {
    pub fn prompt_section(&self) -> String {
        if self.transcript.is_empty()
            && self.compacted_checkpoint.is_none()
            && self.sources.source_pointers.is_empty()
        {
            return String::new();
        }
        let header = format!(
            "durable resumed session context (session={}):\n",
            self.session_id
        );
        let Some(target_tokens) = self.compaction_target_tokens else {
            return self.unbounded_prompt_section(header);
        };
        let mut section = truncate_head_to_tokens(&header, target_tokens);
        let mut remaining = target_tokens.saturating_sub(estimate_tokens(&section));
        if let Some(checkpoint) = &self.compacted_checkpoint {
            let checkpoint_budget = remaining.saturating_mul(50) / 100;
            let checkpoint = truncate_head_to_tokens(
                &format!("\n{}", checkpoint.prompt_section()),
                checkpoint_budget,
            );
            remaining = remaining.saturating_sub(estimate_tokens(&checkpoint));
            section.push_str(&checkpoint);
        }
        let transcript_budget = remaining.saturating_mul(60) / 100;
        let transcript = truncate_tail_to_estimated_tokens(
            &render_transcript(&self.transcript),
            transcript_budget,
        );
        remaining = remaining.saturating_sub(estimate_tokens(&transcript));
        section.push_str(&transcript);
        let sources =
            truncate_head_to_tokens(&format!("\n{}", self.sources.prompt_section()), remaining);
        section.push_str(&sources);
        truncate_head_to_tokens(&section, target_tokens)
    }

    fn unbounded_prompt_section(&self, mut section: String) -> String {
        if let Some(checkpoint) = &self.compacted_checkpoint {
            section.push('\n');
            section.push_str(&checkpoint.prompt_section());
        }
        section.push_str(&render_transcript(&self.transcript));
        section.push('\n');
        section.push_str(&self.sources.prompt_section());
        section
    }

    pub fn summary(&self) -> String {
        format!(
            "context limit={} transcript turns={} tokens={} chars={} compacted={} source pointers={}",
            self.context_limit_tokens,
            self.transcript_turns_selected,
            self.transcript_tokens,
            self.transcript_chars,
            self.compaction_boundary.as_deref().unwrap_or("none"),
            self.sources.files_read
        )
    }
}

fn render_transcript(transcript: &[(String, String)]) -> String {
    let mut section = String::new();
    for (kind, content) in transcript {
        section.push_str(&format!("\n{kind} turn:\n{content}\n"));
    }
    section
}

pub(crate) fn truncate_chars(contents: &str, max_chars: usize) -> String {
    let count = contents.chars().count();
    if count <= max_chars {
        return contents.to_string();
    }
    const MARKER: &str = "\n[truncated]";
    let marker_chars = MARKER.chars().count();
    if max_chars <= marker_chars {
        return MARKER.chars().take(max_chars).collect();
    }
    let prefix = contents
        .chars()
        .take(max_chars - marker_chars)
        .collect::<String>();
    format!("{prefix}{MARKER}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_budget_scales_with_the_declared_model_window() {
        let small = ResumeContextBudget::for_context_limit(4_096);
        let large = ResumeContextBudget::for_context_limit(131_072);

        assert_eq!(small.context_limit_tokens, 4_096);
        assert_eq!(small.transcript_budget_tokens, 512);
        assert_eq!(small.max_turns, 8);
        assert_eq!(large.context_limit_tokens, 131_072);
        assert_eq!(large.transcript_budget_tokens, 16_384);
        assert_eq!(large.per_turn_budget_tokens, 4_096);
        assert_eq!(large.max_turns, 64);
    }

    #[test]
    fn compacted_resume_prompt_honors_one_total_budget_for_korean_content() {
        let korean = "작은 모델이 이어서 수행해야 하는 긴 한국어 컨텍스트 ".repeat(200);
        let checkpoint = CompactionCheckpoint {
            current_task: korean.clone(),
            constraints: (0..8)
                .map(|index| format!("제약 {index} {korean}"))
                .collect(),
            decisions: (0..8)
                .map(|index| format!("결정 {index} {korean}"))
                .collect(),
            remaining_work: (0..8)
                .map(|index| format!("남은 작업 {index} {korean}"))
                .collect(),
            ..CompactionCheckpoint::default()
        };
        let source_pointers = (0..4)
            .map(|index| SourcePointer {
                path: format!("src/file-{index}.rs"),
                stable_ref: format!("src/file-{index}.rs:1"),
                chars: korean.chars().count(),
                fingerprint: "a".repeat(64),
                snippet: korean.clone(),
            })
            .collect::<Vec<_>>();
        let resume = ResumeContext {
            session_id: "session-budget".to_string(),
            context_limit_tokens: 4_096,
            transcript_records_considered: 8,
            transcript_turns_selected: 8,
            transcript_tokens: estimate_tokens(&korean) * 8,
            transcript_chars: korean.chars().count() * 8,
            transcript: (0..8)
                .map(|index| ("user".to_string(), format!("turn-{index} {korean}")))
                .collect(),
            compacted_checkpoint: Some(checkpoint),
            compaction_boundary: Some("record-boundary".to_string()),
            compaction_target_tokens: Some(1_638),
            sources: ContextPack {
                project_root: PathBuf::from("/project"),
                origin: "test".to_string(),
                ontology_records_selected: 0,
                ontology_stale_rejected: 0,
                files_considered: 4,
                files_read: 4,
                chars_read: korean.chars().count() * 4,
                dropped_files: 0,
                source_pointers,
            },
        };

        let prompt = resume.prompt_section();

        assert!(estimate_tokens(&prompt) <= 1_638);
        assert!(prompt.contains("derived compacted checkpoint"));
        assert!(prompt.contains("[compacted]"));
        assert!(prompt.contains("repository context"));
    }

    #[test]
    fn agent_prompt_stays_inside_a_1024_token_runtime_window_with_max_context() {
        let budget = AgentPromptBudget::for_context_limit(1_024, 256).unwrap();
        let resume = "이전 대화와 작업 상태 ".repeat(4_000);
        let repository = "저장소 소스 코드와 검증 근거 ".repeat(4_000);
        let assembled = assemble_agent_prompt(
            budget,
            AgentPromptParts {
                instructions: "필수 runtime 계약: 부작용을 실행하지 말고 한국어로 답합니다.",
                resume_context: &resume,
                repository_context: &repository,
                current_request: "현재 실패 원인을 분석해줘",
                response_cue: "짧고 근거 중심으로 답하고 action contract를 마지막에 기록합니다.",
            },
        )
        .unwrap();

        assert!(assembled.estimated_tokens <= assembled.input_limit_tokens);
        assert!(assembled.text.contains("필수 runtime 계약"));
        assert!(assembled
            .text
            .contains("<RESUME_CONTEXT trust=\"untrusted\">"));
        assert!(assembled
            .text
            .contains("<REPOSITORY_CONTEXT trust=\"untrusted\">"));
        assert!(assembled.text.ends_with(
            "<CURRENT_USER_REQUEST>\n현재 실패 원인을 분석해줘\n</CURRENT_USER_REQUEST>\n\n짧고 근거 중심으로 답하고 action contract를 마지막에 기록합니다."
        ));
    }
}
