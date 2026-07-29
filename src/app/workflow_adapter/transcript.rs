mod ledger_projection;
mod owner;
mod read_model;
mod recording;
mod storage;
mod tool_turn;

pub(crate) use owner::{record_session_turn, TranscriptOwner};
pub use read_model::{record_from_binding, record_from_event, records_for_session};
pub use recording::record_workflow_turn;
#[cfg(test)]
pub use recording::record_workflow_turn_with_streams;
pub(crate) use tool_turn::{
    decode_prepared_no_stream_tool_turn, install_prepared_no_stream_tool_turn,
    prepare_no_stream_tool_turn, tool_output_view_from_canonical_record, PreparedTranscriptTurn,
};

use ledger_projection::{ensure_ledger_event_under_guard, transcript_ledger_event};

#[cfg(test)]
use crate::adapters::filesystem::layout as paths;
#[cfg(test)]
use crate::app::context_adapter::SourcePointer;
#[cfg(test)]
use crate::app::workflow_adapter::{ledger, state};
#[cfg(test)]
use crate::runtime_core::workflow::storage_compat::transcript::{
    TranscriptRecord, MAX_TRANSCRIPT_CONTENT_BYTES, TRANSCRIPT_SCHEMA_V1, TRANSCRIPT_SCHEMA_V2,
    TRANSCRIPT_V2_KEYS,
};
#[cfg(test)]
use storage::{load_record_path, validate_event_details_for_schema, validated_transcript_path};
#[cfg(test)]
use tool_turn::{sanitize_tool_stream, MAX_SANITIZED_STREAM_BYTES};

#[cfg(test)]
#[path = "transcript/tests.rs"]
mod tests;
