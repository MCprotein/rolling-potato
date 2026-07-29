use crate::foundation::error::AppError;

use super::{ImportValidation, SCHEMA_VERSION, SOURCE_POINTER_NONE};

pub(crate) fn validate_import_text(text: &str) -> Result<ImportValidation, AppError> {
    let schema_version = extract_json_u64_tolerant(text, "schemaVersion").ok_or_else(|| {
        AppError::usage("ontology import file에는 schemaVersion: 1이 필요합니다.")
    })?;
    if schema_version != u64::from(SCHEMA_VERSION) {
        return Err(AppError::usage(format!(
            "ontology import schemaVersion은 {}이어야 합니다: {}",
            SCHEMA_VERSION, schema_version
        )));
    }

    let mut records = 0;
    for line in text.lines().filter(|line| line.contains("\"id\"")) {
        records += 1;
        let layer = extract_json_string_tolerant(line, "layer").unwrap_or_default();
        let status = extract_json_string_tolerant(line, "status").unwrap_or_default();
        let claim_state = extract_json_string_tolerant(line, "claimState").unwrap_or_default();
        let source_pointer =
            extract_json_string_tolerant(line, "sourcePointer").unwrap_or_default();
        let source_hash = extract_json_string_tolerant(line, "sourceHash").unwrap_or_default();
        if layer == "B"
            && (status == "confirmed" || claim_state == "confirmed")
            && (source_pointer.trim().is_empty()
                || source_pointer == SOURCE_POINTER_NONE
                || source_hash.trim().is_empty())
        {
            return Err(AppError::blocked(
                "ontology import 차단: confirmed Layer B semantic claim에는 sourcePointer와 sourceHash가 필요합니다.",
            ));
        }
    }

    if records == 0 {
        records = text.matches("\"schemaVersion\"").count().saturating_sub(1);
    }
    Ok(ImportValidation { records })
}

fn extract_json_string_tolerant(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let start = text.find(&needle)? + needle.len();
    let rest = text[start..].trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    super::codec::parse_json_string_tail(rest)
}

fn extract_json_u64_tolerant(text: &str, key: &str) -> Option<u64> {
    let needle = format!("\"{key}\"");
    let start = text.find(&needle)? + needle.len();
    let rest = text[start..].trim_start().strip_prefix(':')?.trim_start();
    rest.chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()
}
