use std::path::PathBuf;

use super::super::compaction::CompactionCheckpoint;

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
