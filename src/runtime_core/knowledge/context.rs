//! Bounded, surface-neutral source and resume context.

use std::collections::BTreeSet;
use std::path::PathBuf;

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
}
