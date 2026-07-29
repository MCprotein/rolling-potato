use std::time::{SystemTime, UNIX_EPOCH};

use crate::foundation::error::AppError;
use crate::runtime_core::workflow::domain::transcript as transcript_domain;
use crate::runtime_core::workflow::storage_compat::transcript::{
    self as transcript_codec, TranscriptSourcePointer,
};

pub(in super::super) fn parse_event_details(details: &str) -> Result<Vec<(&str, &str)>, AppError> {
    transcript_domain::parse_event_details(details)
}

pub(in super::super) fn detail_from_pairs<'a>(
    pairs: &'a [(&'a str, &'a str)],
    key: &str,
) -> Option<&'a str> {
    transcript_domain::detail_from_pairs(pairs, key)
}

pub(in super::super) fn validate_event_details_for_schema(
    details: &str,
    schema_version: u64,
) -> Result<(), AppError> {
    transcript_domain::validate_event_details_for_schema(details, schema_version)
}

pub(in super::super) fn validate_kind(kind: &str) -> Result<(), AppError> {
    transcript_codec::validate_kind(kind)
}

pub(in super::super) fn validate_id(label: &str, value: &str) -> Result<(), AppError> {
    transcript_codec::validate_id(label, value)
}

pub(in super::super) fn validate_source_pointer(
    pointer: &TranscriptSourcePointer,
) -> Result<(), AppError> {
    transcript_codec::validate_source_pointer(pointer)
}

pub(in super::super) fn tool_output_artifact_relative_path(
    project_id: &str,
    session_id: &str,
    workflow_id: &str,
    artifact_id: &str,
) -> String {
    transcript_codec::tool_output_artifact_relative_path(
        project_id,
        session_id,
        workflow_id,
        artifact_id,
    )
}

pub(in super::super) fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
