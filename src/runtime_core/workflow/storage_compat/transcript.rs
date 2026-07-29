//! Canonical transcript DTO and codec compatibility facade.

#[path = "transcript/decode.rs"]
mod decode;
#[path = "transcript/encode.rs"]
mod encode;
#[path = "transcript/schema.rs"]
mod schema;
#[path = "transcript/types.rs"]
mod types;
#[path = "transcript/validation.rs"]
mod validation;

pub(crate) use decode::parse_record;
pub(crate) use encode::canonical_install_bytes;
#[allow(unused_imports)]
pub(crate) use schema::{
    MAX_TRANSCRIPT_CONTENT_BYTES, TRANSCRIPT_SCHEMA_V1, TRANSCRIPT_SCHEMA_V2, TRANSCRIPT_V2_KEYS,
};
#[allow(unused_imports)]
pub use types::{ToolOutputArtifactBinding, TranscriptRecord, TranscriptSourcePointer};
#[allow(unused_imports)]
pub(crate) use validation::{
    tool_output_artifact_relative_path, validate_id, validate_kind, validate_sha256,
    validate_source_pointer, validate_tool_binding_shape,
};
