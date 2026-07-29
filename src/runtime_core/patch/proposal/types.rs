use std::path::{Path, PathBuf};

pub(crate) const MAX_PATCH_FILE_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PatchPreview {
    pub proposal_id: String,
    pub approval_token: String,
    pub relative_path: String,
    pub original_sha256: String,
    pub proposed_sha256: String,
    pub replacements: usize,
    pub diff: String,
    pub proposal_path: PathBuf,
    pub proposed_content: String,
    pub workflow_id: String,
    pub action_id: String,
    pub verification_command: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProposalRecord {
    pub proposal_id: String,
    pub approval_token_hash: String,
    pub relative_path: String,
    pub original_sha256: String,
    pub proposed_sha256: String,
    pub proposed_content: String,
    pub proposal_path: PathBuf,
    pub workflow_id: String,
    pub action_id: String,
    pub verification_command: String,
    pub artifact_hash: String,
    pub legacy_plaintext_token: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowProposal {
    pub proposal_id: String,
    pub approval_token: String,
    pub relative_path: String,
    pub original_sha256: String,
    pub proposed_sha256: String,
    pub diff: String,
    pub verification_command: String,
    pub proposal_hash: String,
    pub approval_credential_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchProposalSummary {
    pub proposal_id: String,
    pub relative_path: String,
    pub original_sha256: String,
    pub proposed_sha256: String,
    pub replacements: String,
    pub status: String,
    pub proposal_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchProposalDetail {
    pub summary: PatchProposalSummary,
    pub diff: String,
}

pub(crate) struct PreviewInput<'a> {
    pub relative_path: &'a str,
    pub original: &'a str,
    pub find: &'a str,
    pub replace: &'a str,
    pub workflow_id: &'a str,
    pub action_id: &'a str,
    pub verification_command: &'a str,
    pub approval_token: String,
    pub proposal_dir: &'a Path,
}

pub(crate) enum RecordParse {
    Canonical(Box<ProposalRecord>),
    LegacyMigration { scrubbed: String },
}
