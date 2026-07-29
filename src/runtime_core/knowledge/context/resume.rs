use super::super::compaction::{
    estimate_tokens, truncate_head_to_tokens, truncate_tail_to_estimated_tokens,
};
use super::types::ResumeContext;

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
