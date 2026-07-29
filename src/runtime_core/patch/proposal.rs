//! Patch proposal domain facade.

mod encoding;
mod preview;
mod record;
mod types;

pub(crate) use preview::build_preview;
pub(crate) use record::{
    parse_header, parse_record, render_record, required_header, validate_proposal_id,
};
pub(crate) use types::{
    PatchPreview, PreviewInput, ProposalRecord, RecordParse, MAX_PATCH_FILE_BYTES,
};
pub use types::{PatchProposalDetail, PatchProposalSummary, WorkflowProposal};

#[cfg(test)]
use encoding::sha256_text;
#[cfg(test)]
#[path = "proposal/tests.rs"]
mod tests;
